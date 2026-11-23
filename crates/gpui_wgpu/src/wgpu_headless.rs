//! A [`gpui::PlatformHeadlessRenderer`] over wgpu, for rendering a [`gpui::Scene`] to
//! an image with no window, compositor or display connection.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui::{DevicePixels, PlatformAtlas, PlatformHeadlessRenderer, Scene, Size};

use crate::{WgpuAtlas, WgpuContext, WgpuRenderer, WgpuSurfaceConfig};

/// Renders scenes into an offscreen wgpu texture and reads them back.
///
/// The Vulkan instance is created without a display handle, so no X11, Wayland,
/// EGL or desktop D-Bus connection is made.
pub struct WgpuHeadlessRenderer {
    context: &'static WgpuContext,
    renderer: Option<WgpuRenderer>,
    atlas: Arc<WgpuAtlas>,
}

thread_local! {
    static CONTEXT: std::cell::Cell<Option<&'static WgpuContext>> = const { std::cell::Cell::new(None) };
}

/// One GPU device per thread: a preview opens a headless window per cell, and
/// creating a Vulkan instance and device for each would dominate the render.
///
/// The context is leaked on purpose. Dropping a Vulkan device from a
/// thread-local destructor runs after other thread-locals it needs are gone and
/// aborts the process at thread exit; the operating system reclaims it at exit.
fn shared_context() -> Result<&'static WgpuContext> {
    if let Some(context) = CONTEXT.get().filter(|context| !context.device_lost()) {
        return Ok(context);
    }
    let context: &'static WgpuContext = Box::leak(Box::new(
        WgpuContext::new_offscreen()
            .context("failed to create an offscreen GPU context")?,
    ));
    CONTEXT.set(Some(context));
    Ok(context)
}

impl WgpuHeadlessRenderer {
    pub fn new() -> Result<Self> {
        let context = shared_context()?;
        let atlas = Arc::new(WgpuAtlas::from_context(context));
        Ok(Self {
            context,
            renderer: None,
            atlas,
        })
    }

    /// Name of the GPU adapter drawing the frames.
    pub fn adapter_name(&self) -> String {
        self.context.adapter.get_info().name
    }

    fn renderer_for(&mut self, size: Size<DevicePixels>) -> Result<&mut WgpuRenderer> {
        anyhow::ensure!(
            size.width.0 > 0 && size.height.0 > 0,
            "invalid size for an offscreen render: {size:?}"
        );
        match &mut self.renderer {
            Some(renderer) => renderer.update_drawable_size(size),
            None => {
                self.renderer = Some(WgpuRenderer::new_offscreen_with_atlas(
                    self.context,
                    WgpuSurfaceConfig {
                        size,
                        transparent: false,
                        preferred_present_mode: None,
                    },
                    self.atlas.clone(),
                )?);
            }
        }
        Ok(self.renderer.as_mut().expect("renderer"))
    }

    fn draw(&mut self, scene: &Scene, size: Size<DevicePixels>) -> Result<&mut WgpuRenderer> {
        // The atlas is already shared with the window, so a lost device can't be replaced under
        // it. Say so rather than failing every later frame in some obscure way.
        anyhow::ensure!(
            !self.context.device_lost(),
            "the GPU device was lost; restart the preview process"
        );
        let renderer = self.renderer_for(size)?;
        // `draw` reports false when it skipped the frame (for example after an error it logged);
        // retry a few times, then fail rather than keep a stale or blank frame. Errors that only
        // surface after submission are caught when the frame is read back.
        for _ in 0..3 {
            if renderer.draw(scene) {
                return Ok(renderer);
            }
        }
        anyhow::bail!("the offscreen renderer could not draw the scene")
    }
}

impl PlatformHeadlessRenderer for WgpuHeadlessRenderer {
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> Result<image::RgbaImage> {
        let renderer = self.draw(scene, size)?;
        let (width, height, mut pixels) = renderer.read_offscreen_frame()?;
        // The texture size is clamped to the device limit; a smaller image than requested would
        // silently misrepresent the view.
        anyhow::ensure!(
            (i64::from(width), i64::from(height)) == (i64::from(size.width.0), i64::from(size.height.0)),
            "offscreen frame is {width}x{height} but {size:?} was requested (device texture limit?)"
        );
        // The target is Bgra8Unorm; images are RGBA.
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        image::RgbaImage::from_raw(width, height, pixels)
            .context("offscreen frame did not match its size")
    }

    fn render_scene(&mut self, scene: &Scene, size: Size<DevicePixels>) -> Result<()> {
        self.draw(scene, size).map(|_| ())
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.atlas.clone()
    }
}

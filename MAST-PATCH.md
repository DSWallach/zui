# Mast patch on top of gaelcado/zui@53869c204efb866d19af292cab8b1773898eeedd

Vendored so the headless UI driver can read an accessibility tree and element
bounds. Everything is `cfg(test)` / `cfg(feature = "test-support")` (the
`TestPlatform` and `HeadlessAppContext` types it touches already are), and off
by default even there. The installed (`--profile installed`) build never enables
`gpui/test-support` (Mast tracks this in scripts/check-installed-clean.sh), so none of this compiles into it.

This branch is rev 53869c2 plus one commit,
so `git show` is the full change.

- `platform/test/platform.rs`: `TestPlatform.a11y_enabled` flag.
- `platform/test/window.rs`: `TestWindow::a11y_init` runs the activation
  callback when the flag is set (what a connecting screen reader does).
- `app/headless_app_context.rs`: `HeadlessAppContext::set_a11y_enabled`.
- `window.rs`: `Window::debug_bounds_all` returns the `debug_selector` bounds map.

# RAKE-006: Runtime window and webview

**Milestone:** M1  
**Depends on:** RAKE-002

## Scope

`rake-runtime` binary: tao event loop, one window, one wry webview filling it, loads the configured URL. Handles close and window resize.

## Acceptance criteria

- [ ] Running with a hardcoded test config opens the site in a window.
- [ ] Resizing the window resizes the webview with no gaps.
- [ ] Release binary has no GPUI, HTTP client or image crates in `cargo tree`.

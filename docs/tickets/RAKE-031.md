# RAKE-031: Linux template and packaging

**Milestone:** M5  
**Depends on:** RAKE-022

## Scope

WebKitGTK template on X11, `.desktop` file and png icon, `.deb` and tarball output. Test native Wayland separately via wry's GTK path.

## Acceptance criteria

- [x] App runs on Ubuntu LTS under X11.
- [x] Wayland behaviour documented, working or not.

Verified on Ubuntu 24.04 arm64 and emulated x86_64 in OrbStack containers with `python3 tests/linux_smoke.py`. The test builds tarball and `.deb` outputs, installs the Debian package, and captures the installed app rendering a local page under Xvfb. Native Wayland loads the extracted app's page and runs JavaScript under headless Weston with `DISPLAY` unset. See [Linux verification](../cli-usage.md#linux-verification) for the environment and limits.

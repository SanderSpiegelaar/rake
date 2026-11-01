<img src="assets/icon.svg" width="64" height="64" alt="">

# Rake

Turn a website into a macOS, Linux, or Windows app using the system webview. Rake copies a prebuilt template, so building an app needs no Rust, Node, or Xcode installation.

## Install

Rake runs on Apple Silicon and Intel Macs, and on Linux x86_64 and arm64. Linux apps require WebKitGTK 4.1 and GTK 3. On Ubuntu 24.04, install them with `sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0t64 xdg-utils`. The CLI also uses `curl` and `tar`, plus `dpkg-deb` for Debian packages.

The CLI needs network access for its first template download and for site icons.

Windows x86_64 apps use the Microsoft Edge WebView2 Runtime. Windows 11 includes it. Other Windows installations need the [Evergreen WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/). The CLI uses Windows PowerShell, `curl.exe`, and `tar.exe`, available on Windows 10 and 11. Windows 11 execution still needs manual verification.

No release is published yet. These steps apply once a release includes both the CLI and its matching template.

1. Open [GitHub Releases](https://github.com/SanderSpiegelaar/rake/releases). Download `rake` on macOS, or `rake-linux-x86_64` or `rake-linux-arm64` on Linux. Save the binary as `~/Downloads/rake`.
2. Install the binary into your user account:

   ```sh
   mkdir -p "$HOME/.local/bin"
   cp "$HOME/Downloads/rake" "$HOME/.local/bin/rake"
   chmod +x "$HOME/.local/bin/rake"
   export PATH="$HOME/.local/bin:$PATH"
   rake --version
   ```

   Add `export PATH="$HOME/.local/bin:$PATH"` to your shell's startup file to keep the command available in new terminals.

Rake and generated apps are not notarized. If macOS blocks a trusted download, attempt to open it, then use **System Settings > Privacy & Security > Open Anyway**. Do not disable Gatekeeper.

On Windows, download `rake-windows-x86_64.exe` from the same release and rename it to `rake.exe`. Run it from PowerShell or add its directory to `PATH`. Windows executables are unsigned, so SmartScreen may warn about them.

## Quick start

Build an app in a new directory, then open it:

```sh
mkdir -p "$HOME/Rake Apps"
cd "$HOME/Rake Apps"
rake https://example.com --name Example
open Example.app
```

Rake writes `Example.app` in the current directory and prints its path to stderr. The first build downloads and verifies the version-matched template. Later builds reuse the cache. If no site icon works, Rake keeps the default icon and prints a warning.

On Linux, the same build writes an `Example` directory. Run `./Example/rake-runtime` to open it. Use `rake https://example.com --name Example --target tarball --force` for an archive, or `--target deb` for a Debian package. Install the package with `sudo apt install ./Example.deb`.

On Windows, run `rake.exe https://example.com --name Example --icon icon.png --target zip`. Rake writes `Example/Example.exe`, its `config.json`, and `Example.zip`. Extract the ZIP and launch `Example.exe`. Use a PNG up to 256 pixels per side or an ICO for the custom icon.

To rebuild the same app, add `--force`:

```sh
rake https://example.com --name Example --force
```

See [CLI usage](docs/cli-usage.md) for every flag, JSON results, exit codes, and config files.

## Development

With Rust and the macOS developer tools or Linux WebKitGTK development packages installed, run the checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The tests check that every flag in `rake --help` appears in the CLI reference.

The desktop builder remains macOS-only. Linux development needs `build-essential`, `pkg-config`, `libwebkit2gtk-4.1-dev`, and `libxdo-dev`. Run `python3 tests/linux_smoke.py` with Xvfb, ImageMagick, Weston, Python, and `libgdk-pixbuf2.0-bin` installed to check X11 and native Wayland. The smoke test installs its proof package through `dpkg`, so use a disposable container or VM.

Windows development requires Rust's MSVC toolchain and Visual Studio C++ Build Tools. Run `python tests/windows_smoke.py` with Python and WebView2 installed. CircleCI uses Windows Server 2022, not Windows 11. See [Windows packaging](docs/cli-usage.md#windows-packaging) for icon resource decisions and verification limits.

See the [PRD](docs/PRD.md) and [roadmap](docs/ROADMAP.md) for scope and planned platforms.

Licensed under [MIT](LICENSE).

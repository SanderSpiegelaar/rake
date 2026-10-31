<img src="assets/icon.svg" width="64" height="64" alt="">

# Rake

Turn a website into a macOS app using the system webview. Rake copies a prebuilt template, so building an app needs no Rust, Node, or Xcode installation.

## Install

Rake runs on Apple Silicon and Intel Macs. The CLI needs network access for its first template download and for site icons.

No release is published yet. These steps apply once a release includes both the CLI and its matching template.

1. Open [GitHub Releases](https://github.com/SanderSpiegelaar/rake/releases). Download the `rake` CLI binary, a universal binary for both architectures, and save it as `~/Downloads/rake`. If the asset is an archive, extract the `rake` binary first.
2. Install the binary into your user account:

   ```sh
   mkdir -p "$HOME/.local/bin"
   cp "$HOME/Downloads/rake" "$HOME/.local/bin/rake"
   chmod +x "$HOME/.local/bin/rake"
   export PATH="$HOME/.local/bin:$PATH"
   rake --version
   ```

   Add `export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc` to keep the command available in new terminals.

Rake and generated apps are not notarized. If macOS blocks a trusted download, attempt to open it, then use **System Settings > Privacy & Security > Open Anyway**. Do not disable Gatekeeper.

## Quick start

Build an app in a new directory, then open it:

```sh
mkdir -p "$HOME/Rake Apps"
cd "$HOME/Rake Apps"
rake https://example.com --name Example
open Example.app
```

Rake writes `Example.app` in the current directory and prints its path to stderr. The first build downloads and verifies the version-matched template. Later builds reuse the cache. If no site icon works, Rake keeps the default icon and prints a warning.

To rebuild the same app, add `--force`:

```sh
rake https://example.com --name Example --force
```

See [CLI usage](docs/cli-usage.md) for every flag, JSON results, exit codes, and config files.

## Development

With Rust and the macOS developer tools installed, run the checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The tests check that every flag in `rake --help` appears in the CLI reference.

See the [PRD](docs/PRD.md) and [roadmap](docs/ROADMAP.md) for scope and planned platforms.

Licensed under [MIT](LICENSE).

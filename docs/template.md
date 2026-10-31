# Template bundle

On a Mac with Rust and Xcode command line tools installed:

```sh
cargo xtask template universal
cd target/template
shasum -a 256 -c SHA256SUMS
tar -xzf rake-template-0.1.0-macos-universal.tar.gz
printf '{"url":"https://example.com"}\n' > Rake.app/Contents/Resources/config.json
open Rake.app
```

The task defaults to the host architecture. Pass `arm64`, `x86_64`, or `universal`
to select an architecture, such as `cargo xtask template universal`.
Universal builds combine both Rust target executables with `lipo` before signing.
Both Rust targets are installed by `rust-toolchain.toml`.

The task builds only `rake-runtime` with the workspace release profile. It writes
`target/template/rake-template-<version>-macos-<arch>.tar.gz` and `SHA256SUMS`.
The version is inherited from `[workspace.package]`. `SHA256SUMS` contains one
sha256sum-compatible line per template archive of that version in the output directory:
`<64 lowercase hex characters>  <archive filename>`.
The filename is relative to `SHA256SUMS`, with two spaces after the digest.

The archive contains a single `Rake.app`, with executable
`Contents/MacOS/rake-runtime`, placeholder metadata in `Contents/Info.plist`, and
`Contents/Resources/icon.icns`. The plist allows plain `http://` loads in web content,
because configs may point at http sites. Resources otherwise starts empty: no config,
injected scripts, or site assets. The monochrome icon is a replaceable placeholder.
The task ad-hoc signs the bundle before packaging. Builders must re-sign after
changing its contents for distribution. No Developer ID signing or notarization
is provided.

Repeated builds replace the staged bundle, including any test config added to it.
The task uses `cargo`, `codesign`, `tar`, `shasum`, and `lipo`, with no added dependencies.
The release workflow publishes all three template archives and a universal `rake`
CLI binary, with one `SHA256SUMS` covering all four assets.

## Release size choice

The release profile uses LTO, stripped symbols, one codegen unit, and
`opt-level = "z"`. On Apple Silicon, the same runtime and lockfile produced a 939,984-byte
binary with `"s"` and an 825,888-byte binary with `"z"` (before bundle signing).
`"z"` saved 114,096 bytes, so it is the default. Reproduce the comparison with:

```sh
CARGO_PROFILE_RELEASE_OPT_LEVEL=s cargo build --release --locked -p rake-runtime --target aarch64-apple-darwin --target-dir target/opt-s
CARGO_PROFILE_RELEASE_OPT_LEVEL=z cargo build --release --locked -p rake-runtime --target aarch64-apple-darwin --target-dir target/opt-z
stat -f '%z %N' target/opt-{s,z}/aarch64-apple-darwin/release/rake-runtime
```

The signed template contains 824,629 bytes of files. The measured gzip archive
is 395,319 bytes. Archive size and checksum can vary with timestamps and signing.

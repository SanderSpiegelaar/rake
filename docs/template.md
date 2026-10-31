# Template bundle

On an Apple Silicon Mac with Rust and Xcode command line tools installed:

```sh
cargo xtask template
cd target/template
shasum -a 256 -c SHA256SUMS
tar -xzf rake-template-0.1.0-macos-arm64.tar.gz
printf '{"url":"https://example.com"}\n' > Rake.app/Contents/Resources/config.json
open Rake.app
```

The task builds only `rake-runtime` with the workspace release profile. It writes
`target/template/rake-template-<version>-macos-arm64.tar.gz` and `SHA256SUMS`.
The version is inherited from `[workspace.package]`. `SHA256SUMS` contains one
sha256sum-compatible line: `<64 lowercase hex characters>  <archive filename>`.
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
The task uses `cargo`, `codesign`, `tar`, and `shasum`, with no added dependencies.

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

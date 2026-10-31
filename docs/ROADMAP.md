# Rake: implementation roadmap

The core library exists from M0, so the CLI stays a thin wrapper from its first line. Requirements live in [PRD.md](PRD.md).

| Milestone | Scope |
|---|---|
| M0: Repo and config | Workspace, CI, license, and the config schema every other crate depends on. |
| M1: Template app | Prebuilt arm64 `.app` that opens any URL from `config.json` with no recompile. |
| M2: Core and CLI | Template download and cache, config injection, icon, `Info.plist`, signing, `.app` and DMG output. |
| M3: Release v0.1 | Tests, docs, published template and CLI releases. |
| M4: GPUI builder app | Form over the core library. Live preview comes after M4. |
| M5: More platforms | Intel and universal macOS, Linux, Windows. |

## M0: Repo and config

1. Cargo workspace with pinned dependency versions and an MIT `LICENSE` file.
2. `rake-config` crate: the config struct with serde, defaults, validation, and a generated JSON Schema in `schema/`.
3. CI on macOS arm64: format check, clippy, tests.

**Done when:** a config JSON round-trips, and invalid input gives a clear error.

## M1: Template app

1. `rake-runtime` binary: one tao window with one wry webview.
2. Load `config.json` from `Contents/Resources`, found relative to the running executable.
3. Apply title, size, minimum size, user agent, dark mode, and hidden title bar.
4. Inject CSS and JS as initialization scripts on every page load.
5. Navigation: safe domains stay in-app, every other URL opens in the default browser.
6. Devtools only when debug is on.
7. Benchmark against a Pake app of the same site: bundle size, cold start, idle memory.

**Done when:** editing `config.json` alone changes the wrapped site and window, and the app is no larger than Pake's.

## M2: Core and CLI

1. Template fetch: download the template for the current OS, architecture and Rake version from GitHub Releases, verify it against `SHA256SUMS`, and cache it.
2. Assemble: copy the template, write `config.json` and inject files into `Resources`.
3. Icon: use `--icon` or fetch the site's icon, convert it to `.icns`.
4. `Info.plist`: set name, bundle ID, version and icon file.
5. Sign: ad-hoc re-sign after every change to the bundle.
6. Package: `.app`, DMG, and `--install` into `/Applications`.
7. `rake-cli`: flags, `--config` with flags overriding the file, `--json` output, exit codes 0 to 4.

**Done when:** a cached build takes under 10 seconds and the app opens on a second Mac.

## M3: Release v0.1

1. Release workflow: build the template, upload it with `SHA256SUMS`, then publish the CLI binary for the same version.
2. End-to-end test: build an app from a fixture config and launch it in CI.
3. `docs/cli-usage.md` and a README with install steps.

**Done when:** a fresh Mac goes from download to a working app with no toolchain installed.

## M4: GPUI builder app

1. Pin one GPUI source and version: git rev or `gpui-pre`.
2. Form for every P0 option, with defaults from `rake-config`.
3. Icon picker with favicon fetch.
4. Build button that calls `rake-core` on a background thread, shows progress, then reveals the app in Finder.
5. Import and export config JSON.

**Done when:** every app the CLI can build can also be built from the form.

## M5: More platforms

1. macOS Intel and universal templates.
2. Linux: X11 template, `.deb` and tarball. Native Wayland needs wry's GTK path, so test it separately.
3. Windows: WebView2 template, exe icon swap, ZIP.
4. Then the P1 options: tray, multi-window, global shortcut, and the rest.

## Repo layout

```text
rake/
├── Cargo.toml                 # workspace root, pinned shared dependencies
├── LICENSE                    # MIT
├── crates/
│   ├── rake-config/           # config struct, defaults, validation, JSON Schema
│   ├── rake-runtime/          # template app: tao window + wry webview, reads config.json
│   ├── rake-core/             # build pipeline: fetch, verify, cache, assemble, icon, plist, sign, package
│   ├── rake-cli/              # thin CLI over rake-core, builds the `rake` binary
│   └── rake-builder/          # GPUI desktop app over rake-core (M4)
├── schema/
│   └── rake.schema.json       # generated from rake-config, never edited by hand
├── xtask/                     # dev tasks: build template bundle, generate schema, package releases
├── tests/
│   └── fixtures/              # sample configs and icons for end-to-end tests
├── docs/                      # PRD, roadmap, cli-usage.md
└── .circleci/
    └── config.yml             # ci workflow (checks, smoke), release workflow (template + CLI)
```

- Only `rake-runtime` goes into generated apps, so it must stay small: no GPUI, no HTTP client, no image libraries. Those belong in `rake-core`.
- The template and the CLI share one version number, so a CLI downloads exactly the template it was released with.

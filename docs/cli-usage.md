# CLI usage

```text
rake [OPTIONS] [URL]
```

Rake builds apps for the host OS. macOS supports arm64, x86_64, and universal apps. Linux supports arm64 and x86_64. `URL` must be an HTTP or HTTPS URL with a host. It is required unless `--config` supplies it. A positional URL overrides the config file's `url`.

## Flags and defaults

Defaults below apply without a config file. Explicit flags override file values. Omitted flags preserve file values.

| Flag | Default | Behavior |
|---|---|---|
| `--config <FILE>` | None | Read a Rake JSON config file. |
| `--name <NAME>` | URL host | Set the app bundle and menu name. Must be a nonempty filename without `/`, `\`, `:`, or NUL, and not `.` or `..`. Linux also rejects control characters. |
| `--identifier <IDENTIFIER>` | Derived from name and URL | Set a reverse-DNS bundle ID. The default is `com.rake.<name-slug>-<url-hash>`, with an 8-character hash. |
| `--icon <ICON>` | Fetch site icon | Use a local image or HTTP or HTTPS image URL. macOS converts to ICNS and rejects invalid explicit icons. Linux copies PNG icons and uses `gdk-pixbuf-thumbnailer` for other formats when available. Unusable Linux icons and failed automatic discovery keep the template icon with a warning. |
| `--width <WIDTH>` | `1200` | Initial logical window width, a positive integer. |
| `--height <HEIGHT>` | `780` | Initial logical window height, a positive integer. |
| `--min-width <MIN_WIDTH>` | Unset | Minimum logical window width, a nonnegative integer no larger than `width`. |
| `--min-height <MIN_HEIGHT>` | Unset | Minimum logical window height, a nonnegative integer no larger than `height`. |
| `--title <TITLE>` | App name | Set the window title. |
| `--user-agent <USER_AGENT>` | System webview user agent | Override the webview user agent. |
| `--hide-title-bar[=<BOOL>]` | `false` | Hide the macOS title and extend page content into the title bar. The top 28 CSS pixels support dragging outside interactive elements. |
| `--dark-mode[=<BOOL>]` | `false` | Request a dark window appearance. Without it, use the system appearance. This does not rewrite the site's CSS. |
| `--debug[=<BOOL>]` | `false` | Enable and open devtools, and log config and page loads to stderr. |
| `--inject <FILE>` | Empty list | Repeat for each `.css` or `.js` file. Copy files into the app and inject them on every page load, in list order. Basenames must be unique. |
| `--safe-domain <DOMAIN>` | Empty extra list | Repeat for each extra bare hostname allowed in-app. The starting URL's host is always allowed. Allowed hosts include their subdomains. Other HTTP and HTTPS hosts open in the default browser. |
| `--target <TARGET>` | `app` | macOS accepts `app` and `dmg`. Linux accepts `app`, `tarball`, and `deb`. Every target builds the app directory. `dmg` adds a compressed disk image with an Applications link. `tarball` adds a `.tar.gz` archive. `deb` adds a Debian package. Other host and target combinations fail before downloading. |
| `--arch <ARCH>` | Host architecture | Use the host OS template for `arm64` or `x86_64`. macOS also accepts `universal`, with both executable slices. Linux rejects `universal`. Independent of `--target`. |
| `--install` | `false` | macOS only. Copy the built app into `/Applications`. Requires write access there. Replaces only a Rake-built app with the same bundle ID. On Linux, install a `.deb` with `sudo apt install ./Example.deb`. |
| `--force` | `false` | Replace an existing output app directory, `.dmg`, `.tar.gz`, or `.deb`. Does not bypass installation safety checks. |
| `--json` | `false` | Write one JSON result to stdout. Diagnostics and the output path remain on stderr. |
| `-h`, `--help` | No action | Print help to stderr and exit successfully. |
| `-V`, `--version` | No action | Print CLI and template versions to stderr and exit successfully. |

Boolean value flags accept `true` or `false`. A bare `--dark-mode`, `--debug`, or `--hide-title-bar` means `true`. Explicit values require `=`, such as `--dark-mode=false`.

Repeated `--inject` or `--safe-domain` flags replace the corresponding config list rather than append to it. CLI file paths resolve from the current directory. Local icon and injection paths in a config file resolve from that file's directory.

The builder's **Import** loads this config format. **Export** saves local icon and injection paths as absolute paths, so saving the config in another directory does not change which files the CLI uses. Installation remains a CLI or builder action, not a config field.

Output files go into the current directory. macOS writes `<Name>.app` and, for `--target dmg`, `<Name>.dmg`. The macOS app is ad-hoc signed after assembly. It is not Developer ID signed or notarized. There is no output-directory flag.

Linux writes `<Name>/` with `rake-runtime`, `config.json`, `icon.png`, injection files, and an `<identifier>.desktop` file. `--target tarball` also writes `<Name>.tar.gz`. Extract it and run `<Name>/rake-runtime`. The directory's desktop entry points to its original build location. Use the runtime directly after moving the directory. `--target deb` also writes `<Name>.deb`, installing the app under `/opt/<lowercase-identifier>` and a desktop entry and icon under `/usr/share`. Debian packages depend on WebKitGTK 4.1, GTK 3, and `xdg-utils`. Linux apps are not signed. The Linux template is built on Ubuntu 24.04. Older distributions are not tested.

Release templates require glibc 2.39 or newer. The Debian package declares this minimum.

Linux icon conversion uses Ubuntu's `libgdk-pixbuf2.0-bin` tools and installed image loaders. Without them, PNG icons still work. Unsupported formats keep the default icon and produce a warning. Generated apps do not need the converter.

The template cache is `$HOME/Library/Caches/rake/templates/<version>/macos-<arch>/Rake.app`. On a cache miss, Rake downloads `rake-template-<version>-macos-<arch>.tar.gz` and `SHA256SUMS` from the matching `v<version>` GitHub Release, checks the archive's SHA-256, and extracts it. There is no CLI cache-refresh flag.

On Linux, the cache is `$XDG_CACHE_HOME/rake/templates/<version>/linux-<arch>/Rake`, defaulting to `$HOME/.cache/rake/templates/<version>/linux-<arch>/Rake`. The archive is `rake-template-<version>-linux-<arch>.tar.gz`, checked against the same release's `SHA256SUMS`. Linux uses `sha256sum`; macOS uses `shasum`.

## Linux verification

`python3 tests/linux_smoke.py` builds the template and CLI, uses a local cached template, produces both package formats, installs the `.deb` with `dpkg -i`, and launches the installed app under Xvfb. It serves a local page, waits for a JavaScript callback, and saves `x11.png`. It also extracts the tarball and launches that runtime on native Wayland. Template download and checksum checks run against a local HTTP server in `cargo test --workspace`.

On Ubuntu 24.04 arm64 and x86_64 with WebKitGTK 2.52.6, GTK 3.24.41, and Weston 13.0, the installed app renders the page under X11. Native Wayland also loads the page and executes JavaScript with `GDK_BACKEND=wayland` and `DISPLAY` unset. The runtime uses `WebViewBuilderExtUnix::build_gtk` with tao's default GTK box on both backends. Weston runs with `--backend=headless`, without Xwayland. Both runs use OrbStack containers on an Apple Silicon Mac, with x86_64 emulated.

The container run uses software rendering. When run as root, the smoke test disables WebKit's sandbox because container kernels can block nested user namespaces. This setting exists only in the test environment, not in generated apps. These checks do not cover a full GNOME or KDE session, GPU acceleration, or the production sandbox. The test writes `wayland.log`, `weston.log`, and `wayland-result.txt` alongside the screenshot in `target/linux-smoke`. Set `RAKE_SMOKE_ARTIFACTS` to save them elsewhere. The test installs a proof package, so run it in a disposable container or VM.

## JSON output

With `--json`, stdout contains one newline-terminated JSON object, including on validation and argument parsing errors. Without `--json`, stdout is empty. Object key order is not part of the format.

Success example:

```json
{
	"status": "success",
	"app_path": "/Users/alex/Rake Apps/Example.app",
	"duration": 1.25,
	"warnings": [],
	"error": null
}
```

Failure example:

```json
{
	"status": "error",
	"app_path": null,
	"duration": 0.001,
	"warnings": [],
	"error": "url: must be an http or https URL, got `invalid`"
}
```

- `status` is `success` for exit code 0, otherwise `error`.
- `app_path` is the absolute local app directory path on build success, even with a package target or `--install`. It is `null` on failure.
- `duration` is elapsed wall-clock seconds as a JSON number.
- `warnings` is an array of strings. A warning does not by itself fail a build.
- `error` is a string on failure, otherwise `null`.

`--json --help` and `--json --version` return a success object with `app_path: null` because they do not build an app. `--json` must appear before the `--` argument separator to select JSON output. If stdout cannot be written, the process exits with code 1 and cannot guarantee a JSON result.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success, including help and version requests. |
| `1` | Unexpected failure, such as resolving the current directory or writing JSON output. |
| `2` | Invalid input, argument parsing failure, unreadable or malformed config, or refused output overwrite. |
| `3` | Build or packaging failure, including filesystem errors and failed signing, icon, or disk-image commands. |
| `4` | Missing environment or unavailable template. All template-fetch failures, including download and checksum errors, map to this code. |

## Config file format

The file is a JSON object with snake_case keys. `url` is the only required key. Unknown keys are rejected. The [JSON Schema](../schema/rake.schema.json) describes the file format. `schema_version` defaults to `1` and has no CLI flag.

Config fields use the flag defaults above. Optional string and minimum-size fields can be omitted or `null`. Boolean fields default to `false`; `inject` and `safe_domains` default to empty arrays. `target`, `install`, `force`, and `json` are CLI-only options, not config keys.

Full example for `example.json`:

```json
{
	"schema_version": 1,
	"url": "https://example.com",
	"name": "Example",
	"identifier": "org.example.rake",
	"icon": "icon.png",
	"width": 1200,
	"height": 780,
	"min_width": 640,
	"min_height": 480,
	"title": "Example",
	"user_agent": "Rake/0.1.0",
	"hide_title_bar": false,
	"dark_mode": false,
	"debug": false,
	"inject": ["style.css", "script.js"],
	"safe_domains": ["login.example.org"]
}
```

This example references `icon.png`, `style.css`, and `script.js` next to `example.json`; those files must exist. Injection files must be UTF-8. The identifier needs at least two dot-separated, nonempty segments containing only ASCII letters, digits, or hyphens. Safe domains cannot contain schemes, paths, ports, wildcards, whitespace, or IP addresses.

A minimal config needs no extra files:

```json
{"url": "https://example.com", "name": "Example"}
```

The command `rake --config example.json --width 1440 --dark-mode --target dmg --json` uses the file with a width and appearance override. Other file values remain unchanged.

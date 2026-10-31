# CLI usage

```sh
rake https://example.com --name Example
rake --config app.json --arch universal
rake https://example.com --arch x86_64 --target dmg
```

The URL is required unless `--config` supplies it. Flags override config values.
Local icon and inject paths in a config file are relative to that file.
Paths passed as flags are relative to the working directory.

## Options

| Flag | Value or behavior |
|---|---|
| `--config` | JSON config file. |
| `--name` | App name. Defaults to the URL hostname. |
| `--identifier` | Bundle identifier. Defaults to a value derived from the name and URL. |
| `--icon` | Local image path or HTTP image URL. Defaults to the site's icon. |
| `--width`, `--height` | Window size. Defaults to 1200 by 780. |
| `--min-width`, `--min-height` | Minimum window size. |
| `--title` | Window title. |
| `--user-agent` | Webview user agent. |
| `--hide-title-bar` | Hide the title bar. |
| `--dark-mode` | Force dark appearance. |
| `--debug` | Enable devtools and runtime logging. |
| `--inject` | CSS or JS file. Repeat for multiple files. |
| `--safe-domain` | Hostname that stays in the app. Repeat for multiple hostnames. |
| `--target` | Output format, `app` or `dmg`. Defaults to `app`. |
| `--arch` | Template architecture, `arm64`, `x86_64`, or `universal`. Defaults to the running CLI's architecture. |
| `--install` | Install the app into `/Applications`. |
| `--force` | Replace existing app or DMG output. |
| `--json` | Write one JSON result to stdout. Diagnostics go to stderr. |
| `--help` | Print help. |
| `--version` | Print CLI and template versions. |

Boolean config overrides accept `=false`, such as `--dark-mode=false`.
Architecture and output format are build options, not runtime config fields.
`--arch universal` produces an app with both arm64 and x86_64 executable slices.
It works with either `--target app` or `--target dmg`.

Templates are version-matched release assets named
`rake-template-<version>-macos-<arch>.tar.gz`, verified against `SHA256SUMS`.
The cache is `~/Library/Caches/rake/templates/<version>/macos-<arch>/Rake.app`.

## JSON and exit codes

The JSON result has `status`, `app_path`, `duration` in seconds, `warnings`, and
`error` fields. `app_path` is the app bundle path even when the output is a DMG.

| Code | Meaning |
|---|---|
| 0 | Success. |
| 1 | Unexpected error. |
| 2 | Invalid input. |
| 3 | File or command failure during assembly or packaging. |
| 4 | Template or environment unavailable. |

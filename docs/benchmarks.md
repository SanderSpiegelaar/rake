# Rake and Pake benchmark

Measured on 2026-10-31. Rake meets the bundle-size target.

## Environment

- MacBook Pro, Apple M5 Pro, 18 CPU cores, 48 GB RAM.
- macOS 27.0.1, build 26A434, arm64.
- Rake 0.1.0, RAKE-012 commit `7dfbab4414c3eba2982b45cb9121c0581c194421`.
- Pake CLI 3.17.8, release build for `aarch64-apple-darwin`.
- Site: `http://127.0.0.1:18763`. The fixture in [benchmark.py](benchmark.py)
  serves one heading, one paragraph and a page-load callback. No remote assets.

The measured Rake runtime predates RAKE-009, RAKE-010 and RAKE-011.
Those add injection, navigation and devtools support. Their expected size impact
is small, but these numbers do not measure the combined runtime.
The separately observed devtools feature adds about 1.4 KB to the executable.
Rerun this benchmark after integration before claiming final release numbers.

## Results

Each timing and memory result uses five fresh processes. Ranges are inclusive.
KiB means 1024 bytes. Bundle size is allocated disk usage from `du -sk`.

| App | Bundle | Main executable | Distribution archive | Page-load median (range) | Idle RSS median (range) |
|---|---:|---:|---:|---:|---:|
| Rake | 820 KiB | 821216 bytes | 395300 bytes, template tar.gz | 401.818 ms (373.012 to 550.150 ms) | 183712 KiB (181744 to 196576 KiB) |
| Pake | 11444 KiB | 11313200 bytes | 4222870 bytes, app tar.gz | 517.061 ms (447.848 to 1168.074 ms) | 214576 KiB (213776 to 219600 KiB) |

Raw observations, in execution order:

```text
run  Rake load ms  Pake load ms  Rake RSS KiB  Pake RSS KiB
1    401.818       678.393       181744        219600
2    388.697       517.061       187712        213776
3    373.012       474.921       183712        214576
4    412.079       1168.074      196576        215008
5    550.150       447.848       183360        213792
```

## Method and limits

The script alternates Rake and Pake, with Rake first in each pair. It starts a
monotonic timer immediately before `open -n <App>.app`. Timing stops when the
server receives `/loaded`, fetched by the page's `load` event handler.
This includes Launch Services, process startup, WebKit startup and local page
loading. It does not measure first visible paint or window appearance directly.
The endpoint measures page readiness, not a remote website's latency.

Cold means a fresh process. The OS file cache is warm. No `sudo purge` was used.
Browser storage is not reset between runs. One Rake pilot was excluded before
the five measured pairs. Other desktop apps and development jobs were running.
This is a small local sample, not a controlled laboratory result.

After the load callback, the script waits 30 seconds and runs:

```sh
ps -ww -axo pid=,ppid=,rss=,command=
```

RSS is the sum of the app and its WebContent, Networking and GPU services.
Every measured run had exactly those three services. WebKit XPC services have
`launchd` as parent. The script uses macOS's
`responsibility_get_pid_responsible_for_pid`, through Python `ctypes`, to match
their responsible PID to the app PID. It excludes all baseline processes.
This private macOS API may need replacement on another OS version.

RSS is easy to reproduce with `ps` and treats both apps identically. It includes
shared resident pages more than once. It is not unique physical memory, peak
memory or Activity Monitor's memory-pressure figure. The script records each
component's RSS and PID in JSON. It terminates only the app and its attributed
WebKit services, then stops the HTTP server.

Pake's default build produced a signed `.app`, but its DMG packaging failed:

```text
execution error: Finder got an error: AppleEvent timed out. (-1712)
Failed running AppleScript
```

Retrying with `--targets app` succeeded. All runtime options stayed at defaults.
No usable DMG size is reported. Pake's archive above was made with `tar -czf`.
Rake's archive is the original template distribution, before benchmark config
injection. These are different payloads, not equivalent installer formats.
Neither app was notarized. Rake's template has no production icon. Pake includes
its default resources and more features, so this is not a feature-parity test.

## Reproduce

Requires the existing Rust toolchain, Pake CLI 3.17.8 and Python 3 on macOS arm64.
Run from the repository root. Port 18763 must be free. Close existing instances
of the benchmark apps first. The script launches real desktop windows.

```sh
BENCH=$(mktemp -d "${TMPDIR%/}/rake-bench-013.XXXXXX")
git worktree add --detach "$BENCH/source" 7dfbab4414c3eba2982b45cb9121c0581c194421
(cd "$BENCH/source" && cargo xtask template)
tar -xzf "$BENCH/source/target/template/rake-template-0.1.0-macos-arm64.tar.gz" -C "$BENCH"
printf '%s' '{"url":"http://127.0.0.1:18763"}' > "$BENCH/Rake.app/Contents/Resources/config.json"
pake --version
# The default command failed only at DMG packaging on this machine.
pake http://127.0.0.1:18763 --name ExampleBench --targets app
mv ExampleBench.app "$BENCH/ExampleBench.app"
python3 docs/benchmark.py \
  "$BENCH/Rake.app/Contents/MacOS/rake-runtime" \
  "$BENCH/ExampleBench.app/Contents/MacOS/pake-examplebench" \
  > "$BENCH/results.jsonl"
du -sk "$BENCH/Rake.app" "$BENCH/ExampleBench.app"
stat -f %z "$BENCH/Rake.app/Contents/MacOS/rake-runtime"
stat -f %z "$BENCH/ExampleBench.app/Contents/MacOS/pake-examplebench"
stat -f %z "$BENCH/source/target/template/rake-template-0.1.0-macos-arm64.tar.gz"
tar -czf "$BENCH/ExampleBench.tar.gz" -C "$BENCH" ExampleBench.app
stat -f %z "$BENCH/ExampleBench.tar.gz"
git worktree remove "$BENCH/source"
```

Archive bytes can vary with timestamps, signatures and tar metadata.
The original measurement extracted Rake inside the temporary worktree's
`target/template` directory. That path difference does not change the payload.
The reproduction leaves results and both apps in `$BENCH` for inspection.

To calculate medians and ranges from the recorded JSON:

```sh
python3 - "$BENCH/results.jsonl" <<'PY'
import json, statistics, sys
rows = [json.loads(line) for line in open(sys.argv[1])]
for app in dict.fromkeys(row["app"] for row in rows):
    for key in ("load_ms", "rss_kib"):
        values = [row[key] for row in rows if row["app"] == app]
        print(app, key, statistics.median(values), min(values), max(values))
PY
```

## Conclusion

Rake's bundle is 10624 KiB smaller. No bundle-size fix is needed for RAKE-013.
Rake also has lower median startup time and idle RSS in this sample.
Both use the system WebKit, so a much smaller executable does not imply the same
reduction in runtime memory. Repeat with the integrated runtime and real sites
before generalizing these results.

# Rake and Pake benchmark

Measured on 2026-10-31. Rake meets the bundle-size target.

## Environment

- MacBook Pro, Apple M5 Pro, 18 CPU cores, 48 GB RAM.
- macOS 27.0.1, build 26A434, arm64.
- Rake 0.1.0, integrated main commit `5a18a157af682232614896ae033e645eb6208984`.
- Pake CLI 3.17.8, release build for `aarch64-apple-darwin`.
- Site: `http://127.0.0.1:18763`. The fixture in [benchmark.py](benchmark.py)
  serves one heading, one paragraph and a page-load callback. No remote assets.

## Results

Each timing and memory result uses five fresh processes. Ranges are inclusive.
KiB means 1024 bytes. Bundle size is allocated disk usage from `du -sk`.

| App | Bundle | Main executable | Distribution archive | Page-load median (range) | Idle RSS median (range) |
|---|---:|---:|---:|---:|---:|
| Rake | 856 KiB | 856192 bytes | 419182 bytes, template tar.gz | 366.681 ms (352.227 to 794.956 ms) | 193280 KiB (192416 to 193952 KiB) |
| Pake | 11444 KiB | 11313200 bytes | 4222870 bytes, app tar.gz | 397.305 ms (371.334 to 418.572 ms) | 217872 KiB (213472 to 218432 KiB) |

Raw observations, in execution order:

```text
run  Rake load ms  Pake load ms  Rake RSS KiB  Pake RSS KiB
1    794.956       418.572       193280        213472
2    352.227       405.546       193600        218432
3    381.892       371.334       192416        218112
4    366.681       397.305       192480        217744
5    356.346       395.768       193952        217872
```

## Method and limits

The script alternates Rake and Pake, with Rake first in each pair. It starts a
monotonic timer immediately before `open -n <App>.app`. Timing stops when the
server receives `/loaded`, fetched by the page's `load` event handler.
This includes Launch Services, process startup, WebKit startup and local page
loading. It does not measure first visible paint or window appearance directly.
The endpoint measures page readiness, not a remote website's latency.

Cold means a fresh process. The OS file cache is warm. No `sudo purge` was used.
Browser storage is not reset between runs. No pilot runs were excluded from
this measurement. Other desktop apps and development jobs were running.
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
git worktree add --detach "$BENCH/source" 5a18a157af682232614896ae033e645eb6208984
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
This measurement reused the existing Pake app and rebuilt Rake in the benchmark
branch after rebasing onto the main commit above. Only these two docs files
differed from that commit. Both apps were outside the build worktree.
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

Rake's bundle is 10588 KiB smaller. No bundle-size fix is needed for RAKE-013.
Rake also has lower median startup time and idle RSS in this sample.
Both use the system WebKit, so a much smaller executable does not imply the same
reduction in runtime memory. Repeat with real sites before generalizing these
results.

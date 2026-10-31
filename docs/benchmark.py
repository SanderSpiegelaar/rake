#!/usr/bin/env python3
"""Measure fresh-process page load and 30-second RSS on macOS."""

import argparse
import ctypes
import json
import os
from pathlib import Path
import signal
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer


def processes():
    text = subprocess.check_output(
        ["ps", "-ww", "-axo", "pid=,ppid=,rss=,command="], text=True
    )
    return {
        int(p): (int(parent), int(rss), cmd)
        for p, parent, rss, cmd in (line.split(None, 3) for line in text.splitlines())
    }


loaded = threading.Event()
loaded_at = 0
responsible_pid = ctypes.CDLL(None).responsibility_get_pid_responsible_for_pid
responsible_pid.argtypes = [ctypes.c_int]
responsible_pid.restype = ctypes.c_int


class Site(BaseHTTPRequestHandler):
    def do_GET(self):
        global loaded_at
        if self.path == "/loaded":
            loaded_at = time.perf_counter()
            loaded.set()
            body = b"ok"
        else:
            body = (
                b"<!doctype html><title>ExampleBench</title>"
                b"<h1>ExampleBench</h1><p>Local benchmark page.</p>"
                b'<script>addEventListener("load",()=>fetch("/loaded"))</script>'
            )
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


def measure(executable, run):
    before = processes()
    loaded.clear()
    started = time.perf_counter()
    subprocess.run(["open", "-n", str(Path(executable).parents[2])], check=True)
    pid = None
    children = set()
    try:
        if not loaded.wait(30):
            raise RuntimeError(f"Page did not load: {executable}")
        latency = (loaded_at - started) * 1000
        pid = next(
            p
            for p, (_, _, cmd) in processes().items()
            if p not in before and cmd == executable
        )
        time.sleep(30)
        current = processes()
        children = {
            p
            for p, (_, _, cmd) in current.items()
            if p not in before
            and "com.apple.WebKit." in cmd
            and responsible_pid(p) == pid
        }
        selected = {pid} | children
        if len(children) < 3:
            raise RuntimeError(
                "Expected attributable GPU, WebContent and Networking services"
            )
        rows = [
            {
                "pid": p,
                "ppid": current[p][0],
                "rss_kib": current[p][1],
                "command": current[p][2],
            }
            for p in sorted(selected)
        ]
        print(
            json.dumps(
                {
                    "app": executable,
                    "run": run,
                    "load_ms": round(latency, 3),
                    "rss_kib": sum(row["rss_kib"] for row in rows),
                    "processes": rows,
                }
            ),
            flush=True,
        )
    finally:
        # XPC services have launchd as parent. Use macOS responsibility instead.
        children |= {
            p
            for p, (_, _, cmd) in processes().items()
            if p not in before
            and "com.apple.WebKit." in cmd
            and pid is not None
            and responsible_pid(p) == pid
        }
        if pid is None:
            pid = next(
                (
                    p
                    for p, (_, _, cmd) in processes().items()
                    if p not in before and cmd == executable
                ),
                None,
            )
        for pid in children | ({pid} if pid else set()):
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executables", nargs="+")
    parser.add_argument("--runs", type=int, default=5)
    args = parser.parse_args()
    HTTPServer.allow_reuse_address = True
    server = HTTPServer(("127.0.0.1", 18763), Site)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        for run in range(1, args.runs + 1):
            for executable in args.executables:
                measure(executable, run)
    finally:
        server.shutdown()
        server.server_close()

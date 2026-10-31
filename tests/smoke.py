#!/usr/bin/env python3
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]


def launch(bundle):
    subprocess.run(
        ["codesign", "--verify", "--deep", "--strict", str(bundle)], check=True
    )
    executable = subprocess.check_output(
        [
            "/usr/libexec/PlistBuddy",
            "-c",
            "Print :CFBundleExecutable",
            str(bundle / "Contents/Info.plist"),
        ],
        text=True,
    ).strip()
    environment = os.environ.copy()
    environment.pop("RAKE_CONFIG", None)
    process = subprocess.Popen(
        [str(bundle / "Contents/MacOS" / executable)], env=environment
    )
    try:
        time.sleep(5)
        if process.poll() is not None:
            raise RuntimeError(
                f"App exited within 5 seconds with status {process.returncode}"
            )
        print(f"App stayed alive for 5 seconds (PID {process.pid})", flush=True)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
                raise RuntimeError("App did not quit within 5 seconds")


def main():
    if sys.platform != "darwin":
        raise RuntimeError("Smoke test requires macOS")
    if len(sys.argv) == 2:
        launch(Path(sys.argv[1]).resolve())
        return
    if len(sys.argv) != 1:
        raise RuntimeError("Usage: python3 tests/smoke.py [APP_BUNDLE]")

    subprocess.run(["cargo", "xtask", "template"], cwd=ROOT, check=True)
    subprocess.run(
        ["cargo", "build", "--locked", "--package", "rake-cli"], cwd=ROOT, check=True
    )
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT
        )
    )
    version = next(
        p["version"] for p in metadata["packages"] if p["name"] == "rake-cli"
    )
    template = ROOT / "target/template/Rake.app"
    with tempfile.TemporaryDirectory(prefix="rake-smoke-") as directory:
        directory = Path(directory)
        home = directory / "home"
        cache = (
            home / "Library/Caches/rake/templates" / version / "macos-arm64/Rake.app"
        )
        shutil.copytree(template, cache)
        environment = os.environ.copy()
        environment["HOME"] = str(home)
        subprocess.run(
            [
                str(Path(metadata["target_directory"]) / "debug/rake"),
                "--config",
                str(ROOT / "tests/fixtures/basic.json"),
                "--icon",
                str(template / "Contents/Resources/icon.icns"),
            ],
            cwd=directory,
            env=environment,
            check=True,
        )
        launch(directory / "Rake Smoke Test.app")


if __name__ == "__main__":
    main()

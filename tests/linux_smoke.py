#!/usr/bin/env python3
import functools
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import threading
import time


ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def wait_for(predicate, process, description):
    for _ in range(300):
        if process.poll() is not None:
            raise RuntimeError(
                f"{description}: process exited with {process.returncode}"
            )
        if predicate():
            return
        time.sleep(0.1)
    raise RuntimeError(f"Timed out waiting for {description}")


def main():
    if platform.system() != "Linux":
        raise RuntimeError(
            "Run this test on Linux with WebKitGTK, Xvfb, and Weston installed"
        )
    os.umask(0o077)
    artifacts = Path(
        os.environ.get("RAKE_SMOKE_ARTIFACTS", ROOT / "target/linux-smoke")
    )
    artifacts.mkdir(parents=True, exist_ok=True)
    run("cargo", "xtask", "template", cwd=ROOT)
    run("cargo", "build", "--locked", "--package", "rake-cli", cwd=ROOT)
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT
        )
    )
    version = next(
        p["version"] for p in metadata["packages"] if p["name"] == "rake-cli"
    )
    cli = Path(metadata["target_directory"]) / "debug/rake"
    arch = "arm64" if platform.machine() == "aarch64" else "x86_64"
    with tempfile.TemporaryDirectory(prefix="rake-linux-smoke-") as temporary:
        directory = Path(temporary)
        rendered = threading.Event()

        class Handler(SimpleHTTPRequestHandler):
            def do_GET(self):
                if self.path == "/rendered":
                    rendered.set()
                    self.send_response(204)
                    self.end_headers()
                else:
                    super().do_GET()

        (directory / "index.html").write_text(
            "<!doctype html><title>Rake Linux proof</title>"
            "<style>body{background:#103b35;color:white;font:48px sans-serif;padding:60px}"
            "strong{color:#b4f17e}</style><h1>Rake on Linux</h1>"
            '<p>WebKitGTK rendered this page.</p><strong id="proof">Waiting</strong>'
            '<script>document.getElementById("proof").textContent="JavaScript works";'
            'requestAnimationFrame(()=>requestAnimationFrame(()=>fetch("/rendered")))</script>'
        )
        server = ThreadingHTTPServer(
            ("127.0.0.1", 0), functools.partial(Handler, directory=str(directory))
        )
        threading.Thread(target=server.serve_forever, daemon=True).start()
        environment = os.environ.copy()
        environment["LIBGL_ALWAYS_SOFTWARE"] = "1"
        environment["XDG_CACHE_HOME"] = str(directory / "cache")
        cache = directory / "cache/rake/templates" / version / f"linux-{arch}/Rake"
        shutil.copytree(ROOT / "target/template/Rake", cache)
        output = directory / "output"
        output.mkdir()
        args = [
            str(cli),
            f"http://127.0.0.1:{server.server_port}",
            "--name",
            "Rake Linux Proof",
            "--identifier",
            "org.rake.linux-proof",
            "--icon",
            str(ROOT / "assets/icon.svg"),
            "--json",
        ]
        for target in ["tarball", "deb"]:
            result = subprocess.check_output(
                args + ["--target", target, "--force"], cwd=output, env=environment
            )
            result = json.loads(result)
            assert result["status"] == "success"
            assert result["warnings"] == [], result["warnings"]
        tarball = output / "Rake Linux Proof.tar.gz"
        extracted = directory / "extracted"
        extracted.mkdir()
        run("tar", "-xzf", str(tarball), "-C", str(extracted))
        app = extracted / "Rake Linux Proof"
        for name in [
            "config.json",
            "icon.png",
            "rake-runtime",
            "org.rake.linux-proof.desktop",
        ]:
            assert (app / name).is_file(), name
        deb = output / "Rake Linux Proof.deb"
        run(*([] if os.getuid() == 0 else ["sudo"]), "dpkg", "-i", str(deb))
        assert Path("/opt/org.rake.linux-proof").stat().st_mode & 0o777 == 0o755
        assert (
            Path("/opt/org.rake.linux-proof/rake-runtime").stat().st_mode & 0o777
            == 0o755
        )
        for artifact in [tarball, deb]:
            shutil.copy2(artifact, artifacts)
        processes = []
        try:
            xlog = (artifacts / "xvfb.log").open("w")
            xvfb = subprocess.Popen(
                ["Xvfb", ":93", "-screen", "0", "1280x900x24", "-nolisten", "tcp"],
                stdout=xlog,
                stderr=xlog,
            )
            processes.append(xvfb)
            wait_for(lambda: Path("/tmp/.X11-unix/X93").exists(), xvfb, "Xvfb socket")
            environment["DISPLAY"] = ":93"
            environment["GDK_BACKEND"] = "x11"
            # Container kernels may forbid WebKit's nested user namespace sandbox.
            if os.getuid() == 0:
                environment["WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS"] = "1"
            log = (artifacts / "x11.log").open("w")
            installed = "/opt/org.rake.linux-proof/rake-runtime"
            process = subprocess.Popen(
                [installed], env=environment, stdout=log, stderr=log
            )
            processes.append(process)
            wait_for(rendered.is_set, process, "X11 page JavaScript")
            time.sleep(2)
            run(
                "import",
                "-display",
                ":93",
                "-window",
                "root",
                str(artifacts / "x11.png"),
            )
            process.terminate()
            process.wait(timeout=10)
            rendered.clear()
            runtime = directory / "wayland"
            runtime.mkdir(mode=0o700)
            environment["XDG_RUNTIME_DIR"] = str(runtime)
            environment["WAYLAND_DISPLAY"] = "rake-wayland"
            environment["GDK_BACKEND"] = "wayland"
            environment.pop("DISPLAY", None)
            weston_log = (artifacts / "weston.log").open("w")
            weston = subprocess.Popen(
                [
                    "weston",
                    "--backend=headless",
                    "--socket=rake-wayland",
                    "--idle-time=0",
                    "--width=1280",
                    "--height=900",
                ],
                env=environment,
                stdout=weston_log,
                stderr=weston_log,
            )
            processes.append(weston)
            wait_for(
                lambda: (runtime / "rake-wayland").exists(), weston, "Weston socket"
            )
            wayland_log = (artifacts / "wayland.log").open("w")
            environment["WAYLAND_DEBUG"] = "1"
            process = subprocess.Popen(
                [str(app / "rake-runtime")],
                env=environment,
                stdout=wayland_log,
                stderr=wayland_log,
            )
            processes.append(process)
            try:
                wait_for(rendered.is_set, process, "native Wayland page JavaScript")
                time.sleep(2)
                assert process.poll() is None
                result = "Native Wayland loaded the page and ran JavaScript with DISPLAY unset."
            except (RuntimeError, AssertionError) as error:
                result = f"Native Wayland did not pass: {error}"
            (artifacts / "wayland-result.txt").write_text(result + "\n")
            print(result, flush=True)
            print(f"X11 screenshot: {artifacts / 'x11.png'}", flush=True)
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=10)
            server.shutdown()


if __name__ == "__main__":
    main()

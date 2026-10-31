#!/usr/bin/env python3
import ctypes
from ctypes import wintypes
import functools
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import tempfile
import threading
import time
import zipfile
import zlib


ROOT = Path(__file__).resolve().parents[1]


def resource(executable, kind, identifier):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
    kernel.LoadLibraryExW.restype = wintypes.HMODULE
    kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
    kernel.FindResourceW.restype = wintypes.HRSRC
    kernel.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HRSRC]
    kernel.SizeofResource.restype = wintypes.DWORD
    kernel.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HRSRC]
    kernel.LoadResource.restype = wintypes.HGLOBAL
    kernel.LockResource.argtypes = [wintypes.HGLOBAL]
    kernel.LockResource.restype = ctypes.c_void_p
    kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
    module = kernel.LoadLibraryExW(str(executable), None, 2)
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        entry = kernel.FindResourceW(module, identifier, kind)
        if not entry:
            return None
        size = kernel.SizeofResource(module, entry)
        data = kernel.LockResource(kernel.LoadResource(module, entry))
        assert size and data
        return ctypes.string_at(data, size)
    finally:
        kernel.FreeLibrary(module)


def png():
    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data))
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 64, 64, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress((b"\0" + b"\xeb\x35\x67\xff" * 64) * 64))
        + chunk(b"IEND", b"")
    )


def main():
    if platform.system() != "Windows":
        raise RuntimeError("Run this test on Windows with WebView2 installed")
    artifacts = Path(
        os.environ.get("RAKE_SMOKE_ARTIFACTS", ROOT / "target/windows-smoke")
    )
    artifacts.mkdir(parents=True, exist_ok=True)
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
    cli = Path(metadata["target_directory"]) / "debug/rake.exe"
    with tempfile.TemporaryDirectory(prefix="rake-windows-smoke-") as temporary:
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
            "<!doctype html><title>Rake Windows proof</title><h1>Rake on Windows</h1>"
            "<script>requestAnimationFrame(()=>requestAnimationFrame(()=>{"
            'if(window.rakeInjected)fetch("/rendered")}))</script>'
        )
        server = ThreadingHTTPServer(
            ("127.0.0.1", 0), functools.partial(Handler, directory=str(directory))
        )
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            environment = os.environ.copy()
            environment["LOCALAPPDATA"] = str(directory / "cache")
            template = (
                directory / "cache/rake/templates" / version / "windows-x86_64/Rake"
            )
            shutil.copytree(ROOT / "target/template/Rake", template)
            original = (template / "rake-runtime.exe").read_bytes()
            custom_icon = png()
            icon = directory / "custom.png"
            icon.write_bytes(custom_icon)
            inject = directory / "proof.js"
            inject.write_text("window.rakeInjected = true;")
            output = directory / "output"
            output.mkdir()
            args = [
                str(cli),
                f"http://127.0.0.1:{server.server_port}",
                "--name",
                "Rake Windows Proof",
                "--icon",
                str(icon),
                "--inject",
                str(inject),
                "--target",
                "zip",
                "--json",
            ]
            result = json.loads(
                subprocess.check_output(args, cwd=output, env=environment)
            )
            assert result["status"] == "success" and result["warnings"] == [], result
            assert (template / "rake-runtime.exe").read_bytes() == original
            assert not (template / "config.json").exists()
            refused = subprocess.run(
                args, cwd=output, env=environment, capture_output=True
            )
            assert refused.returncode == 2
            subprocess.run(args + ["--force"], cwd=output, env=environment, check=True)
            archive = output / "Rake Windows Proof.zip"
            extracted = directory / "extracted"
            with zipfile.ZipFile(archive) as zipped:
                zipped.extractall(extracted)
            app = extracted / "Rake Windows Proof"
            executable = app / "Rake Windows Proof.exe"
            assert executable.is_file() and (app / "config.json").is_file()
            assert (app / "inject/proof.js").read_text() == inject.read_text()
            assert resource(executable, 3, 1) == custom_icon
            assert resource(executable, 14, 1)[:6] == b"\0\0\x01\0\x01\0"
            assert resource(template / "rake-runtime.exe", 3, 1) != custom_icon
            pe = executable.read_bytes()
            offset = struct.unpack_from("<I", pe, 0x3C)[0]
            assert struct.unpack_from("<H", pe, offset + 24 + 68)[0] == 2, (
                "release exe must use GUI subsystem"
            )
            shutil.copy2(archive, artifacts)
            with (artifacts / "runtime.log").open("w") as log:
                process = subprocess.Popen(
                    [str(executable)],
                    cwd=directory,
                    env=environment,
                    stdout=log,
                    stderr=log,
                )
                try:
                    for _ in range(600):
                        if process.poll() is not None:
                            raise RuntimeError(
                                f"Runtime exited with {process.returncode}"
                            )
                        if rendered.wait(0.1):
                            break
                    else:
                        raise RuntimeError(
                            "WebView2 did not execute page JavaScript within 60 seconds"
                        )
                    assert process.poll() is None
                    result = f"{platform.platform()}: ZIP app loaded config next to exe, custom icon resources match, WebView2 executed JavaScript."
                    (artifacts / "result.txt").write_text(result + "\n")
                    print(result, flush=True)
                finally:
                    process.terminate()
                    process.wait(timeout=15)
        finally:
            server.shutdown()


if __name__ == "__main__":
    main()

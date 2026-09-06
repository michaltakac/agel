#!/usr/bin/env python3
"""Local, layout-aware text console beside the real QEMU framebuffer.

The browser supplies composed UTF-8; QEMU and Agel still execute every form.
No Python packages are required. Only a loopback HTTP endpoint is exposed.
"""
import argparse
import json
import secrets
import socket
import subprocess
import tempfile
import threading
import time
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def connect(path):
    deadline = time.monotonic() + 15
    while True:
        stream = socket.socket(socket.AF_UNIX)
        try:
            stream.connect(str(path))
            stream.settimeout(15)
            return stream
        except (FileNotFoundError, ConnectionRefusedError):
            stream.close()
            if time.monotonic() >= deadline:
                raise TimeoutError(f"QEMU did not open {path}")
            time.sleep(0.05)


class Machine:
    def __init__(self, image, directory, snapshot=False):
        self.directory = Path(directory)
        self.serial_lock = threading.Lock()
        self.qmp_lock = threading.Lock()
        self.process = subprocess.Popen([
            "qemu-system-x86_64", "-machine", "pc,accel=tcg", "-m", "64M",
            "-display", "none", "-no-reboot", "-vga", "std",
            "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
            "-qmp", f"unix:{directory}/qmp,server=on,wait=off",
            "-chardev", f"socket,id=serial0,path={directory}/serial,server=on,wait=on",
            "-serial", "chardev:serial0", "-boot", "order=c,strict=on",
            "-drive", f"format=raw,file={image},if=ide,index=0,media=disk" +
            (",snapshot=on" if snapshot else ""),
        ], stdin=subprocess.DEVNULL)
        try:
            self.serial = connect(self.directory / "serial")
            self.qmp = connect(self.directory / "qmp")
            self.qmp_file = self.qmp.makefile("rb")
            json.loads(self.qmp_file.readline())
            self.command("qmp_capabilities")
            self.boot = self.until_prompt().decode("utf-8", errors="replace")
            self.ready = True
        except BaseException:
            self.close()
            raise

    def command(self, execute, arguments=None):
        with self.qmp_lock:
            self.qmp.sendall(json.dumps({"execute": execute, "arguments": arguments or {}}).encode() + b"\n")
            while True:
                result = json.loads(self.qmp_file.readline())
                if "error" in result:
                    raise RuntimeError(result["error"]["desc"])
                if "return" in result:
                    return result["return"]

    def until_prompt(self):
        result = bytearray()
        deadline = time.monotonic() + 30
        while not result.endswith(b"live-desktop> "):
            if time.monotonic() > deadline or len(result) > 65536:
                raise TimeoutError("Agel did not return its prompt")
            byte = self.serial.recv(1)
            if not byte:
                raise RuntimeError("Agel stopped")
            result.extend(byte)
        return bytes(result)

    def submit(self, source):
        encoded = source.encode("utf-8")
        if not encoded or len(encoded) > 256:
            raise ValueError("Enter one form, at most 256 UTF-8 bytes")
        if any(ord(char) < 32 or ord(char) == 127 for char in source):
            raise ValueError("Enter one line without control characters")
        if source.strip() == ":shutdown":
            raise ValueError("Stop this viewer with Ctrl-C in its launching terminal")
        with self.serial_lock:
            if not self.ready:
                raise RuntimeError("Input connection lost synchronization; restart the viewer")
            self.ready = False
            # Acknowledge each byte: a burst can overrun the emulated UART
            # while the guest redraws the command bar between characters.
            for byte in encoded:
                self.serial.sendall(bytes([byte]))
                if self.serial.recv(1) != bytes([byte]):
                    raise RuntimeError("Agel input echo lost synchronization; restart the viewer")
            self.serial.sendall(b"\n")
            result = self.until_prompt().decode("utf-8", errors="replace")
            self.ready = True
            return result

    def frame(self):
        # HTTPServer serializes frame requests; QMP serializes commands.
        frame = self.directory / "frame.png"
        self.command("screendump", {"filename": str(frame), "format": "png"})
        return frame.read_bytes()

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        for name in ("qmp_file", "qmp", "serial"):
            stream = getattr(self, name, None)
            if stream is not None:
                stream.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image")
    parser.add_argument("--no-open", action="store_true")
    parser.add_argument("--snapshot", action="store_true")
    args = parser.parse_args()
    token = secrets.token_urlsafe(24)
    with tempfile.TemporaryDirectory(prefix="agel-view-", dir="/tmp") as directory:
        machine = Machine(str(Path(args.image).resolve()), directory, args.snapshot)
        frame_lock = threading.Lock()

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def respond(self, body, content_type, status=200):
                self.send_response(status)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(body)))
                self.send_header("Cache-Control", "no-store")
                self.send_header("X-Content-Type-Options", "nosniff")
                self.send_header("Content-Security-Policy", "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'")
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                if self.path == "/favicon.ico":
                    self.respond(b"", "image/x-icon", 204)
                elif self.path == f"/{token}/":
                    self.respond(Path(__file__).with_name("graphical-console.html").read_bytes(), "text/html; charset=utf-8")
                elif self.path.split("?")[0] == f"/{token}/frame.png":
                    try:
                        with frame_lock:
                            frame = machine.frame()
                        self.respond(frame, "image/png")
                    except Exception as error:
                        self.respond(str(error).encode(), "text/plain", 503)
                else:
                    self.respond(b"Not found", "text/plain", 404)

            def do_POST(self):
                origin = f"http://127.0.0.1:{self.server.server_port}"
                if self.path != f"/{token}/input" or self.headers.get("Origin") != origin:
                    self.respond(b"Forbidden", "text/plain", 403)
                    return
                try:
                    length = int(self.headers.get("Content-Length", "0"))
                    if not 0 < length <= 4096:
                        raise ValueError("Invalid request size")
                    source = json.loads(self.rfile.read(length))["source"]
                    if not isinstance(source, str):
                        raise ValueError("Source must be text")
                    result = machine.submit(source)
                    self.respond(json.dumps({"result": result}).encode(), "application/json")
                except Exception as error:
                    self.respond(json.dumps({"error": str(error)}).encode(), "application/json", 400)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        url = f"http://127.0.0.1:{server.server_port}/{token}/"
        print(f"Agel graphical console: {url}", flush=True)
        print("Host keyboard layout and paste enabled. No mouse capture. Ctrl-C here stops QEMU.", flush=True)
        if not args.no_open:
            webbrowser.open(url)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
        finally:
            server.server_close()
            machine.close()


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Prompt-synchronized QEMU test for the real native serial REPL."""

from __future__ import annotations

import queue
import subprocess
import sys
import threading
import time


class Harness:
    def __init__(self, image: str) -> None:
        self.output: queue.Queue[bytes | None] = queue.Queue()
        self.transcript = bytearray()
        self.deadline = time.monotonic() + 45.0
        self.process = subprocess.Popen(
            [
                "qemu-system-x86_64",
                "-machine",
                "pc,accel=tcg",
                "-m",
                "64M",
                "-display",
                "none",
                "-monitor",
                "none",
                "-chardev",
                "stdio,id=serial0,signal=off,mux=off",
                "-serial",
                "chardev:serial0",
                "-no-reboot",
                "-device",
                "isa-debug-exit,iobase=0xf4,iosize=0x04",
                "-boot",
                "order=c,strict=on",
                "-drive",
                f"format=raw,file={image},snapshot=on",
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self) -> None:
        assert self.process.stdout is not None
        while byte := self.process.stdout.read(1):
            self.transcript.extend(byte)
            self.output.put(byte)
        self.output.put(None)

    def remaining(self, maximum: float) -> float:
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("global native REPL deadline exceeded")
        return min(maximum, remaining)

    def expect_until(self, expected: bytes, timeout: float = 8.0) -> None:
        deadline = time.monotonic() + self.remaining(timeout)
        matched = 0
        while matched != len(expected):
            remaining = min(deadline, self.deadline) - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"waiting for {expected!r}")
            try:
                byte = self.output.get(timeout=remaining)
            except queue.Empty as error:
                raise TimeoutError(f"waiting for {expected!r}") from error
            if byte is None:
                raise RuntimeError(f"QEMU exited while waiting for {expected!r}")
            matched = matched + 1 if byte[0] == expected[matched] else int(byte == expected[:1])

    def expect_exact(self, expected: bytes, timeout: float = 8.0) -> None:
        for wanted in expected:
            try:
                byte = self.output.get(timeout=self.remaining(timeout))
            except queue.Empty as error:
                raise TimeoutError(f"waiting for exact frame {expected!r}") from error
            if byte is None:
                raise RuntimeError(f"QEMU exited during exact frame {expected!r}")
            if byte[0] != wanted:
                raise RuntimeError(
                    f"protocol mismatch: expected byte {wanted:#x}, received {byte[0]:#x}"
                )

    def send(self, line: str, expected: str, revision: int) -> None:
        self.send_bytes(line)
        assert self.process.stdin is not None
        self.process.stdin.write(b"\n")
        self.process.stdin.flush()
        frame = f"\r\n{expected}\r\nagel-native[{revision}]> ".encode("ascii")
        self.expect_exact(frame)

    def send_bytes(self, text: str) -> None:
        assert self.process.stdin is not None
        for byte in text.encode("ascii"):
            self.process.stdin.write(bytes([byte]))
            self.process.stdin.flush()
            self.expect_exact(bytes([byte]), timeout=2.0)

    def continue_form(self, line: str) -> None:
        self.send_bytes(line)
        self.process.stdin.write(b"\n")
        self.process.stdin.flush()
        self.expect_exact(b"\r\n             ... ")

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=2)
        self.reader.join(timeout=2)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: test-native-repl.py IMAGE", file=sys.stderr)
        return 2
    harness = Harness(sys.argv[1])
    failure: Exception | None = None
    try:
        harness.expect_until(b"AGEL_NATIVE_READY")
        harness.expect_until(b"agel-native[0]> ")
        harness.send("(+ 20 22)", "42", 1)
        harness.send("(def native-answer 40)", "40", 2)
        harness.send("(+ native-answer 2)", "42", 3)
        harness.send("(eval '(+ 19 23))", "42", 4)
        harness.send("(def x 1)", "1", 5)
        harness.send("(def x 2)", "2", 6)
        harness.send(
            "(begin (def x 3) (/ 1 0))",
            "error: division by zero (transaction rolled back)",
            6,
        )
        harness.send(":rollback", "rolled back one committed native world", 7)
        harness.send("x", "1", 8)
        harness.continue_form("(def fact (fn (n)")
        harness.send("  (if (= n 0) 1 (* n (fact (- n 1))))))", "#<native-function>", 9)
        harness.send("(fact 6)", "720", 10)
        harness.send(":defs", "definitions (3): native-answer x fact", 10)
        harness.send(
            ":limits",
            "source=256 nodes=128 globals=24 name=24 params=4 locals=8 "
            "args=8 body=192 depth=24 fuel=2000",
            10,
        )
        harness.send(":verify", "candidate B: isolated health evidence accepted", 10)
        harness.send(":promote", "selected slot B; slot A retained for rollback", 10)
        harness.send(":verify", "candidate B: isolated health evidence accepted", 10)
        harness.send(
            ":promote",
            "denied: candidate B is already active; slot A remains rollback",
            10,
        )
        harness.send(":fault", "watchdog fault: rolled back to slot A", 10)
        harness.send(":recovery-status", "active slot: A (stable)", 10)

        assert harness.process.stdin is not None
        for byte in b":shutdown":
            harness.process.stdin.write(bytes([byte]))
            harness.process.stdin.flush()
            harness.expect_exact(bytes([byte]), timeout=2.0)
        harness.process.stdin.write(b"\n")
        harness.process.stdin.flush()
        harness.expect_exact(b"\r\n")
        exit_code = harness.process.wait(timeout=harness.remaining(8.0))
        if exit_code != 33:
            raise RuntimeError(f"QEMU exit status {exit_code}, expected 33")
    except Exception as error:  # test harness must always show the VM transcript
        failure = error
    finally:
        harness.close()
    if failure is not None:
        print(f"native REPL test failed: {failure}", file=sys.stderr)
        print(harness.transcript.decode("utf-8", errors="replace"), file=sys.stderr)
        return 1
    print("Agel native serial REPL: synchronized end-to-end session [ok]")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

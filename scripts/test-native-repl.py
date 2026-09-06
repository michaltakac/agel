#!/usr/bin/env python3
"""Prompt-synchronized QEMU test for the real native serial REPL."""

from __future__ import annotations

import queue
import subprocess
import sys
import threading
import time
import zlib


class Harness:
    def __init__(self, image: str, *, persistent: bool = False) -> None:
        self.output: queue.Queue[bytes | None] = queue.Queue()
        self.transcript = bytearray()
        self.deadline = time.monotonic() + 60.0
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
                (
                    f"format=raw,file={image}"
                    if persistent
                    else f"format=raw,file={image},snapshot=on"
                ),
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


def persistence_test(image: str) -> None:
    first = Harness(image, persistent=True)
    try:
        first.expect_until(b"AGEL_NATIVE_READY")
        first.expect_until(b"workspace: no persisted image; starting empty")
        first.expect_until(b"agel-native[0]> ")
        first.send_bytes(":edit scratch")
        assert first.process.stdin is not None
        first.process.stdin.write(b"\n")
        first.process.stdin.flush()
        first.expect_exact(b"\r\nedit[scratch]> ")
        first.send("(+ 1 1)", "cell staged; :run NAME to evaluate, :save to persist", 0)
        first.send(":reload", "workspace reload restored empty state", 0)
        first.send(":cells", "cells (0):", 0)
        first.send_bytes(":edit boot")
        assert first.process.stdin is not None
        first.process.stdin.write(b"\n")
        first.process.stdin.flush()
        first.expect_exact(b"\r\nedit[boot]> ")
        first.send(
            "(def persisted-answer 42)",
            "cell staged; :run NAME to evaluate, :save to persist",
            0,
        )
        first.send(":run boot", "42", 1)
        first.send(
            ":save",
            "workspace generation 1 committed: 1 cells; evaluator rebuilt from cells; previous slot retained",
            2,
        )
        first.send_bytes(":edit bad")
        assert first.process.stdin is not None
        first.process.stdin.write(b"\n")
        first.process.stdin.flush()
        first.expect_exact(b"\r\nedit[bad]> ")
        first.send(
            "(def broken (/ 1 0))",
            "cell staged; :run NAME to evaluate, :save to persist",
            2,
        )
        first.send_bytes(":save")
        first.process.stdin.write(b"\n")
        first.process.stdin.flush()
        first.expect_exact(
            b"\r\nworkspace replay rejected at cell bad: error: division by zero\r\n"
            b"workspace not saved; committed evaluator restored\r\n"
            b"agel-native[4]> "
        )
        first.send(
            ":delete bad",
            "cell deleted from staged workspace; :save to commit",
            4,
        )
        shutdown(first)
    finally:
        first.close()

    second = Harness(image, persistent=True)
    try:
        second.expect_until(b"AGEL_NATIVE_READY")
        second.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        second.expect_until(b"agel-native[1]> ")
        second.send("persisted-answer", "42", 2)
        second.send(":show boot", "(def persisted-answer 42)", 2)
        second.send(
            ":workspace", "workspace generation 1, 1 cells, clean", 2
        )
        second.send_bytes(":edit math")
        assert second.process.stdin is not None
        second.process.stdin.write(b"\n")
        second.process.stdin.flush()
        second.expect_exact(b"\r\nedit[math]> ")
        second.send(
            "(def double (fn (x) (+ x x)))",
            "cell staged; :run NAME to evaluate, :save to persist",
            2,
        )
        second.send(":run math", "#<native-function>", 3)
        second.send(
            ":save",
            "workspace generation 2 committed: 2 cells; evaluator rebuilt from cells; previous slot retained",
            5,
        )
        shutdown(second)
    finally:
        second.close()

    # Make generation 2 checksummed and structurally valid but semantically
    # invalid. Boot must reject its replay and try generation 1.
    with open(image, "r+b") as disk:
        disk.seek(256 * 512)
        header = bytearray(disk.read(512))
        length = int.from_bytes(header[24:28], "big")
        disk.seek(257 * 512)
        payload = bytearray(disk.read(15 * 512))
        old = b"(def persisted-answer 42)"
        new = b"(def persisted-answer zz)"
        position = payload[:length].find(old)
        if position < 0 or len(old) != len(new):
            raise RuntimeError("could not synthesize replay-invalid generation")
        payload[position : position + len(old)] = new
        header[28:32] = (
            zlib.crc32(header[:28] + payload[:length]) & 0xFFFFFFFF
        ).to_bytes(4, "big")
        disk.seek(256 * 512)
        disk.write(header)
        disk.seek(257 * 512)
        disk.write(payload)
        disk.flush()

    third = Harness(image, persistent=True)
    try:
        third.expect_until(b"AGEL_NATIVE_READY")
        third.expect_until(
            b"workspace replay rejected at cell boot: error: unbound native symbol"
        )
        third.expect_until(b"trying previous workspace generation")
        third.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        third.expect_until(b"agel-native[1]> ")
        third.send("persisted-answer", "42", 2)
        third.send(":cells", "cells (1): boot", 2)
        shutdown(third)
    finally:
        third.close()

    # Now damage generation 2's payload without updating its checksum. The
    # structural verifier must independently reach the same older generation.
    with open(image, "r+b") as disk:
        disk.seek(257 * 512)
        original = disk.read(1)
        if len(original) != 1:
            raise RuntimeError("test image has no workspace payload sector")
        disk.seek(257 * 512)
        disk.write(bytes([original[0] ^ 0x80]))
        disk.flush()

    fourth = Harness(image, persistent=True)
    try:
        fourth.expect_until(b"AGEL_NATIVE_READY")
        fourth.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        fourth.expect_until(b"agel-native[1]> ")
        fourth.send("persisted-answer", "42", 2)
        shutdown(fourth)
    finally:
        fourth.close()

    # Model a power loss after target-slot invalidation and a partial payload
    # write. With no published header, boot must ignore the torn generation.
    with open(image, "r+b") as disk:
        disk.seek(256 * 512)
        disk.write(bytes(512))
        disk.write(b"partial-uncommitted-workspace")
        disk.flush()

    fifth = Harness(image, persistent=True)
    try:
        fifth.expect_until(b"AGEL_NATIVE_READY")
        fifth.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        fifth.expect_until(b"agel-native[1]> ")
        fifth.send("persisted-answer", "42", 2)
        shutdown(fifth)
    finally:
        fifth.close()


def shutdown(harness: Harness) -> None:
    assert harness.process.stdin is not None
    harness.send_bytes(":shutdown")
    harness.process.stdin.write(b"\n")
    harness.process.stdin.flush()
    harness.expect_exact(b"\r\n")
    exit_code = harness.process.wait(timeout=harness.remaining(8.0))
    if exit_code != 33:
        raise RuntimeError(f"QEMU exit status {exit_code}, expected 33")


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print("usage: test-native-repl.py IMAGE [--persistence]", file=sys.stderr)
        return 2
    if len(sys.argv) == 3:
        if sys.argv[2] != "--persistence":
            print("unknown test mode", file=sys.stderr)
            return 2
        try:
            persistence_test(sys.argv[1])
        except Exception as error:
            print(f"native persistence test failed: {error}", file=sys.stderr)
            return 1
        print(
            "Agel native workspace: edit -> reboot -> semantic, corruption, and torn-write fallback [ok]"
        )
        return 0
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
            "args=8 body=192 depth=24 fuel=2000 agents=8 mailbox=8 run-turns=32",
            10,
        )
        harness.send(
            "(def accumulate (fn (self state message) (+ state message)))",
            "#<native-function>",
            11,
        )
        harness.send("(def counter (spawn accumulate 0))", "#<native-agent:1>", 12)
        harness.send("(send counter 20)", "1", 13)
        harness.send("(send counter 22)", "2", 14)
        harness.send(
            "(begin (run 1) (/ 1 0))",
            "error: division by zero (transaction rolled back)",
            14,
        )
        harness.send("(agent-pending counter)", "2", 15)
        harness.send("(agent-state counter)", "0", 16)
        harness.send("(run 2)", "2", 17)
        harness.send("(agent-state counter)", "42", 18)
        harness.send("(agent-turns counter)", "2", 19)
        harness.send("(agent-count)", "1", 20)
        harness.send(
            "(def fragile (fn (self state message) (/ state message)))",
            "#<native-function>",
            21,
        )
        harness.send("(def broken (spawn fragile 1))", "#<native-agent:2>", 22)
        harness.send("(send broken 0)", "1", 23)
        harness.send("(step)", "#t", 24)
        harness.send("(agent-faulted? broken)", "#t", 25)
        harness.send("(agent-pending broken)", "1", 26)
        harness.send("(drop-message broken)", "0", 27)
        harness.send("(restart-agent broken)", "#<native-agent:2>", 28)
        harness.send("(agent-faulted? broken)", "#f", 29)
        harness.send(":verify", "candidate B: isolated health evidence accepted", 29)
        harness.send(":promote", "selected slot B; slot A retained for rollback", 29)
        harness.send(":verify", "candidate B: isolated health evidence accepted", 29)
        harness.send(
            ":promote",
            "denied: candidate B is already active; slot A remains rollback",
            29,
        )
        harness.send(":fault", "watchdog fault: rolled back to slot A", 29)
        harness.send(":recovery-status", "active slot: A (stable)", 29)

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

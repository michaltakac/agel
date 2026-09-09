#!/usr/bin/env python3
"""Prompt-synchronized QEMU test for the real native serial REPL."""

from __future__ import annotations

import queue
import subprocess
import sys
import tempfile
import threading
import time
import zlib


# The x86-64 debug-exit device maps the guest's clean value 0x10 to host status
# 33; the other two machines leave cleanly with status 0 and the success token.
LAST_HARNESS: "Harness | None" = None
EXPECTED_EXIT = {"x86_64": 33, "aarch64": 0, "riscv64": 0}


def qemu_command(
    architecture: str, image: str, persistent: bool, disk: str | None
) -> list[str]:
    serial = [
        "-display",
        "none",
        "-monitor",
        "none",
        "-chardev",
        "stdio,id=serial0,signal=off,mux=off",
        "-serial",
        "chardev:serial0",
        "-no-reboot",
    ]
    if architecture == "x86_64":
        return [
            "qemu-system-x86_64",
            "-machine",
            "pc,accel=tcg",
            "-m",
            "64M",
            *serial,
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
        ]
    # The diskless machines get a virtio block device behind a modern
    # virtio-mmio transport; QEMU's `virt` exposes legacy transports unless
    # told otherwise, and the driver speaks only the modern layout.
    virtio = (
        [
            "-global",
            "virtio-mmio.force-legacy=false",
            "-drive",
            f"if=none,format=raw,file={disk},id=disk0"
            + ("" if persistent else ",snapshot=on"),
            "-device",
            "virtio-blk-device,drive=disk0",
        ]
        if disk is not None
        else []
    )
    if architecture == "aarch64":
        return [
            "qemu-system-aarch64",
            "-machine",
            "virt",
            "-cpu",
            "cortex-a72",
            "-m",
            "128M",
            *serial,
            *virtio,
            "-kernel",
            image,
        ]
    if architecture == "riscv64":
        return [
            "qemu-system-riscv64",
            "-machine",
            "virt",
            "-m",
            "128M",
            *serial,
            *virtio,
            "-bios",
            "default",
            "-kernel",
            image,
        ]
    raise ValueError(f"unknown architecture {architecture}")


class Harness:
    def __init__(
        self,
        image: str,
        *,
        persistent: bool = False,
        architecture: str = "x86_64",
        disk: str | None = None,
    ) -> None:
        global LAST_HARNESS
        LAST_HARNESS = self
        self.output: queue.Queue[bytes | None] = queue.Queue()
        self.transcript = bytearray()
        self.deadline = time.monotonic() + 90.0
        self.architecture = architecture
        self.process = subprocess.Popen(
            qemu_command(architecture, image, persistent, disk),
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

    def query(self, line: str) -> tuple[str, int]:
        """Send a line and return the reply text and the revision the next
        prompt shows, for answers the test cannot predict exactly."""
        self.send_bytes(line)
        assert self.process.stdin is not None
        self.process.stdin.write(b"\n")
        self.process.stdin.flush()
        self.expect_exact(b"\r\n")
        collected = bytearray()
        marker = b"\r\nagel-native["
        while not collected.endswith(marker):
            byte = self.output.get(timeout=self.remaining(8.0))
            if byte is None:
                raise RuntimeError(f"QEMU exited while answering {line!r}")
            collected.extend(byte)
        reply = bytes(collected[: -len(marker)]).decode("ascii")
        digits = bytearray()
        while True:
            byte = self.output.get(timeout=self.remaining(8.0))
            if byte is None:
                raise RuntimeError("QEMU exited inside a prompt")
            if byte == b"]":
                break
            digits.extend(byte)
        self.expect_exact(b"> ")
        return reply, int(digits.decode("ascii"))

    def expect_any(self, options: list[bytes], timeout: float = 8.0) -> int:
        """Wait until one of `options` has been seen; return its index."""
        deadline = time.monotonic() + self.remaining(timeout)
        window = bytearray()
        longest = max(len(option) for option in options)
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"waiting for one of {options!r}")
            byte = self.output.get(timeout=remaining)
            if byte is None:
                raise RuntimeError(f"QEMU exited while waiting for one of {options!r}")
            window.extend(byte)
            del window[:-longest]
            for index, option in enumerate(options):
                if window.endswith(option):
                    return index

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


def persistence_test(image: str, architecture: str, disk: str) -> None:
    """`image` boots the machine; `disk` is what the workspace lives on. On
    x86-64 they are the same file."""
    first = Harness(image, persistent=True, architecture=architecture, disk=disk)
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
            b"\r\nworkspace save failed: source candidate rejected; live world retained\r\n"
            b"agel-native[2]> "
        )
        first.send(
            ":delete bad",
            "cell deleted from staged workspace; :save to commit",
            2,
        )
        shutdown(first)
    finally:
        first.close()

    second = Harness(image, persistent=True, architecture=architecture, disk=disk)
    try:
        second.expect_until(b"AGEL_NATIVE_READY")
        second.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        second.expect_until(b"agel-native[1]> ")
        send_healthy(second, "persisted-answer", "42", 1, 2)
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
            4,
        )
        shutdown(second)
    finally:
        second.close()

    # Make generation 2 checksummed and structurally valid but semantically
    # invalid. Boot must reject its replay and try generation 1.
    with open(disk, "r+b") as media:
        media.seek(256 * 512)
        header = bytearray(media.read(512))
        length = int.from_bytes(header[24:28], "big")
        media.seek(257 * 512)
        payload = bytearray(media.read(15 * 512))
        old = b"(def persisted-answer 42)"
        new = b"(def persisted-answer zz)"
        position = payload[:length].find(old)
        if position < 0 or len(old) != len(new):
            raise RuntimeError("could not synthesize replay-invalid generation")
        payload[position : position + len(old)] = new
        header[28:32] = (
            zlib.crc32(header[:28] + payload[:length]) & 0xFFFFFFFF
        ).to_bytes(4, "big")
        media.seek(256 * 512)
        media.write(header)
        media.seek(257 * 512)
        media.write(payload)
        media.flush()

    third = Harness(image, persistent=True, architecture=architecture, disk=disk)
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
    with open(disk, "r+b") as media:
        media.seek(257 * 512)
        original = media.read(1)
        if len(original) != 1:
            raise RuntimeError("test image has no workspace payload sector")
        media.seek(257 * 512)
        media.write(bytes([original[0] ^ 0x80]))
        media.flush()

    fourth = Harness(image, persistent=True, architecture=architecture, disk=disk)
    try:
        fourth.expect_until(b"AGEL_NATIVE_READY")
        fourth.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        fourth.expect_until(b"agel-native[1]> ")
        # Generation 2 is gone from disk, so generation 1 is the candidate
        # again and this boot verifies it.
        send_healthy(fourth, "persisted-answer", "42", 1, 2)
        shutdown(fourth)
    finally:
        fourth.close()

    # Model a power loss after target-slot invalidation and a partial payload
    # write. With no published header, boot must ignore the torn generation.
    with open(disk, "r+b") as media:
        media.seek(256 * 512)
        media.write(bytes(512))
        media.write(b"partial-uncommitted-workspace")
        media.flush()

    fifth = Harness(image, persistent=True, architecture=architecture, disk=disk)
    try:
        fifth.expect_until(b"AGEL_NATIVE_READY")
        fifth.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        fifth.expect_until(b"agel-native[1]> ")
        fifth.send("persisted-answer", "42", 2)
        shutdown(fifth)
    finally:
        fifth.close()

    # The recovery plane. Generation 1 is the verified candidate and nothing is
    # trusted yet. A staged health cell that fails blocks explicit verification,
    # one that passes admits it, promotion makes generation 1 the rollback
    # point, and the next save becomes the candidate the boot budget judges.
    sixth = Harness(image, persistent=True, architecture=architecture, disk=disk)
    try:
        sixth.expect_until(b"AGEL_NATIVE_READY")
        sixth.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        sixth.expect_until(b"agel-native[1]> ")
        sixth.send(
            ":recovery-status",
            "recovery: trusted generation 0; candidate generation 1 (verified, boots 0)",
            1,
        )
        edit_cell(sixth, "health", "(/ persisted-answer 0)", 1)
        sixth.send(
            ":verify",
            "candidate generation 1: health cell rejected: error: division by zero",
            1,
        )
        edit_cell(sixth, "health", "(+ persisted-answer 0)", 1)
        sixth.send(":verify", "candidate generation 1: isolated health evidence accepted", 1)
        sixth.send(":promote", "selected generation 1; no earlier generation to retain", 1)
        sixth.send(":promote", "denied: no candidate generation", 1)
        sixth.send(":verify", "denied: no candidate generation to verify", 1)
        sixth.send(":recovery-status", "recovery: trusted generation 1; no candidate", 1)
        sixth.send(":delete health", "cell deleted from staged workspace; :save to commit", 1)
        edit_cell(sixth, "extra", "(def extra 7)", 1)
        sixth.send(
            ":save",
            "workspace generation 2 committed: 2 cells; evaluator rebuilt from cells; previous slot retained",
            2,
        )
        sixth.send(
            ":recovery-status",
            "recovery: trusted generation 1; candidate generation 2 (unverified, boots 0)",
            2,
        )
        sixth.send(":promote", "denied: verify candidate before promotion", 2)
        shutdown(sixth)
    finally:
        sixth.close()

    # Three boots that never evaluate a form exhaust the candidate's budget.
    for attempt in (1, 2, 3):
        boot = Harness(image, persistent=True, architecture=architecture, disk=disk)
        try:
            boot.expect_until(b"AGEL_NATIVE_READY")
            boot.expect_until(b"workspace generation 2 restored: 2 cells replayed")
            boot.expect_until(b"agel-native[2]> ")
            boot.send(
                ":recovery-status",
                f"recovery: trusted generation 1; candidate generation 2 (unverified, boots {attempt})",
                2,
            )
            shutdown(boot)
        finally:
            boot.close()

    # The fourth boot rolls back to the trusted generation on its own. A
    # healthy trusted generation says nothing about the candidate, and an
    # explicit fault keeps the exhausted candidate from booting again.
    rolled = Harness(image, persistent=True, architecture=architecture, disk=disk)
    try:
        rolled.expect_until(b"AGEL_NATIVE_READY")
        rolled.expect_until(
            b"watchdog fault: candidate generation 2 failed 3 boots; rolling back to generation 1"
        )
        rolled.expect_until(b"workspace generation 1 restored: 1 cells replayed")
        rolled.expect_until(b"agel-native[1]> ")
        rolled.send("persisted-answer", "42", 2)
        rolled.send(
            ":recovery-status",
            "recovery: trusted generation 1; candidate generation 2 (unverified, boots 3); running trusted generation after watchdog rollback",
            2,
        )
        rolled.send(":fault", "watchdog fault: rolled back to generation 1", 3)
        shutdown(rolled)
    finally:
        rolled.close()

    # Only explicit evidence revives an exhausted candidate: verify it from
    # the trusted generation, and the next boot replays it as verified.
    revived = Harness(image, persistent=True, architecture=architecture, disk=disk)
    try:
        revived.expect_until(b"AGEL_NATIVE_READY")
        revived.expect_until(b"watchdog fault: candidate generation 2 failed 3 boots")
        revived.expect_until(b"agel-native[1]> ")
        revived.send(":verify", "candidate generation 2: isolated health evidence accepted", 1)
        shutdown(revived)
    finally:
        revived.close()
    verified = Harness(image, persistent=True, architecture=architecture, disk=disk)
    try:
        verified.expect_until(b"AGEL_NATIVE_READY")
        verified.expect_until(b"workspace generation 2 restored: 2 cells replayed")
        verified.expect_until(b"agel-native[2]> ")
        verified.send("extra", "7", 3)
        verified.send(
            ":recovery-status",
            "recovery: trusted generation 1; candidate generation 2 (verified, boots 0)",
            3,
        )
        verified.send(":promote", "selected generation 2; generation 1 retained for rollback", 3)
        verified.send(":recovery-status", "recovery: trusted generation 2; no candidate", 3)
        shutdown(verified)
    finally:
        verified.close()


def kernel_rollback_test(image: str, kernel_path: str) -> None:
    import importlib.util
    import os

    location = os.path.join(os.path.dirname(os.path.abspath(__file__)), "stage-kernel.py")
    spec = importlib.util.spec_from_file_location("stage_kernel", location)
    assert spec is not None and spec.loader is not None
    stage_kernel = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(stage_kernel)
    with open(kernel_path, "rb") as file:
        good_kernel = file.read()
    # A candidate that halts at its entry point with interrupts off: the boot
    # stage jumps to it and nothing ever reaches the serial console.
    broken_kernel = b"\xf4\xeb\xfd"

    def selector() -> dict[str, int]:
        state = stage_kernel.read_selector(image)
        return {key: state[key] for key in ("trusted", "candidate", "attempts", "verified", "admitted")}

    good_signature = stage_kernel.sign(good_kernel)
    broken_signature = stage_kernel.sign(broken_kernel)
    def admission_boot(slot: str, trusted: str, running: str, revision: int = 0) -> None:
        """A boot of the trusted kernel that finds a staged candidate."""
        boot = Harness(image, persistent=True)
        try:
            boot.expect_until(b"AGEL_NATIVE_READY")
            boot.expect_until(
                f"candidate kernel slot {slot} admitted: signature verified against the kernel's trust key; next boot tries it".encode()
            )
            boot.expect_until(
                f"kernel: running slot {running}; trusted slot {trusted}; candidate slot {slot} (unverified, boots 0)".encode()
            )
            boot.expect_until(f"agel-native[{revision}]> ".encode())
            shutdown(boot)
        finally:
            boot.close()

    # No selector on disk: slot A, nothing proposed, nothing to promote.
    first = Harness(image, persistent=True)
    try:
        first.expect_until(b"AGEL_NATIVE_READY")
        first.expect_until(b"kernel: running slot A; trusted slot A; no candidate")
        first.expect_until(b"agel-native[0]> ")
        first.send(":kernel-promote", "denied: no candidate kernel slot", 0)
        first.send(":kernel-fault", "denied: no candidate kernel slot to give up", 0)
        shutdown(first)
    finally:
        first.close()

    # The same kernel staged as candidate B with a good signature: the stage
    # does not load it until the running kernel has admitted it.
    assert stage_kernel.stage(image, good_kernel, good_signature) == 1
    assert selector() == {"trusted": 0, "candidate": 1, "attempts": 0, "verified": 0, "admitted": 0}, selector()
    admission_boot("B", "A", "A")
    assert selector() == {"trusted": 0, "candidate": 1, "attempts": 0, "verified": 0, "admitted": 1}, selector()

    # Now the stage loads it, charged one boot; its first successful
    # evaluation verifies it, then it is promoted.
    second = Harness(image, persistent=True)
    try:
        second.expect_until(b"AGEL_NATIVE_READY")
        second.expect_until(
            b"kernel: running slot B; trusted slot A; candidate slot B (unverified, boots 1)"
        )
        second.expect_until(b"agel-native[0]> ")
        second.send(
            ":kernel-promote",
            "denied: the candidate kernel has not completed a healthy boot",
            0,
        )
        send_kernel_healthy(second, "(+ 1 1)", "2", "B", 1)
        second.send(
            ":kernel-status",
            "kernel: running slot B; trusted slot A; candidate slot B (verified, boots 0)",
            1,
        )
        second.send(":kernel-promote", "selected kernel slot B; slot A retained for rollback", 1)
        second.send(":kernel-status", "kernel: running slot B; trusted slot B; no candidate", 1)
        shutdown(second)
    finally:
        second.close()
    assert selector() == {"trusted": 1, "candidate": 0xFF, "attempts": 0, "verified": 0, "admitted": 0}, selector()

    # The stage now loads slot B by default.
    third = Harness(image, persistent=True)
    try:
        third.expect_until(b"AGEL_NATIVE_READY")
        third.expect_until(b"kernel: running slot B; trusted slot B; no candidate")
        third.expect_until(b"agel-native[0]> ")
        shutdown(third)
    finally:
        third.close()

    # A candidate without a valid signature is refused by the running kernel
    # and cleared, so the stage never loads it.
    assert stage_kernel.stage(image, broken_kernel, bytes(64)) == 0
    refused = Harness(image, persistent=True)
    try:
        refused.expect_until(b"AGEL_NATIVE_READY")
        refused.expect_until(
            b"candidate kernel slot A refused: signature does not verify against the kernel's trust key; slot cleared"
        )
        refused.expect_until(b"kernel: running slot B; trusted slot B; no candidate")
        refused.expect_until(b"agel-native[0]> ")
        shutdown(refused)
    finally:
        refused.close()
    assert selector() == {"trusted": 1, "candidate": 0xFF, "attempts": 0, "verified": 0, "admitted": 0}, selector()
    # A good signature over the wrong bytes is refused too.
    assert stage_kernel.stage(image, broken_kernel, good_signature) == 0
    mismatched = Harness(image, persistent=True)
    try:
        mismatched.expect_until(b"candidate kernel slot A refused: signature does not verify")
        mismatched.expect_until(b"agel-native[0]> ")
        shutdown(mismatched)
    finally:
        mismatched.close()

    # A signed kernel that never comes up, staged as candidate A and admitted.
    # Three boots reach nothing; each is charged by the boot stage before the
    # candidate runs.
    assert stage_kernel.stage(image, broken_kernel, broken_signature) == 0
    admission_boot("A", "B", "B")
    for attempt in (1, 2, 3):
        hung = Harness(image, persistent=True)
        try:
            try:
                hung.expect_until(b"AGEL_NATIVE_READY", timeout=4.0)
            except TimeoutError:
                pass
            else:
                raise RuntimeError("the halted candidate kernel reached the console")
        finally:
            hung.close()
        assert selector() == {"trusted": 1, "candidate": 0, "attempts": attempt, "verified": 0, "admitted": 1}, (
            attempt,
            selector(),
        )

    # The fourth boot is the trusted slot, and it says why.
    rolled = Harness(image, persistent=True)
    try:
        rolled.expect_until(b"AGEL_NATIVE_READY")
        rolled.expect_until(
            b"watchdog fault: candidate kernel slot A failed 3 boots; booted trusted slot B"
        )
        rolled.expect_until(
            b"kernel: running slot B; trusted slot B; candidate slot A (unverified, boots 3); running trusted slot after watchdog rollback"
        )
        rolled.expect_until(b"agel-native[0]> ")
        # A healthy trusted kernel is no evidence for the candidate.
        rolled.send("(+ 2 2)", "4", 1)
        rolled.send(
            ":kernel-fault",
            "watchdog fault: candidate kernel slot A given up; next boot loads trusted slot B",
            1,
        )
        shutdown(rolled)
    finally:
        rolled.close()
    assert selector() == {"trusted": 1, "candidate": 0, "attempts": 3, "verified": 0, "admitted": 1}, selector()

    # A fresh signed candidate in slot A is admitted, gets a fresh budget and
    # verifies itself.
    assert stage_kernel.stage(image, good_kernel, good_signature) == 0
    admission_boot("A", "B", "B")
    revived = Harness(image, persistent=True)
    try:
        revived.expect_until(b"AGEL_NATIVE_READY")
        revived.expect_until(
            b"kernel: running slot A; trusted slot B; candidate slot A (unverified, boots 1)"
        )
        revived.expect_until(b"agel-native[0]> ")
        send_kernel_healthy(revived, "(+ 1 2)", "3", "A", 1)
        shutdown(revived)
    finally:
        revived.close()
    assert selector() == {"trusted": 1, "candidate": 0, "attempts": 0, "verified": 1, "admitted": 1}, selector()


def power_cut_test(image: str, architecture: str, disk: str) -> None:
    """Cut the power at every sector write of a save, one boot per write, and
    require each reboot to land on a whole generation: the old one or the
    new one, never a torn one. The sweep ends at the first save the cut
    never reaches, which proves the cut was tried past the last write."""
    seed = Harness(image, persistent=True, architecture=architecture, disk=disk)
    try:
        seed.expect_until(b"AGEL_NATIVE_READY")
        seed.expect_until(b"workspace: no persisted image; starting empty")
        seed.expect_until(b"agel-native[0]> ")
        edit_cell(seed, "boot", "(def persisted-answer 42)", 0)
        seed.send(
            ":save",
            "workspace generation 1 committed: 1 cells; evaluator rebuilt from cells; previous slot retained",
            1,
        )
        shutdown(seed)
    finally:
        seed.close()

    committed_value = 42
    committed_generation = 1
    attempted: tuple[int, int] | None = None
    cuts = 0
    write = 1
    while True:
        boot = Harness(image, persistent=True, architecture=architecture, disk=disk)
        try:
            boot.expect_until(b"AGEL_NATIVE_READY")
            boot.expect_until(b"workspace generation ")
            boot.expect_until(b"agel-native[")
            boot.expect_until(b"]> ")
            # The first evaluation after a boot may also verify a candidate
            # generation and say so on a second line; the value is the first.
            value = boot.query("persisted-answer")[0].split("\r\n")[0]
            status, _ = boot.query(":workspace")
            allowed = {committed_value: committed_generation}
            if attempted is not None:
                allowed[attempted[0]] = attempted[1]
            if int(value) not in allowed:
                raise RuntimeError(
                    f"after a cut at write {write - 1} the workspace held {value}, not one of {sorted(allowed)}"
                )
            generation = allowed[int(value)]
            if not status.startswith(f"workspace generation {generation}, 1 cells, clean"):
                raise RuntimeError(f"generation and cell disagree: {status!r} for value {value}")
            committed_value, committed_generation = int(value), generation
            recovery, _ = boot.query(":recovery-status")
            if not recovery.startswith("recovery: "):
                raise RuntimeError(f"recovery record unreadable: {recovery!r}")
            armed, revision = boot.query(f":cut-power {write}")
            if armed != f"power cut armed: sector write {write} will be torn and the machine halted":
                raise RuntimeError(armed)
            new_value = 100 + write
            edit_cell(boot, "boot", f"(def persisted-answer {new_value})", revision)
            boot.send_bytes(":save")
            assert boot.process.stdin is not None
            boot.process.stdin.write(b"\n")
            boot.process.stdin.flush()
            outcome = boot.expect_any([b"power cut injected: sector ", b" committed: 1 cells"])
            if outcome == 0:
                boot.expect_until(b"torn; halting")
                exit_code = boot.process.wait(timeout=boot.remaining(8.0))
                if exit_code != EXPECTED_EXIT[architecture]:
                    raise RuntimeError(f"QEMU exit status {exit_code} after the cut")
                attempted = (new_value, committed_generation + 1)
                cuts += 1
                write += 1
                continue
            # The generation is published. The recovery record is written after
            # that, so the cut may still land on it: then the new generation
            # is the only acceptable one and the record reads as empty.
            outcome = boot.expect_any([b"power cut injected: sector ", b"]> "])
            if outcome == 0:
                boot.expect_until(b"torn; halting")
                exit_code = boot.process.wait(timeout=boot.remaining(8.0))
                if exit_code != EXPECTED_EXIT[architecture]:
                    raise RuntimeError(f"QEMU exit status {exit_code} after the cut")
                committed_value, committed_generation = new_value, committed_generation + 1
                attempted = None
                cuts += 1
                write += 1
                continue
            # The cut was never reached: the sweep covered every write of a save.
            shutdown(boot)
            break
        finally:
            boot.close()
    if cuts < 18:
        raise RuntimeError(f"only {cuts} writes were cut; a save has more")
    print(f"  {cuts} sector writes cut, one boot each; every reboot found a whole generation")


def send_kernel_healthy(
    harness: Harness, line: str, expected: str, slot: str, revision: int
) -> None:
    """The first form evaluated after a boot verifies a candidate kernel slot."""
    harness.send_bytes(line)
    assert harness.process.stdin is not None
    harness.process.stdin.write(b"\n")
    harness.process.stdin.flush()
    harness.expect_exact(
        f"\r\n{expected}\r\nkernel slot {slot} verified by a healthy boot"
        f"\r\nagel-native[{revision}]> ".encode("ascii")
    )


def send_healthy(
    harness: Harness, line: str, expected: str, generation: int, revision: int
) -> None:
    """The first form evaluated after a boot verifies an unverified candidate."""
    harness.send_bytes(line)
    assert harness.process.stdin is not None
    harness.process.stdin.write(b"\n")
    harness.process.stdin.flush()
    harness.expect_exact(
        f"\r\n{expected}\r\ncandidate generation {generation} verified by a healthy boot"
        f"\r\nagel-native[{revision}]> ".encode("ascii")
    )


def edit_cell(harness: Harness, name: str, source: str, revision: int) -> None:
    harness.send_bytes(f":edit {name}")
    assert harness.process.stdin is not None
    harness.process.stdin.write(b"\n")
    harness.process.stdin.flush()
    harness.expect_exact(f"\r\nedit[{name}]> ".encode("ascii"))
    harness.send(source, "cell staged; :run NAME to evaluate, :save to persist", revision)


def shutdown(harness: Harness) -> None:
    assert harness.process.stdin is not None
    harness.send_bytes(":shutdown")
    harness.process.stdin.write(b"\n")
    harness.process.stdin.flush()
    harness.expect_exact(b"\r\n")
    exit_code = harness.process.wait(timeout=harness.remaining(8.0))
    expected = EXPECTED_EXIT[harness.architecture]
    if exit_code != expected:
        raise RuntimeError(f"QEMU exit status {exit_code}, expected {expected}")


def main() -> int:
    arguments = sys.argv[1:]
    architecture = "x86_64"
    if "--arch" in arguments:
        index = arguments.index("--arch")
        if index + 1 >= len(arguments):
            print("--arch needs a value", file=sys.stderr)
            return 2
        architecture = arguments[index + 1]
        del arguments[index : index + 2]
    if architecture not in EXPECTED_EXIT:
        print(f"unknown architecture {architecture}", file=sys.stderr)
        return 2
    disk: str | None = None
    if "--disk" in arguments:
        index = arguments.index("--disk")
        if index + 1 >= len(arguments):
            print("--disk needs a value", file=sys.stderr)
            return 2
        disk = arguments[index + 1]
        del arguments[index : index + 2]
    if architecture == "x86_64":
        # The BIOS image is the disk.
        disk = arguments[0] if arguments else None
    if len(arguments) == 3 and arguments[1] == "--kernel-rollback":
        if architecture != "x86_64":
            print("kernel slots need the BIOS stage; only x86-64 has one", file=sys.stderr)
            return 2
        try:
            kernel_rollback_test(arguments[0], arguments[2])
        except Exception as error:
            print(f"kernel rollback test failed: {error}", file=sys.stderr)
            if LAST_HARNESS is not None:
                print(LAST_HARNESS.transcript.decode("utf-8", errors="replace"), file=sys.stderr)
            return 1
        print("Agel kernel slots: stage -> budgeted boots -> verify, promote, rollback [ok]")
        return 0
    if len(arguments) not in (1, 2):
        print(
            "usage: test-native-repl.py IMAGE [--persistence | --power-cut | --kernel-rollback KERNEL] [--arch ARCH] [--disk DISK]",
            file=sys.stderr,
        )
        return 2
    if len(arguments) == 2 and arguments[1] == "--power-cut":
        if disk is None:
            print("a power cut needs a disk; pass --disk", file=sys.stderr)
            return 2
        try:
            power_cut_test(arguments[0], architecture, disk)
        except Exception as error:
            print(f"power cut test failed: {error}", file=sys.stderr)
            if LAST_HARNESS is not None:
                print(LAST_HARNESS.transcript.decode("utf-8", errors="replace"), file=sys.stderr)
            return 1
        print(f"Agel native workspace [{architecture}]: a power cut at every sector write of a save leaves a whole generation [ok]")
        return 0
    if len(arguments) == 2:
        if arguments[1] != "--persistence":
            print("unknown test mode", file=sys.stderr)
            return 2
        if disk is None:
            print("persistence on this machine needs --disk", file=sys.stderr)
            return 2
        try:
            persistence_test(arguments[0], architecture, disk)
        except Exception as error:
            print(f"native persistence test failed: {error}", file=sys.stderr)
            if LAST_HARNESS is not None:
                print(LAST_HARNESS.transcript.decode("utf-8", errors="replace"), file=sys.stderr)
            return 1
        print(
            "Agel native workspace: edit -> reboot -> semantic, corruption, and torn-write fallback [ok]"
        )
        return 0
    scratch = None
    if disk is None:
        # A blank scratch disk, opened snapshot-on, so the diskless machines
        # run the same session as x86-64 without keeping anything.
        scratch = tempfile.NamedTemporaryFile(prefix="agel-scratch-", suffix=".img")
        scratch.write(bytes(2048 * 512))
        scratch.flush()
        disk = scratch.name
    harness = Harness(arguments[0], architecture=architecture, disk=disk)
    failure: Exception | None = None
    try:
        harness.expect_until(b"AGEL_NATIVE_READY")
        harness.expect_until(b"workspace: no persisted image; starting empty")
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
            "args=8 body=192 depth=24 fuel=2000 agents=8 mailbox=8 run-turns=32 scene-rects=12 "
            "cells=384 text=2048",
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
        # The disk-backed recovery plane with nothing on disk to select, on
        # every machine.
        harness.send(":verify", "denied: no candidate generation to verify", 29)
        harness.send(":promote", "denied: no candidate generation", 29)
        harness.send(":fault", "denied: no trusted generation to roll back to", 29)
        harness.send(":recovery-status", "recovery: no generation trusted or proposed", 29)
        harness.send("(let ((x 20) (y 22)) (+ x y))", "42", 30)
        harness.send("(let ((x 40)) (let ((x 1) (y x)) (+ x y)))", "41", 31)
        harness.send("(- (* 2 3 7) (+) (*) -1)", "42", 32)
        harness.send(
            "(def norm (fn (a b) (def last a) (- (* a a) (* b b))))",
            "#<native-function>",
            33,
        )
        harness.send("(norm 9 6)", "45", 34)
        harness.send("last", "9", 35)
        harness.send(
            "(/ 1)",
            "error: / expects at least two integers (transaction rolled back)",
            35,
        )
        harness.send("(list 1 (+ 20 22) 'x)", "(1 42 x)", 36)
        harness.send("(cons 0 '(1 2))", "(0 1 2)", 37)
        harness.send("(get (dict 'a 1 'b 2) 'b)", "2", 38)
        harness.send("(keys (assoc (dict 'a 1) 'b 2))", "(a b)", 39)
        harness.send('(text-concat "Ag" "el")', '"Agel"', 40)
        harness.send('(count "Ahoj svet")', "9", 41)
        harness.send("(= '(1 (2)) (list 1 (list 2)))", "#t", 42)
        harness.send("(def plan '(compile (core) \"v1\"))", "(compile (core) \"v1\")", 43)
        harness.send("(eval (cons '+ '(20 22)))", "42", 44)

        assert harness.process.stdin is not None
        for byte in b":shutdown":
            harness.process.stdin.write(bytes([byte]))
            harness.process.stdin.flush()
            harness.expect_exact(bytes([byte]), timeout=2.0)
        harness.process.stdin.write(b"\n")
        harness.process.stdin.flush()
        harness.expect_exact(b"\r\n")
        exit_code = harness.process.wait(timeout=harness.remaining(8.0))
        expected = EXPECTED_EXIT[architecture]
        if exit_code != expected:
            raise RuntimeError(f"QEMU exit status {exit_code}, expected {expected}")
    except Exception as error:  # test harness must always show the VM transcript
        failure = error
    finally:
        harness.close()
        if scratch is not None:
            scratch.close()
    if failure is not None:
        print(f"native REPL test failed: {failure}", file=sys.stderr)
        print(harness.transcript.decode("utf-8", errors="replace"), file=sys.stderr)
        return 1
    print(f"Agel native serial REPL [{architecture}]: synchronized end-to-end session [ok]")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

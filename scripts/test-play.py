"""Agel plays DOOM, and the loop is Agel in the OS.

The desktop loads `doom-agent`, an Agel program, into its native evaluator,
starts the engine, and `:play` steps it: each step the kernel pauses the
game, shows the Agel program the window and the engine's state line through
the `look` words, asks it which keys to hold, and injects them. No host
policy and no model: the perceive-decide-act loop is the Agel program's."""
import re
import socket
import sys
import tempfile
import time
from pathlib import Path
import graphical_console as module


def until_text(machine, wanted, timeout):
    result = bytearray()
    deadline = time.monotonic() + timeout
    machine.serial.settimeout(5)
    try:
        while wanted not in result:
            if time.monotonic() > deadline:
                raise TimeoutError(f"Agel did not write {wanted!r}: {bytes(result[-3000:])!r}")
            try:
                chunk = machine.serial.recv(4096)
            except socket.timeout:
                continue
            if not chunk:
                raise RuntimeError("Agel stopped")
            result.extend(chunk)
    finally:
        machine.serial.settimeout(15)
    return bytes(result)


def send_line(machine, line):
    """Type one command line without waiting for the prompt: `:play` runs
    for many seconds and speaks as it goes."""
    for byte in line.encode():
        machine.serial.sendall(bytes([byte]))
        machine.serial.recv(1)
    machine.serial.sendall(b"\n")


STEPS = 8

with tempfile.TemporaryDirectory(prefix="agel-play-", dir="/tmp") as directory:
    machine = module.Machine(sys.argv[1], directory)
    try:
        assert "formatted" in machine.submit(":fs-format")
        assert "DOOM AGENT READY" in machine.submit(":load doom-agent"), "the agent did not load"
        # The engine, started from the data region into a window that takes
        # the keyboard; the desktop keeps answering.
        response = machine.submit(":exec c-doom -- -iwad /data/doom1.wad -mb 8 -warp 1 -skill 2")
        assert "PROCESS RUNNING" in response, response
        until_text(machine, b"doom: frame 0 ", 900)
        time.sleep(3)
        # The Agel loop runs the game: a command line still reaches the
        # workshop though the window holds the keyboard, because it opens
        # with a colon.
        with machine.serial_lock:
            send_line(machine, f":play {STEPS} 4000")
            report = until_text(machine, f"PLAYED {STEPS} STEPS".encode(), 1800).decode(errors="replace")
        machine.serial.settimeout(15)
        steps = re.findall(r"play: step (\d+) keys (\([^)]*\)|\w+) reason", report)
        assert len(steps) == STEPS, f"expected {STEPS} steps, saw {steps!r} in {report[-2000:]!r}"
        assert any("up" in keys or "ctrl" in keys for _, keys in steps), report[-2000:]
        assert "doom: state map 1 " in report, "the agent never saw the engine's state"
        Path("target/play-in-os.png").write_bytes(machine.frame())
    finally:
        machine.close()
print(f"Agel plays DOOM from inside the OS: {STEPS} steps by the doom-agent Agel program, "
      "the window and state read through the look words, the keys it chose injected [ok]")

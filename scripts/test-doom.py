"""DOOM runs on the desktop: started with the shareware data from the data
region and a timed demo, it draws into its window while the desktop keeps
its prompt, reports the demo's frame rate on the console and exits."""
import re
import socket
import sys
import tempfile
import time
from pathlib import Path
import graphical_console as module


def until_text(machine, wanted, timeout):
    """Everything the console says until `wanted`, waiting through silence
    up to `timeout` seconds in all: a game that computes says nothing for
    a while."""
    result = bytearray()
    deadline = time.monotonic() + timeout
    machine.serial.settimeout(5)
    try:
        while wanted not in result:
            if time.monotonic() > deadline:
                raise TimeoutError(f"Agel did not write {wanted!r}: {bytes(result[-2000:])!r}")
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


with tempfile.TemporaryDirectory(prefix="agel-doom-", dir="/tmp") as directory:
    machine = module.Machine(sys.argv[1], directory)
    try:
        assert "formatted" in machine.submit(":fs-format")
        listing = machine.submit(":fs-ls /data")
        assert "doom1.wad  4196020 bytes" in listing, listing
        # The data file seeks and reads as the host sees it: the WAD's
        # header and the first entries of its directory.
        checked = machine.submit(":exec c-wadcheck")
        assert "wadcheck: length 4196020" in checked, checked
        assert "wadcheck: entry 0 got 16 name PLAYPAL at 12 size 10752" in checked, checked
        assert "wadcheck: entry 3 got 16 name DEMO1 at 23468 size 20118" in checked, checked
        response = machine.submit(":exec c-doom -- -iwad /data/doom1.wad -mb 8 -timedemo demo1")
        assert "PROCESS RUNNING" in response, response
        # The engine's banner and a first frame: the window is drawn where
        # the plain content was, and the desktop still answers.
        until_text(machine, b"doom: frame 350 ", 900)
        time.sleep(3)
        Path("target/doom-demo.png").write_bytes(machine.frame())
        content = machine.region(560, 160, 640, 400)
        plain = content.count(b"\x1b\x1b\x1b") * 3
        assert plain < len(content) // 2, "the window's content is still plain"
        report = until_text(machine, b"process c-doom exited", 1800).decode(errors="replace")
        timed = re.search(r"timed (\d+) gametics in (\d+) realtics", report)
        assert timed, report[-3000:]
        gametics, realtics = int(timed.group(1)), int(timed.group(2))
        fps = gametics * 35 / max(realtics, 1)
        print(f"doom: timed {gametics} gametics in {realtics} realtics, {fps:.1f} frames per second")
        until_text(machine, b"live-desktop> ", 30)
        Path("target/doom-end.png").write_bytes(machine.frame())
    finally:
        machine.close()
print("Agel runs DOOM: the shareware demo played from the data region, drawn into a window, timed [ok]")

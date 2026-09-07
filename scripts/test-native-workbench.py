#!/usr/bin/env python3
"""Native QEMU input, candidate isolation, source inspection and reboot proof."""
import importlib.util
import shutil
import sys
import tempfile
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location("console", Path(__file__).with_name("graphical-console.py"))
console = importlib.util.module_from_spec(spec)
spec.loader.exec_module(console)


def frame(machine):
    path = machine.directory / "frame.ppm"
    machine.command("screendump", {"filename": str(path), "format": "ppm"})
    return path.read_bytes().split(b"\n", 3)[3]


def dock_pixel(machine):
    data = frame(machine)
    offset = (645 * 1024 + 365) * 3
    return data[offset:offset + 3]


def value(machine, form, expected):
    result = machine.submit(form)
    assert f"\r\n{expected}\r\n" in result, (form, result)


with tempfile.TemporaryDirectory(prefix="agel-workbench-", dir="/tmp") as directory:
    image = Path(directory) / "disk.img"
    shutil.copyfile(sys.argv[1], image)
    with image.open("r+b") as disk:
        disk.seek(256 * 512)
        disk.write(bytes(32 * 512))
    machine = console.Machine(str(image), directory)
    try:
        assert "WORKBENCH READY" in machine.submit(":workbench")
        original = dock_pixel(machine)
        assert "CANDIDATE VALIDATED" in machine.submit("  :preview (point 360 640)  ")
        assert dock_pixel(machine) != original
        assert "CANDIDATE DISCARDED" in machine.submit(":discard")
        assert dock_pixel(machine) == original
        value(machine, "(inspect-agent)", 0)
        assert "CANDIDATE VALIDATED" in machine.submit(":preview (point 360 640)")
        assert "CANDIDATE PROMOTED" in machine.submit(":promote")
        assert dock_pixel(machine) != original
        assert "rolled back" in machine.submit(":rollback")
        assert dock_pixel(machine) == original
        machine.submit("(def broken (fn (self state message) (/ 1 0)))")
        assert "candidate agent turn failed" in machine.submit(":preview (begin (agent-become dock broken) (activate))")
        assert dock_pixel(machine) == original
        value(machine, "(agent-faulted? dock)", "#f")
        # Real PS/2 packets via QEMU, not the serial command path.
        machine.command("input-send-event", {"events": [
            {"type": "rel", "data": {"axis": "x", "value": -152}},
            {"type": "rel", "data": {"axis": "y", "value": 256}},
        ]})
        time.sleep(0.3)
        machine.command("input-send-event", {"events": [
            {"type": "btn", "data": {"down": True, "button": "left"}},
        ]})
        response = machine.until_prompt().decode()
        assert "\r\n1\r\n" in response, response
        machine.command("input-send-event", {"events": [
            {"type": "btn", "data": {"down": False, "button": "left"}},
        ]})
        value(machine, "(inspect-agent)", 1)
        machine.command("send-key", {"keys": [{"type": "qcode", "data": "tab"}]})
        time.sleep(0.3)
        value(machine, "selected", 2)
        machine.command("send-key", {"keys": [{"type": "qcode", "data": "ret"}]})
        assert "\r\n3\r\n" in machine.until_prompt().decode()
        source = machine.submit(":source 1")
        assert "(paint self (+ state message))" in source
        Path("target/native-workbench.png").write_bytes(machine.frame())
        machine.command("send-key", {"keys": [{"type": "qcode", "data": "esc"}]})
        time.sleep(0.3)
        # Successful replacement changes behavior, retaining the current state.
        machine.submit("(def twice (fn (self state message) (begin (paint self (+ state (* message 2))) (+ state (* message 2)))))")
        assert "CANDIDATE VALIDATED" in machine.submit(":preview (begin (agent-become dock twice) (activate))")
        assert "CANDIDATE PROMOTED" in machine.submit(":promote")
        value(machine, "(inspect-agent)", 7)
        assert "CELL STAGED" in machine.submit(":cell invalid (/ 1 0)")
        assert "source candidate rejected" in machine.submit(":save")
        value(machine, "(inspect-agent)", 7)
        assert "(* message 2)" in machine.submit(":source 1")
        assert "CELL DELETED" in machine.submit(":delete invalid")
        # Persist source, not a raw heap or ephemeral counter value.
        assert "CELL STAGED" in machine.submit(":cell wb-3 (def behavior (fn (self state message) (begin (paint self (+ state (* message 2))) (+ state (* message 2)))))")
        assert "SAVED GENERATION 1" in machine.submit(":save")
    finally:
        machine.close()
    machine = console.Machine(str(image), directory)
    try:
        value(machine, "(activate)", 2)
        assert "(* message 2)" in machine.submit(":source 1")
    finally:
        machine.close()
print("Native workbench: pointer -> focus -> inspect -> candidate -> recovery -> persisted upgrade [ok]")

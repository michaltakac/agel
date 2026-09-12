#!/usr/bin/env python3
"""Native QEMU input, candidate isolation, source inspection and reboot proof."""
import sys
import tempfile
import time
from pathlib import Path
import graphical_console as console


with tempfile.TemporaryDirectory(prefix="agel-workbench-", dir="/tmp") as directory:
    image = console.prepared_image(sys.argv[1], directory)
    machine = console.Machine(image, directory)
    try:
        assert "WORKBENCH READY" in machine.submit(":workbench")
        original = machine.region(365, 645, 1, 1)
        assert "CANDIDATE VALIDATED" in machine.submit("  :preview (point 360 640)  ")
        assert machine.region(365, 645, 1, 1) != original
        assert "CANDIDATE DISCARDED" in machine.submit(":discard")
        assert machine.region(365, 645, 1, 1) == original
        machine.expect("(inspect-agent)", 0)
        assert "CANDIDATE VALIDATED" in machine.submit(":preview (point 360 640)")
        assert "CANDIDATE PROMOTED" in machine.submit(":promote")
        assert machine.region(365, 645, 1, 1) != original
        assert "rolled back" in machine.submit(":rollback")
        assert machine.region(365, 645, 1, 1) == original
        machine.submit("(def broken (fn (self state message) (/ 1 0)))")
        assert "candidate agent turn failed" in machine.submit(":preview (begin (agent-become dock broken) (activate))")
        assert machine.region(365, 645, 1, 1) == original
        machine.expect("(agent-faulted? dock)", "#f")
        # Real PS/2 packets via QEMU, not the serial command path.
        machine.command("input-send-event", {"events": [
            # From the screen's centre (960, 540) to the widget at (360, 640).
            {"type": "rel", "data": {"axis": "x", "value": -600}},
            {"type": "rel", "data": {"axis": "y", "value": 100}},
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
        machine.expect("(inspect-agent)", 1)
        machine.command("send-key", {"keys": [{"type": "qcode", "data": "tab"}]})
        time.sleep(0.3)
        machine.expect("selected", 2)
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
        machine.expect("(inspect-agent)", 7)
        assert "CELL STAGED" in machine.submit(":cell invalid (/ 1 0)")
        assert "source candidate rejected" in machine.submit(":save")
        machine.expect("(inspect-agent)", 7)
        assert "(* message 2)" in machine.submit(":source 1")
        assert "CELL DELETED" in machine.submit(":delete invalid")
        # Persist source, not a raw heap or ephemeral counter value.
        assert "CELL STAGED" in machine.submit(":cell wb-3 (def behavior (fn (self state message) (begin (paint self (+ state (* message 2))) (+ state (* message 2)))))")
        assert "SAVED GENERATION 1" in machine.submit(":save")
    finally:
        machine.close()
    machine = console.Machine(str(image), directory)
    try:
        machine.expect("(activate)", 2)
        assert "(* message 2)" in machine.submit(":source 1")
    finally:
        machine.close()
print("Native workbench: pointer -> focus -> inspect -> candidate -> recovery -> persisted upgrade [ok]")

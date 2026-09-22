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
        # A click on the empty desktop opens the workbench: no form is
        # evaluated on a world with nothing loaded, nothing rolls back.
        machine.command("input-send-event", {"events": [
            {"type": "btn", "data": {"down": True, "button": "left"}},
        ]})
        opened = machine.until_prompt().decode()
        assert "WORKBENCH READY" in opened, opened
        assert "rolled back" not in opened, opened
        machine.command("input-send-event", {"events": [
            {"type": "btn", "data": {"down": False, "button": "left"}},
        ]})
        time.sleep(0.3)
        # A world of nothing but programs is replaced on a load: the
        # workbench reloads over itself. The operator's own cells would
        # keep it from doing so.
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
            # The pointer is absolute (QEMU's vmmouse): to the widget at
            # (360, 640), in 32768ths of the screen.
            {"type": "abs", "data": {"axis": "x", "value": 6147}},
            {"type": "abs", "data": {"axis": "y", "value": 19435}},
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
        # A sentence summons the agent beside the workbench: its six cells
        # join the workbench's nine, the sentence goes out with the
        # request as its task, and the judge's done ends the run.
        with machine.serial_lock:
            for byte in b"show me the help, please":
                machine.serial.sendall(bytes([byte]))
                machine.serial.recv(1)
            machine.serial.sendall(b"\n")
            buffer = bytearray()
            machine.serial.settimeout(5)
            deadline = time.monotonic() + 120
            while b"model-request end" not in buffer:
                assert time.monotonic() < deadline, bytes(buffer[-2000:])
                try:
                    buffer.extend(machine.serial.recv(4096))
                except TimeoutError:
                    continue
            block = buffer.decode(errors="replace")
            assert "rolled back" not in block, block
            assert "task: show me the help, please" in block, block
            assert "(choice act " in block, block
            number = int(block.split("model-request ")[1].split(" ")[0])
            machine.serial.sendall(f":model-reply {number} act choice 8 wait 0 0 0 0 0 0 0 0 0 done noul 950\n".encode())
            while b"DRIVE DONE AFTER 1 STEPS" not in buffer:
                assert time.monotonic() < deadline, bytes(buffer[-2000:])
                try:
                    buffer.extend(machine.serial.recv(4096))
                except TimeoutError:
                    continue
            while b"live-desktop> " not in buffer[-40:]:
                try:
                    buffer.extend(machine.serial.recv(4096))
                except TimeoutError:
                    break
            machine.serial.settimeout(15)
        cells = machine.submit(":cells")
        assert "CELLS 15" in cells and "dk-6" in cells and "wb-7" in cells, cells
        # The workbench still answers its own forms beside the agent; the
        # join replayed every cell, so the dock's state starts over.
        machine.expect("(inspect-agent)", 0)
    finally:
        machine.close()
    machine = console.Machine(str(image), directory)
    try:
        machine.expect("(activate)", 2)
        assert "(* message 2)" in machine.submit(":source 1")
    finally:
        machine.close()
print("Native workbench: pointer -> focus -> inspect -> candidate -> recovery -> persisted upgrade [ok]")

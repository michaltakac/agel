#!/usr/bin/env python3
"""Compile modules with native Agel, preview in real QEMU, persist and reboot."""
import sys
import tempfile
from pathlib import Path
import graphical_console as console

source = Path("examples/jit-module-dock.agel").read_text()


with tempfile.TemporaryDirectory(prefix="agel-modules-", dir="/tmp") as directory:
    image = console.prepared_image(sys.argv[1], directory)
    machine = console.Machine(image, directory)
    try:
        assert "WORKBENCH READY" in machine.submit(":workbench")
        machine.expect("(inspect-agent)", 0)
        initial = machine.frame()
        result = console.module_action(machine, source, "preview")
        assert "CANDIDATE VALIDATED" in result["result"], result
        assert machine.frame() != initial
        assert "CANDIDATE DISCARDED" in machine.submit(":discard")
        machine.expect("(inspect-agent)", 0)
        assert "CANDIDATE VALIDATED" in console.module_action(machine, source, "preview")["result"]
        assert "CANDIDATE PROMOTED" in machine.submit(":promote")
        machine.expect("(inspect-agent)", 2)
        machine.expect("(activate)", 4)
        assert "* message 2" in machine.submit(":source 1")
        # Host probe passes on zero state; real guest preview must reject on state 4.
        broken = "(module dock (export behavior) (def behavior (fn (s h m) (if (= h 0) 2 (/ 1 0)))))"
        failed = console.module_action(machine, broken, "preview")
        assert "candidate agent turn failed" in failed["result"], failed
        machine.expect("(inspect-agent)", 4)
        machine.expect("(agent-faulted? dock)", "#f")
        try:
            console.module_action(machine, "(module dock (import missing))", "preview")
            raise AssertionError("invalid module accepted")
        except ValueError:
            pass
        machine.expect("(inspect-agent)", 4)
        assert "CELL STAGED" in console.module_action(machine, source, "stage")["result"]
        assert "SAVED GENERATION 1" in machine.submit(":save")
        Path("target/native-modules.png").write_bytes(machine.frame())
    finally:
        machine.close()
    machine = console.Machine(str(image), directory)
    try:
        machine.expect("(activate)", 2)
        assert "* message 2" in machine.submit(":source 1")
    finally:
        machine.close()
print("Native module bridge: Agel expansion -> QEMU preview/discard/promote -> failure containment -> save/reboot [ok]")

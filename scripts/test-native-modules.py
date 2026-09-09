#!/usr/bin/env python3
"""Compile modules with native Agel, preview in real QEMU, persist and reboot."""
import importlib.util
import shutil
import sys
import tempfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("console", Path(__file__).with_name("graphical-console.py"))
console = importlib.util.module_from_spec(spec)
spec.loader.exec_module(console)
source = Path("examples/jit-module-dock.agel").read_text()


def value(machine, form, expected):
    result = machine.submit(form)
    assert f"\r\n{expected}\r\n" in result, (form, result)


with tempfile.TemporaryDirectory(prefix="agel-modules-", dir="/tmp") as directory:
    image = Path(directory) / "disk.img"
    shutil.copyfile(sys.argv[1], image)
    with image.open("r+b") as disk:
        disk.seek(256 * 512)
        disk.write(bytes(32 * 512))
    machine = console.Machine(str(image), directory)
    try:
        assert "WORKBENCH READY" in machine.submit(":workbench")
        value(machine, "(inspect-agent)", 0)
        initial = machine.frame()
        result = console.module_action(machine, source, "preview")
        assert "CANDIDATE VALIDATED" in result["result"], result
        assert machine.frame() != initial
        assert "CANDIDATE DISCARDED" in machine.submit(":discard")
        value(machine, "(inspect-agent)", 0)
        assert "CANDIDATE VALIDATED" in console.module_action(machine, source, "preview")["result"]
        assert "CANDIDATE PROMOTED" in machine.submit(":promote")
        value(machine, "(inspect-agent)", 2)
        value(machine, "(activate)", 4)
        assert "* message 2" in machine.submit(":source 1")
        # Host probe passes on zero state; real guest preview must reject on state 4.
        broken = "(module dock (export behavior) (def behavior (fn (s h m) (if (= h 0) 2 (/ 1 0)))))"
        failed = console.module_action(machine, broken, "preview")
        assert "candidate agent turn failed" in failed["result"], failed
        value(machine, "(inspect-agent)", 4)
        value(machine, "(agent-faulted? dock)", "#f")
        try:
            console.module_action(machine, "(module dock (import missing))", "preview")
            raise AssertionError("invalid module accepted")
        except ValueError:
            pass
        value(machine, "(inspect-agent)", 4)
        assert "CELL STAGED" in console.module_action(machine, source, "stage")["result"]
        assert "SAVED GENERATION 1" in machine.submit(":save")
        Path("target/native-modules.png").write_bytes(machine.frame())
    finally:
        machine.close()
    machine = console.Machine(str(image), directory)
    try:
        value(machine, "(activate)", 2)
        assert "* message 2" in machine.submit(":source 1")
    finally:
        machine.close()
print("Native module bridge: Agel expansion -> QEMU preview/discard/promote -> failure containment -> save/reboot [ok]")

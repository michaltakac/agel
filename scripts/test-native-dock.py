#!/usr/bin/env python3
"""Prove language-to-frame commits, agent rollback, and persisted dock replay."""
import sys
import tempfile
from pathlib import Path
import graphical_console as module


def pixels(machine):
    # Exclude the panel, whose clock turns, and the command field below the
    # scene: the language's drawing region is what these comparisons mean.
    return machine.region(0, 40, 1920, 960)


with tempfile.TemporaryDirectory(prefix="agel-dock-", dir="/tmp") as directory:
    image = module.prepared_image(sys.argv[1], directory)
    machine = module.Machine(image, directory)
    try:
        baseline = pixels(machine)
        sources = [line for line in Path("boot/desktop/dock.agel").read_text().splitlines() if line.startswith("(")]
        for index, source in enumerate(sources):
            assert "CELL STAGED" in machine.submit(f":cell dock-{index} {source}")
        machine.submit(":cell desktop (dock-paint 3423048)")
        assert "SAVED GENERATION 1" in machine.submit(":save")
        dock = pixels(machine)
        assert dock != baseline
        machine.submit("(def paint (fn (self state message) (begin (dock-paint message) message)))")
        machine.submit("(def dock (spawn paint 3423048))")
        machine.submit("(send dock 4609905)")
        machine.submit("(step)")
        assert pixels(machine) != dock
        machine.submit(":rollback")
        assert pixels(machine) == dock
        assert "error:" in machine.submit("(begin (scene-clear) (scene-rect 0 0 9999 80 10 123))")
        assert pixels(machine) == dock
        assert "\r\n6\r\n" in machine.submit("(scene-count)")
        Path("target/native-dock.png").write_bytes(machine.frame())
    finally:
        machine.close()
    machine = module.Machine(str(image), directory)
    try:
        assert pixels(machine) == dock
        assert "\r\n6\r\n" in machine.submit("(scene-count)")
        machine.submit("(scene-clear)")
        assert pixels(machine) == baseline
    finally:
        machine.close()
print("Native Agel dock: source -> pixels -> actor turn -> rollback -> reboot replay [ok]")

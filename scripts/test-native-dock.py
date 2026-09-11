#!/usr/bin/env python3
"""Prove language-to-frame commits, agent rollback, and persisted dock replay."""
import importlib.util
import shutil
import sys
import tempfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("console", Path(__file__).with_name("graphical-console.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def pixels(machine):
    frame = machine.directory / "scene.ppm"
    machine.command("screendump", {"filename": str(frame), "format": "ppm"})
    header, dimensions, maximum, data = frame.read_bytes().split(b"\n", 3)
    assert (header, dimensions, maximum) == (b"P6", b"1920 1080", b"255")
    # Exclude the panel, whose clock turns, and the command field below the
    # scene: the language's drawing region is what these comparisons mean.
    return data[1920 * 40 * 3 : 1920 * 1000 * 3]


with tempfile.TemporaryDirectory(prefix="agel-dock-", dir="/tmp") as directory:
    image = Path(directory) / "disk.img"
    shutil.copyfile(sys.argv[1], image)
    with image.open("r+b") as disk:
        disk.seek(1024 * 512)
        disk.write(bytes(32 * 512))
    machine = module.Machine(str(image), directory)
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

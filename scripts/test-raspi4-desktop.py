"""The desktop on QEMU's Raspberry Pi 4: the framebuffer from the firmware's
mailbox, the compositor domain on AArch64, the scene painted, commands from
the serial console, a program run from the card."""
import importlib.util
import shutil
import sys
import tempfile
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location("console", Path(__file__).with_name("graphical-console.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def region(machine, x, y, width, height):
    path = machine.directory / "frame.ppm"
    machine.command("screendump", {"filename": str(path), "format": "ppm"})
    header, data = path.read_bytes().split(b"\n", 3)[:3], path.read_bytes().split(b"\n", 3)[3]
    rows = []
    for row in range(y, y + height):
        start = (row * 1920 + x) * 3
        rows.append(data[start : start + width * 3])
    return header, b"".join(rows)


image = sys.argv[1]
card = sys.argv[2]
with tempfile.TemporaryDirectory(prefix="agel-pi-desktop-", dir="/tmp") as directory:
    qemu = [
        "qemu-system-aarch64", "-machine", "raspi4b", "-display", "none",
        "-qmp", f"unix:{directory}/qmp,server=on,wait=off",
        "-chardev", f"socket,id=serial0,path={directory}/serial,server=on,wait=on",
        "-serial", "chardev:serial0",
        "-drive", f"file={card},if=sd,format=raw",
        "-kernel", image,
    ]
    machine = module.Machine(card, directory, qemu=qemu)
    try:
        assert "aarch64-raspi4" in machine.boot, machine.boot
        assert "AGEL_GRAPHICS_OK" in machine.boot, machine.boot
        header, panel = region(machine, 0, 0, 1920, 40)
        assert header[1] == b"1920 1080", header
        # The panel is COSMIC's dark surface blended over the wallpaper, so
        # each channel is within a few units of its grey; the workspace pill
        # is opaque violet, which also proves the colour order.
        assert all(abs(byte - 0x1b) <= 4 for byte in panel[:3]), panel[:3]
        _, pill = region(machine, 96, 200, 8, 8)
        assert pill[:3] == b"\xe7\x9c\xfe", pill[:3]
        response = machine.submit("(accent cyan)")
        assert "COMMITTED REV 1" in response, response
        time.sleep(1.0)
        _, pill = region(machine, 96, 200, 8, 8)
        assert pill[:3] == b"\x63\xd0\xdf", pill[:3]
        response = machine.submit(":exec c-hello")
        assert "hello from C on Agel" in response, response
        assert "process c-hello exited with status 7" in response, response
        assert "42" in machine.submit("(+ 20 22)")
        Path("target/raspi4-desktop.png").write_bytes(machine.frame())
    finally:
        machine.close()
print("Agel desktop on the Raspberry Pi 4: mailbox framebuffer -> AArch64 compositor -> scene, commands, a program [ok]")

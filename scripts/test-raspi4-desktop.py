"""The desktop on QEMU's Raspberry Pi 4: the framebuffer from the firmware's
mailbox, the compositor domain on AArch64, the scene painted, commands from
the serial console, a program run from the card."""
import sys
import tempfile
import time
from pathlib import Path
import graphical_console as module


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
        panel = machine.region(0, 0, 1920, 40)
        # The panel is COSMIC's dark surface blended over the wallpaper, so
        # each channel is within a few units of its grey; the workspace pill
        # is opaque violet, which also proves the colour order.
        assert all(abs(byte - 0x1b) <= 4 for byte in panel[:3]), panel[:3]
        pill = machine.region(96, 200, 8, 8)
        assert pill[:3] == b"\xe7\x9c\xfe", pill[:3]
        response = machine.submit("(accent cyan)")
        assert "COMMITTED REV 1" in response, response
        time.sleep(1.0)
        pill = machine.region(96, 200, 8, 8)
        assert pill[:3] == b"\x63\xd0\xdf", pill[:3]
        response = machine.submit(":exec c-hello")
        assert "hello from C on Agel" in response, response
        assert "process c-hello exited with status 7" in response, response
        assert "42" in machine.submit("(+ 20 22)")
        Path("target/raspi4-desktop.png").write_bytes(machine.frame())
    finally:
        machine.close()
print("Agel desktop on the Raspberry Pi 4: mailbox framebuffer -> AArch64 compositor -> scene, commands, a program [ok]")

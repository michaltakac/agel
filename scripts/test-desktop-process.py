"""Programs run on the desktop: the graphical workshop's :exec and file
commands, answered on the serial console and drawn in the terminal panel."""
import importlib.util
import shutil
import sys
import tempfile
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location("console", Path(__file__).with_name("graphical-console.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def panel_region(machine, x, y, width, height):
    path = machine.directory / "frame.ppm"
    machine.command("screendump", {"filename": str(path), "format": "ppm"})
    data = path.read_bytes().split(b"\n", 3)[3]
    rows = []
    for row in range(y, y + height):
        start = (row * 1920 + x) * 3
        rows.append(data[start : start + width * 3])
    return b"".join(rows)


def panel(machine):
    """The terminal panel's pixels: rows 340 to 780, columns 476 to 1800."""
    path = machine.directory / "frame.ppm"
    machine.command("screendump", {"filename": str(path), "format": "ppm"})
    data = path.read_bytes().split(b"\n", 3)[3]
    rows = []
    for y in range(340, 780):
        start = (y * 1920 + 476) * 3
        rows.append(data[start : start + (1800 - 476) * 3])
    return b"".join(rows)


with tempfile.TemporaryDirectory(prefix="agel-desktop-process-", dir="/tmp") as directory:
    image = Path(directory) / "disk.img"
    shutil.copyfile(sys.argv[1], image)
    with image.open("r+b") as disk:
        # A blank workspace, records and filesystem region.
        disk.seek(1024 * 512)
        disk.write(bytes(1024 * 512))
    machine = module.Machine(str(image), directory)
    try:
        before = panel(machine)
        assert "formatted" in machine.submit(":fs-format")
        assert "directory ready: app" in machine.submit(":fs-mkdir app")
        assert "directory ready: etc" in machine.submit(":fs-mkdir etc")
        response = machine.submit(":exec writer")
        assert "writer: wrote etc/secret and app/notes" in response, response
        assert "process writer exited with status 0" in response, response
        response = machine.submit(":exec c-hello")
        assert "hello from C on Agel: a heap string of 13 bytes, 100% sure, ff hex" in response, response
        assert "process c-hello exited with status 7" in response, response
        response = machine.submit(":exec c-cat /app -- notes")
        assert "notes for the app" in response, response
        listing = machine.submit(":fs-ls /etc")
        assert "secret  11 bytes" in listing, listing
        after = panel(machine)
        assert after != before, "the terminal panel did not change"
        # The clock driver answered at boot.
        assert "clock: 20" in machine.boot, machine.boot
        # The desktop responds to the pointer: Applications opens the
        # launcher, an entry runs the program, a dock tile lists the root.
        def move_to(x, y):
            # A PS/2 packet carries at most 127 pixels per axis and the
            # controller's queue holds a few packets, so a long move is sent
            # as steps the guest can drain, as a real mouse would.
            move_to.at = getattr(move_to, "at", (960, 540))
            while move_to.at != (x, y):
                dx = max(-120, min(120, x - move_to.at[0]))
                dy = max(-120, min(120, y - move_to.at[1]))
                machine.command("input-send-event", {"events": [
                    {"type": "rel", "data": {"axis": "x", "value": dx}},
                    {"type": "rel", "data": {"axis": "y", "value": dy}},
                ]})
                move_to.at = (move_to.at[0] + dx, move_to.at[1] + dy)
                time.sleep(0.25)
            time.sleep(0.6)
        def click():
            machine.command("input-send-event", {"events": [{"type": "btn", "data": {"down": True, "button": "left"}}]})
            time.sleep(0.3)
            machine.command("input-send-event", {"events": [{"type": "btn", "data": {"down": False, "button": "left"}}]})
            time.sleep(0.3)
        move_to(60, 20)
        click()
        time.sleep(1.0)
        frame_with_launcher = panel_region(machine, 24, 48, 384, 400)
        move_to(200, 128)
        click()
        response = machine.until_prompt().decode()
        assert "writer: wrote etc/secret and app/notes" in response, response
        move_to(888, 964)
        click()
        response = machine.until_prompt().decode()
        assert "app/" in response and "etc/" in response, response
        assert frame_with_launcher != panel_region(machine, 24, 48, 384, 400), "the launcher did not close"
        # The workshop is still whole, and the frame is a real image.
        assert "42" in machine.submit("(+ 20 22)")
        assert machine.frame().startswith(b"\x89PNG\r\n\x1a\n")
        Path("target/desktop-process.png").write_bytes(machine.frame())
    finally:
        machine.close()
print("Agel desktop: programs run in the workshop window and answer on the serial console [ok]")

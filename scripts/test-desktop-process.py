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
        # A process owns a window: the chart asks for one, is refused a
        # rectangle past its edge, and draws its bars, which the supervisor
        # keeps and paints; the bars' colour is on the screen inside the
        # window's box and nowhere before it.
        def bar_pixels(x, y, width, height):
            return panel_region(machine, x, y, width, height).count(b"\x63\xd0\xdf")
        assert bar_pixels(560, 120, 480, 320) == 0, "bar colour before any window"
        response = machine.submit(":exec c-chart -- 3 7 5 9 outside")
        assert "chart: a rectangle outside the window was refused (errno 22)" in response, response
        assert "chart: window 0 shows 4 bars" in response, response
        assert "process c-chart exited with status 0" in response, response
        assert bar_pixels(560, 120, 480, 320) > 2000, "no bars in the first window"
        response = machine.submit(":exec c-chart -- 1 2")
        assert "chart: window 1 shows 2 bars" in response, response
        assert bar_pixels(624, 184, 480, 320) > 2000, "no bars in the second window"
        assert bar_pixels(1040, 184, 64, 320) > 0, "the second window's last bar is missing"
        Path("target/desktop-windows.png").write_bytes(machine.frame())
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
        # The second window's close control, at its header's right, closes
        # it as a typed :close; the first goes by the command itself.
        move_to(1080, 204)
        click()
        response = machine.until_prompt().decode()
        assert ":close 1" in response and "WINDOW CLOSED 1" in response, response
        assert bar_pixels(1040, 184, 64, 320) == 0, "the second window is still painted"
        assert "WINDOW CLOSED 0" in machine.submit(":close 0")
        assert bar_pixels(560, 120, 480, 320) == 0, "the first window is still painted"
        assert "NO SUCH WINDOW" in machine.submit(":close 0")
        # A window that listens: sketch waits for events, so the workshop
        # gets its prompt back while the process lives; a click in the
        # content becomes a dot and a console line, a key ends it.
        def until_text(wanted, timeout=30):
            result = bytearray()
            deadline = time.monotonic() + timeout
            while wanted not in result:
                if time.monotonic() > deadline or len(result) > 65536:
                    raise TimeoutError(f"Agel did not write {wanted!r}: {bytes(result)!r}")
                byte = machine.serial.recv(1)
                if not byte:
                    raise RuntimeError("Agel stopped")
                result.extend(byte)
            return bytes(result)
        response = machine.submit(":exec c-sketch")
        assert "PROCESS LISTENING" in response, response
        assert "exited" not in response, response
        def dot_pixels(x, y, width, height):
            return panel_region(machine, x, y, width, height).count(b"\xe7\x9c\xfe")
        assert dot_pixels(740, 280, 40, 40) == 0, "a dot before any press"
        def press():
            machine.command("input-send-event", {"events": [{"type": "btn", "data": {"down": True, "button": "left"}}]})
            time.sleep(0.4)
        def release():
            machine.command("input-send-event", {"events": [{"type": "btn", "data": {"down": False, "button": "left"}}]})
            time.sleep(0.4)
        move_to(760, 300)
        press()
        until_text(b"sketch: press at 200,140")
        time.sleep(1.0)
        assert dot_pixels(740, 280, 40, 40) > 300, "no dot where the press landed"
        # While the button is held the pointer is the window's: the dot
        # follows it, and the release fixes it where it is.
        move_to(820, 340)
        time.sleep(1.0)
        assert dot_pixels(800, 320, 40, 40) > 300, "the dot did not follow the pointer"
        assert dot_pixels(740, 280, 40, 40) == 0, "the dot left a trace"
        release()
        until_text(b"sketch: release at 260,180")
        Path("target/desktop-sketch.png").write_bytes(machine.frame())
        # A press in the header takes hold of the window: it follows the
        # pointer, dot and all, and the place it left is repainted.
        move_to(700, 140)
        press()
        move_to(900, 240)
        release()
        time.sleep(1.5)
        assert dot_pixels(1000, 420, 40, 40) > 300, "the window did not move with its header"
        assert dot_pixels(800, 320, 40, 40) == 0, "the window's old place was not repainted"
        Path("target/desktop-moved.png").write_bytes(machine.frame())
        # The window has the keyboard: a serial byte reaches the process,
        # not the workshop's line.
        machine.serial.sendall(b"q")
        response = until_text(b"live-desktop> ")
        assert "sketch: quit after 1 dots" in response.decode(), response
        assert "process c-sketch exited with status 0" in response.decode(), response
        assert "PROCESS ENDED" in response.decode(), response
        # Its window stays, with the dot, until closed; a window opened
        # later covers it, and a click on it brings it back to the front.
        assert dot_pixels(1000, 420, 40, 40) > 300, "the dot vanished with the process"
        response = machine.submit(":exec c-chart -- 1 2")
        assert "chart: window 1 shows 2 bars" in response, response
        assert dot_pixels(1000, 420, 40, 40) == 0, "the new window did not cover the old"
        move_to(1140, 540)
        click()
        time.sleep(1.5)
        assert dot_pixels(1000, 420, 40, 40) > 300, "the clicked window was not raised"
        assert "WINDOW CLOSED 0" in machine.submit(":close 0")
        assert "WINDOW CLOSED 1" in machine.submit(":close 1")
        # The workshop is still whole, and the frame is a real image.
        assert "42" in machine.submit("(+ 20 22)")
        assert machine.frame().startswith(b"\x89PNG\r\n\x1a\n")
        Path("target/desktop-process.png").write_bytes(machine.frame())
    finally:
        machine.close()
print("Agel desktop: programs run in the workshop window and answer on the serial console [ok]")

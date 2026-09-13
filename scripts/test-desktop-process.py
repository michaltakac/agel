"""Programs run on the desktop: the graphical workshop's :exec and file
commands, answered on the serial console and drawn in the terminal panel."""
import sys
import tempfile
import time
from pathlib import Path
import graphical_console as module


with tempfile.TemporaryDirectory(prefix="agel-desktop-process-", dir="/tmp") as directory:
    image = module.prepared_image(sys.argv[1], directory, 1024)
    machine = module.Machine(image, directory)
    try:
        # The terminal panel: rows 340 to 780, columns 476 to 1800.
        before = machine.region(476, 340, 1324, 440)
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
        after = machine.region(476, 340, 1324, 440)
        assert after != before, "the terminal panel did not change"
        # A process owns a window: the chart asks for one, is refused a
        # rectangle past its edge, and draws its bars, which the supervisor
        # keeps and paints; the bars' colour is on the screen inside the
        # window's box and nowhere before it.
        def bar_pixels(x, y, width, height):
            return machine.region(x, y, width, height).count(b"\x63\xd0\xdf")
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
        def press():
            machine.command("input-send-event", {"events": [{"type": "btn", "data": {"down": True, "button": "left"}}]})
            time.sleep(0.4)
        def release():
            machine.command("input-send-event", {"events": [{"type": "btn", "data": {"down": False, "button": "left"}}]})
            time.sleep(0.4)
        def click():
            machine.command("input-send-event", {"events": [{"type": "btn", "data": {"down": True, "button": "left"}}]})
            time.sleep(0.3)
            machine.command("input-send-event", {"events": [{"type": "btn", "data": {"down": False, "button": "left"}}]})
            time.sleep(0.3)
        launcher_closed = machine.region(24, 48, 384, 400)
        move_to(60, 20)
        click()
        # The launcher is painted after the click's command; under load
        # that takes longer than a fixed pause, so wait for the pixels.
        deadline = time.monotonic() + 20
        while (frame_with_launcher := machine.region(24, 48, 384, 400)) == launcher_closed:
            assert time.monotonic() < deadline, "the launcher did not open"
            time.sleep(0.5)
        move_to(200, 128)
        click()
        response = machine.until_prompt().decode()
        assert "writer: wrote etc/secret and app/notes" in response, response
        # The files tile lists the root; while the button is held the tile
        # is drawn pressed (darker), and the release restores it.
        def brightness(x, y, width, height):
            return sum(machine.region(x, y, width, height))
        move_to(888, 964)
        tile_before = brightness(860, 940, 56, 48)
        press()
        response = machine.until_prompt().decode()
        assert "app/" in response and "etc/" in response, response
        assert frame_with_launcher != machine.region(24, 48, 384, 400), "the launcher did not close"
        tile_pressed = brightness(860, 940, 56, 48)
        assert tile_pressed < tile_before * 9 // 10, (tile_before, tile_pressed)
        release()
        time.sleep(1.0)
        tile_after = brightness(860, 940, 56, 48)
        assert tile_after > tile_pressed, (tile_pressed, tile_after)
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
            return machine.region(x, y, width, height).count(b"\xe7\x9c\xfe")
        assert dot_pixels(740, 280, 40, 40) == 0, "a dot before any press"
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
        # The header's controls: maximize fills the screen below the panel
        # and tells the process its new size, again restores; minimize
        # leaves a pill in the panel, which brings the window back; the
        # corner resizes, and the release tells the process.
        def header_pixel():
            return machine.region(10, 60, 1, 1)[:3]
        assert header_pixel() != b"\x26\x26\x26", "the maximized header colour before maximize"
        move_to(904, 140)
        click()
        response = until_text(b"sketch: resized to 1920x920")
        assert ":maximize 0" in response.decode() and "WINDOW MAXIMIZED 0" in response.decode(), response
        time.sleep(1.0)
        assert header_pixel() == b"\x26\x26\x26", "the window did not fill the screen"
        move_to(1920 - 72 + 16, 40 + 4 + 16)
        click()
        response = until_text(b"sketch: resized to 400x300")
        assert "WINDOW RESTORED 0" in response.decode(), response
        time.sleep(1.0)
        assert header_pixel() != b"\x26\x26\x26", "the window did not restore"
        move_to(872, 140)
        click()
        response = until_text(b"live-desktop> ")
        assert ":minimize 0" in response.decode() and "WINDOW MINIMIZED 0" in response.decode(), response
        time.sleep(1.0)
        # The window's content is the darker surface; the workshop body
        # beneath it is the lighter one.
        assert machine.region(570, 280, 1, 1)[:3] == b"\x26\x26\x26", "the window is still painted"
        assert machine.region(380, 6, 200, 28).count(b"\x33\x33\x33") > 200, "no pill in the panel"
        move_to(400, 20)
        click()
        response = until_text(b"live-desktop> ")
        assert ":restore 0" in response.decode() and "WINDOW RESTORED 0" in response.decode(), response
        time.sleep(1.0)
        assert machine.region(570, 280, 1, 1)[:3] == b"\x1b\x1b\x1b", "the window did not come back"
        move_to(952, 452)
        press()
        move_to(1052, 502)
        release()
        response = until_text(b"sketch: resized to 500x350")
        time.sleep(1.0)
        assert machine.region(1050, 280, 1, 1)[:3] == b"\x1b\x1b\x1b", "the window did not grow"
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
        # Other programs run while the sketch listens: a chart opens its
        # window, draws, exits and is reported at once; the sketch keeps
        # its window, its dot and its events. A click on the wallpaper
        # gives the keyboard back to the workshop first.
        move_to(300, 700)
        click()
        machine.until_prompt()
        response = machine.submit(":exec c-chart -- 1 2")
        assert "chart: window 1 shows 2 bars" in response, response
        assert "process c-chart exited with status 0" in response, response
        assert "PROCESS ENDED" in response, response
        assert "WINDOW CLOSED 1" in machine.submit(":close 1")
        assert dot_pixels(1000, 420, 40, 40) > 300, "the dot did not outlast the chart"
        # A second sketch beside the first, each with its own window and
        # dot; the one with the keyboard quits alone and is reported.
        response = machine.submit(":exec c-sketch")
        assert "PROCESS LISTENING" in response, response
        assert dot_pixels(680, 280, 40, 40) == 0, "a dot in the second window before any press"
        move_to(700, 300)
        click()
        time.sleep(1.5)
        assert dot_pixels(680, 280, 40, 40) > 300, "no dot in the second window"
        Path("target/desktop-two-programs.png").write_bytes(machine.frame())
        machine.serial.sendall(b"q")
        response = until_text(b"live-desktop> ")
        assert "sketch: quit after 1 dots" in response.decode(), response
        assert "process c-sketch exited with status 0" in response.decode(), response
        assert "PROCESS ENDED" in response.decode(), response
        assert "WINDOW CLOSED 1" in machine.submit(":close 1")
        assert dot_pixels(1000, 420, 40, 40) > 300, "the first window's dot is gone"
        # The first sketch still answers: a press in its window is a line.
        move_to(1000, 420)
        press()
        until_text(b"sketch: press at ")
        release()
        until_text(b"sketch: release at ")
        # The window has the keyboard: a serial byte reaches the process,
        # not the workshop's line.
        machine.serial.sendall(b"q")
        response = until_text(b"live-desktop> ")
        assert "sketch: quit after 2 dots" in response.decode(), response
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
        # A canvas: pixels a process draws into pages of its own, blitted
        # at twice their size; the gradient and the last frame's bar are
        # read back from the screen at the window's content (560, 160).
        response = machine.submit(":exec c-canvas")
        assert "PROCESS LISTENING" in response, response
        assert "canvas: frame 0 in" in response, response
        assert machine.region(760, 260, 1, 1)[:3] == bytes([79, 64, 0x40]), "the gradient is wrong"
        assert machine.region(565, 260, 1, 1)[:3] == b"\xff\xff\xff", "the bar is missing"
        assert machine.region(1198, 558, 1, 1)[:3] == bytes([255, 255, 0x40]), "the far corner is wrong"
        Path("target/desktop-canvas.png").write_bytes(machine.frame())
        # The canvas goes with its process: the content is plain again.
        machine.serial.sendall(b"q")
        response = until_text(b"live-desktop> ").decode()
        assert "canvas: 1 frames" in response, response
        assert "process c-canvas exited with status 0" in response, response
        assert machine.region(760, 260, 1, 1)[:3] == b"\x1b\x1b\x1b", "the canvas outlived its process"
        assert "WINDOW CLOSED 0" in machine.submit(":close 0")
        # The workshop is still whole, and the frame is a real image.
        assert "42" in machine.submit("(+ 20 22)")
        assert machine.frame().startswith(b"\x89PNG\r\n\x1a\n")
        Path("target/desktop-process.png").write_bytes(machine.frame())
    finally:
        machine.close()
print("Agel desktop: programs run in the workshop window and answer on the serial console [ok]")

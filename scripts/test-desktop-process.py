"""Programs run on the desktop: the graphical workshop's :exec and file
commands, answered on the serial console and drawn in the terminal panel."""
import importlib.util
import shutil
import sys
import tempfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("console", Path(__file__).with_name("graphical-console.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


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
        # The workshop is still whole, and the frame is a real image.
        assert "42" in machine.submit("(+ 20 22)")
        assert machine.frame().startswith(b"\x89PNG\r\n\x1a\n")
        Path("target/desktop-process.png").write_bytes(machine.frame())
    finally:
        machine.close()
print("Agel desktop: programs run in the workshop window and answer on the serial console [ok]")

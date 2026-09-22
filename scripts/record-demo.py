#!/usr/bin/env python3
"""Record Agel and Jev using the desktop and playing DOOM, as an mp4.

    set -a; . ./.env; set +a
    python3 scripts/record-demo.py target/demo/agel-jev-demo.mp4

The desktop boots headless from a fresh copy of the seed image with the
game, its data and the hosted runtime installed; the host bridge attaches
to its serial console with the live judge (TypeSafe's Jev), and this
script uses the machine the way a person at the window would — QEMU's own
absolute mouse and keyboard, nothing typed on the serial console — while
it takes a screenshot twice a second. Afterwards every screenshot gets a
caption strip built from what the console said at that moment (what was
typed, what the judge answered, what Agel did), the long wait while the
game boots under emulation is sped up, and ffmpeg encodes the result.
Nothing in the video is staged: the judge's answers are the live ones.
"""
import json
import os
import shutil
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "boot/desktop/fonts"
SCREEN = (1920, 1080)
OUT_SIZE = (1280, 720)
STRIP = 104
SHOT_EVERY = 0.5

SENTENCES = [
    "list the files on the disk, then finish",
    "hi Jev, can you play DOOM for a minute?",
    "the game is running now, please play it",
]


def run(*command, **kwargs):
    return subprocess.run(command, check=True, cwd=ROOT, **kwargs)


def prepare(work):
    image = run("./scripts/build-boot.sh", "--features", "native-graphics", capture_output=True, text=True).stdout.strip().splitlines()[-1]
    doom = run("./scripts/build-c-program.sh", "doom", "x86_64", capture_output=True, text=True).stdout.strip().splitlines()[-1]
    wad = run("./scripts/fetch-doom-wad.sh", capture_output=True, text=True).stdout.strip().splitlines()[-1]
    agel = run("./scripts/build-program.sh", "agel", "x86_64", capture_output=True, text=True).stdout.strip().splitlines()[-1]
    run("cargo", "build", "-q", "--release", "-p", "agel-play")
    disk = work / "disk.img"
    shutil.copy(image, disk)
    with open(disk, "r+b") as file:
        file.seek(1024 * 512)
        file.write(bytes(1024 * 512))
    install = [sys.executable, "scripts/install-program.py"]
    run(*install, str(disk), "c-doom", doom, stdout=subprocess.DEVNULL)
    run(*install, "--region", "data", str(disk), "doom1.wad", wad, stdout=subprocess.DEVNULL)
    run(*install, str(disk), "agel", agel, stdout=subprocess.DEVNULL)
    return disk


class Qmp:
    def __init__(self, path):
        deadline = time.monotonic() + 30
        while True:
            try:
                self.socket = socket.socket(socket.AF_UNIX)
                self.socket.connect(str(path))
                break
            except OSError:
                if time.monotonic() > deadline:
                    raise
                time.sleep(0.2)
        self.file = self.socket.makefile("rw")
        self.lock = threading.Lock()
        self.file.readline()
        self.command("qmp_capabilities")

    def command(self, execute, arguments=None):
        with self.lock:
            request = {"execute": execute}
            if arguments is not None:
                request["arguments"] = arguments
            self.file.write(json.dumps(request) + "\n")
            self.file.flush()
            while True:
                reply = json.loads(self.file.readline())
                if "return" in reply:
                    return reply["return"]
                if "error" in reply:
                    raise RuntimeError(reply["error"])


class Console:
    """The bridge's output, line by line with the time each arrived."""

    def __init__(self, path):
        self.path = path
        self.events = []
        self.stop = False
        threading.Thread(target=self.follow, daemon=True).start()

    def follow(self):
        while not self.path.exists():
            time.sleep(0.1)
        with open(self.path, errors="replace") as file:
            pending = ""
            while not self.stop:
                chunk = file.readline()
                if not chunk:
                    time.sleep(0.05)
                    continue
                pending += chunk
                if pending.endswith("\n"):
                    self.events.append((time.monotonic(), pending.rstrip("\r\n")))
                    pending = ""

    def wait(self, needles, timeout, after=0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for stamp, line in list(self.events):
                if stamp >= after and any(needle in line for needle in needles):
                    return line
            time.sleep(0.2)
        raise TimeoutError(f"the desktop did not say any of {needles}")


class Recorder:
    def __init__(self, qmp, directory):
        self.qmp = qmp
        self.directory = directory
        self.shots = []
        self.stop = False
        self.thread = threading.Thread(target=self.loop, daemon=True)
        self.thread.start()

    def loop(self):
        index = 0
        while not self.stop:
            began = time.monotonic()
            path = self.directory / f"{index:06d}.png"
            try:
                self.qmp.command("screendump", {"filename": str(path), "format": "png"})
                self.shots.append((began, path))
                index += 1
            except Exception:
                pass
            time.sleep(max(0.0, SHOT_EVERY - (time.monotonic() - began)))


KEY_NAMES = {" ": "spc", ",": "comma", ".": "dot", "-": "minus", "'": "apostrophe", "/": "slash"}
SHIFTED = {"?": "slash", "!": "1", ":": "semicolon", '"': "apostrophe"}


def type_text(qmp, text, console=None):
    if console:
        # The echo can interleave with a program's output: what is typed,
        # and when, is the recorder's own record.
        console.events.append((time.monotonic(), "typed> " + text))
    for character in text:
        if character in SHIFTED:
            keys = ["shift", SHIFTED[character]]
        elif character.isupper():
            keys = ["shift", character.lower()]
        else:
            keys = [KEY_NAMES.get(character, character)]
        qmp.command("send-key", {"keys": [{"type": "qcode", "data": key} for key in keys]})
        time.sleep(0.09)
    qmp.command("send-key", {"keys": [{"type": "qcode", "data": "ret"}]})


def point(qmp, x, y):
    qmp.command("input-send-event", {"events": [
        {"type": "abs", "data": {"axis": "x", "value": round(x * 32767 / (SCREEN[0] - 1))}},
        {"type": "abs", "data": {"axis": "y", "value": round(y * 32767 / (SCREEN[1] - 1))}},
    ]})


def click(qmp):
    for down in (True, False):
        qmp.command("input-send-event", {"events": [{"type": "btn", "data": {"down": down, "button": "left"}}]})
        time.sleep(0.15)


def glide(qmp, start, end, seconds=1.2):
    steps = max(2, int(seconds / 0.04))
    for step in range(steps + 1):
        t = step / steps
        t = t * t * (3 - 2 * t)
        point(qmp, start[0] + (end[0] - start[0]) * t, start[1] + (end[1] - start[1]) * t)
        time.sleep(seconds / steps)


# --- captions from the console

KEYS = {"up": "forward", "down": "back", "left": "turn left", "right": "turn right", "ctrl": "fire", "space": "use"}


def permille(text):
    try:
        return int(text)
    except ValueError:
        return 0


def judgment(line):
    """`act choice N OPTION CONF P... [foe noul P] [risk score ...] [done noul P]`"""
    words = line.split()
    parts = []
    try:
        if words[:2] == ["act", "choice"]:
            count = int(words[2])
            option, confidence = words[3], permille(words[4])
            parts.append(f"next: {option} ({confidence / 10:.0f}% sure)")
            rest = words[5 + count:]
        else:
            rest = words
        while len(rest) >= 3:
            name, kind = rest[0], rest[1]
            if kind == "noul":
                p = permille(rest[2])
                label = {"foe": "enemy in view", "done": "task done"}.get(name, name)
                parts.append(f"{label} {p / 10:.0f}%")
                rest = rest[3:]
            elif kind == "score":
                count = int(rest[2])
                score = permille(rest[3])
                if name == "risk":
                    parts.append(f"risk {score / 1000:.1f} of 2")
                rest = rest[5 + count:]
            else:
                break
    except (IndexError, ValueError):
        return line
    return " · ".join(parts)


def captions(events):
    """What to show at each moment: (time, headline, judge line)."""
    state = {"headline": "Agel boots: an agentic OS with its agents inside it", "judge": ""}
    timeline = []
    for stamp, raw in events:
        line = raw.replace("live-desktop> ", "", 1) if raw.startswith("live-desktop> ") else raw
        if raw.startswith("typed> "):
            state["headline"] = f'You type: "{raw[len("typed> "):]}"'
            state["judge"] = "Agel summons its desktop agent; Jev, a System One model, judges each step"
        elif line.startswith("WORKBENCH READY"):
            state["headline"] = "A click on the desktop opens the workbench"
        elif line.startswith("agel-play: model reply"):
            answer = line.split(": ", 2)[-1]
            state["judge"] = "Jev: " + judgment(answer)
        elif line.startswith("drive: step "):
            command = line.split(" do ", 1)[-1].split(" reason ", 1)[0].strip()
            names = {":fs-ls /": "lists the files", ":help": "shows the help", "wait": "waits", "done": "is done"}
            if command.startswith(":exec c-doom"):
                what = "starts DOOM in a window"
            elif command.startswith(":handover"):
                what = "hands the desktop to its DOOM-playing agent"
            else:
                what = names.get(command, f"types {command}")
            state["headline"] = f"Agel {what}"
        elif line.startswith("HANDOVER "):
            state["headline"] = "Agel hands the desktop to its DOOM-playing agent"
        elif line.startswith("doom: frame 0"):
            state["headline"] = "DOOM is running on Agel"
        elif line.startswith("play: step "):
            rest = line[len("play: step "):]
            number = rest.split(" ", 1)[0]
            keys = rest.split("keys ", 1)[-1].split(" reason ", 1)[0].strip("() ")
            held = " + ".join(KEYS.get(key, key) for key in keys.split()) or "nothing"
            state["headline"] = f"Agel's DOOM agent, step {number}: {held}"
        elif line.startswith("PLAYED "):
            state["headline"] = f"Agel {line.lower()}: Jev judged every one"
        elif line.startswith(("DRIVE DONE", "DROVE ")):
            state["headline"] = "Agel: the task is done"
        else:
            continue
        timeline.append((stamp, state["headline"], state["judge"]))
    return timeline


def font(name, size):
    return ImageFont.truetype(str(FONTS / name), size)


# While the DOOM agent plays, the frame closes in on the game's window and
# the terminal lines under it, and opens out again when the play is over.
GAME_VIEW = (400, 100, 1360, 640)
ZOOM_SECONDS = 1.6


def view_at(stamp, zooms):
    amount = 0.0
    for start, end in zooms:
        if start <= stamp:
            amount = min(1.0, (stamp - start) / ZOOM_SECONDS)
        if end is not None and end <= stamp:
            amount = max(0.0, 1.0 - (stamp - end) / ZOOM_SECONDS)
    amount = amount * amount * (3 - 2 * amount)
    full = (0, 0, SCREEN[0], SCREEN[1])
    return tuple(round(f + (g - f) * amount) for f, g in zip(full, GAME_VIEW))


def compose(shots, timeline, directory, speeds, zooms=()):
    headline_font, judge_font, badge_font = font("FiraSans-Medium.ttf", 30), font("FiraMono-Regular.ttf", 21), font("FiraSans-Medium.ttf", 20)
    frames = []
    cursor = 0
    headline, judge = "Agel boots: an agentic OS with its agents inside it", ""
    for index, (stamp, path) in enumerate(shots):
        while cursor < len(timeline) and timeline[cursor][0] <= stamp:
            _, headline, judge = timeline[cursor]
            cursor += 1
        try:
            shot = Image.open(path).convert("RGB")
        except OSError:
            continue
        canvas = Image.new("RGB", (OUT_SIZE[0], OUT_SIZE[1] + STRIP), (16, 20, 28))
        canvas.paste(shot.crop(view_at(stamp, zooms)).resize(OUT_SIZE, Image.LANCZOS), (0, 0))
        draw = ImageDraw.Draw(canvas)
        draw.rectangle((0, OUT_SIZE[1], OUT_SIZE[0], OUT_SIZE[1] + 3), fill=(84, 196, 255))
        draw.rounded_rectangle((20, OUT_SIZE[1] + 20, 150, OUT_SIZE[1] + 52), 10, fill=(84, 196, 255))
        draw.text((32, OUT_SIZE[1] + 24), "Agel + Jev", font=badge_font, fill=(10, 14, 20))
        draw.text((170, OUT_SIZE[1] + 16), headline[:78], font=headline_font, fill=(240, 244, 250))
        draw.text((170, OUT_SIZE[1] + 60), judge[:96], font=judge_font, fill=(150, 214, 255))
        speed = 1.0
        for start, end, factor in speeds:
            if start <= stamp < end:
                speed = factor
        following = shots[index + 1][0] if index + 1 < len(shots) else stamp + SHOT_EVERY
        output = directory / f"{len(frames):06d}.jpg"
        canvas.save(output, quality=90)
        frames.append((output, max(0.02, (following - stamp) / speed)))
    return frames


def card(path, title, lines):
    canvas = Image.new("RGB", (OUT_SIZE[0], OUT_SIZE[1] + STRIP), (12, 15, 22))
    draw = ImageDraw.Draw(canvas)
    draw.text((96, 250), title, font=font("FiraSans-Medium.ttf", 64), fill=(240, 244, 250))
    y = 350
    for line in lines:
        draw.text((96, y), line, font=font("FiraSans-Regular.ttf", 30), fill=(160, 196, 230))
        y += 46
    draw.rectangle((96, 226, 196, 232), fill=(84, 196, 255))
    canvas.save(path, quality=92)


def encode(frames, output, work):
    listing = work / "frames.txt"
    with open(listing, "w") as file:
        for path, duration in frames:
            file.write(f"file '{path}'\nduration {duration:.4f}\n")
        file.write(f"file '{frames[-1][0]}'\n")
    run("ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(listing),
        "-vf", "fps=30,format=yuv420p", "-c:v", "libx264", "-preset", "slow", "-crf", "24",
        "-movflags", "+faststart", str(output))


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    output = Path(sys.argv[1]).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    if not os.environ.get("TYPESAFEAI_API_KEY"):
        raise SystemExit("TYPESAFEAI_API_KEY is not in the environment: set -a; . ./.env; set +a")
    work = output.parent / (output.stem + "-work")
    shutil.rmtree(work, ignore_errors=True)
    (work / "shots").mkdir(parents=True)
    (work / "frames").mkdir()
    disk = prepare(work)
    qemu = subprocess.Popen(["qemu-system-x86_64", "-machine", "pc,accel=tcg", "-m", "64M", "-monitor", "none",
                             "-display", "none", "-no-reboot", "-vga", "std", "-boot", "order=c,strict=on",
                             "-qmp", f"unix:{work}/qmp,server=on,wait=off",
                             "-chardev", f"socket,id=serial0,path={work}/serial,server=on,wait=off",
                             "-serial", "chardev:serial0",
                             "-drive", f"format=raw,file={disk},if=ide,index=0,media=disk"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    bridge = recorder = console = None
    try:
        qmp = Qmp(work / "qmp")
        log = work / "bridge.log"
        bridge = subprocess.Popen([str(ROOT / "target/release/agel-play"), "--attach", str(work / "serial"),
                                   "--scene", "desktop", "--policy", "jev", "--out", str(work / "run")],
                                  cwd=ROOT, stdout=open(log, "w"), stderr=subprocess.STDOUT)
        console = Console(log)
        recorder = Recorder(qmp, work / "shots")
        begun = time.monotonic()
        booted = start_time = frame_time = begun
        # The prompt ends without a newline; the boot's last whole line is
        # the filesystem's (formatted or found) or the clock's.
        console.wait(["filesystem: a blank region formatted", "clock: "], 180)
        time.sleep(1)
        booted = time.monotonic()
        time.sleep(2)
        glide(qmp, (1500, 250), (1010, 500))
        time.sleep(0.6)
        click(qmp)
        console.wait(["WORKBENCH READY"], 60)
        time.sleep(3)
        glide(qmp, (1010, 500), (380, 646), 1.0)
        time.sleep(0.5)
        click(qmp)
        time.sleep(2.5)
        mark = time.monotonic()
        type_text(qmp, SENTENCES[0], console)
        console.wait(["DRIVE DONE", "DROVE "], 240, after=mark)
        time.sleep(3)
        glide(qmp, (380, 646), (1860, 60), 0.8)
        mark = time.monotonic()
        type_text(qmp, SENTENCES[1], console)
        console.wait([":exec c-doom"], 240, after=mark)
        start_time = time.monotonic()
        console.wait(["doom: frame 0"], 900, after=mark)
        frame_time = time.monotonic()
        # The judge hands over once it sees the game running; if the eight
        # summoned steps ran out while the game was still booting, a person
        # would ask again, and so does this script, at the prompt.
        ended = console.wait(["do :handover", "DROVE ", "DRIVE DONE"], 600, after=mark)
        if ":handover" not in ended:
            time.sleep(2)
            # The game's window holds the keyboard now: a click on the
            # desktop beside it gives the prompt the keyboard back.
            glide(qmp, (1860, 60), (1560, 560), 0.8)
            click(qmp)
            time.sleep(1.5)
            glide(qmp, (1560, 560), (1860, 60), 0.6)
            again = time.monotonic()
            type_text(qmp, SENTENCES[2], console)
            console.wait(["do :handover"], 300, after=again)
        console.wait(["PLAYED ", "PROCESS ENDED"], 1800, after=frame_time)
        time.sleep(4)
    except TimeoutError as error:
        print(f"record-demo: {error}; composing what was recorded", file=sys.stderr)
    finally:
        if recorder:
            recorder.stop = True
            recorder.thread.join(5)
        if console:
            console.stop = True
        if bridge:
            bridge.terminate()
        qemu.terminate()
        qemu.wait(10)
    # The waits sped up: the boot, and the game booting under emulation.
    speeds = [(0, booted - 1, 6.0), (start_time + 3, frame_time - 2, 12.0)]
    (work / "timings.json").write_text(json.dumps({
        "events": console.events,
        "shots": [(stamp, str(path)) for stamp, path in recorder.shots],
        "speeds": speeds,
    }))
    finish(work, output, console.events, recorder.shots, speeds)
    print(f"recorded {time.monotonic() - begun:.0f} s; the console transcript is {log}")


def finish(work, output, events, shots, speeds):
    shots = [(stamp, Path(path)) for stamp, path in shots]
    shutil.rmtree(work / "frames", ignore_errors=True)
    (work / "frames").mkdir()
    timeline = captions(events)
    handover = next((stamp for stamp, line in events if "do :handover" in line), None)
    played = next((stamp for stamp, line in events if line.startswith(("PLAYED ", "PROCESS ENDED"))), None)
    zooms = [(handover + 2.0, played + 1.0 if played else None)] if handover else []
    frames = compose(shots, timeline, work / "frames", speeds, zooms)
    opening, closing = work / "frames" / "opening.jpg", work / "frames" / "closing.jpg"
    card(opening, "Agel + Jev", [
        "Agel is an agentic operating system: agents are part of the system.",
        "Jev is TypeSafe's System One model: typed judgments in milliseconds.",
        "Everything below is live: the judge's answers are its own.",
    ])
    card(closing, "What you saw", [
        "A person clicks and types; the OS summons its agent for a sentence.",
        "Jev judges which command comes next; Agel types it, inside the OS.",
        "DOOM, compiled for Agel, played step by step by an agent written in Agel.",
        "github.com/michaltakac/agel",
    ])
    frames = [(opening, 5.0)] + frames + [(closing, 6.0)]
    encode(frames, output, work)
    print(f"{output}: {len(frames)} frames, {sum(duration for _, duration in frames):.0f} s of video")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--recompose":
        # Captions and timing again from a recording's saved timings.
        target = Path(sys.argv[2]).resolve()
        work = target.parent / (target.stem + "-work")
        saved = json.loads((work / "timings.json").read_text())
        finish(work, target, [tuple(event) for event in saved["events"]], saved["shots"], saved["speeds"])
    else:
        main()

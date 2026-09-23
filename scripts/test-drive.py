"""An Agel program drives the desktop from inside the OS.

The desktop loads `desktop-agent` into its native evaluator and `:drive`
steps it: each step the kernel shows the program the desktop's own state
through the look line — windows, focus, whether a process runs, the last
line the terminal finished — asks it for one command line, relays the
typed questions it asks on the serial console, and types the line the
program decided on as the operator would. This harness answers as the
host bridge would, with typed judgments, and checks that what the program
decided was done and that the program saw what it did."""
import re
import socket
import sys
import tempfile
import time
import graphical_console as module


def until_text(machine, wanted, timeout):
    result = bytearray()
    deadline = time.monotonic() + timeout
    machine.serial.settimeout(5)
    try:
        while wanted not in result:
            if time.monotonic() > deadline:
                raise TimeoutError(f"Agel did not write {wanted!r}: {bytes(result[-3000:])!r}")
            try:
                chunk = machine.serial.recv(4096)
            except socket.timeout:
                continue
            if not chunk:
                raise RuntimeError("Agel stopped")
            result.extend(chunk)
    finally:
        machine.serial.settimeout(15)
    return bytes(result)


def send_line(machine, line):
    """Type one line without waiting for the prompt: `:drive` speaks as it
    goes and waits for `:model-reply` lines while it does."""
    for byte in line.encode():
        machine.serial.sendall(bytes([byte]))
        machine.serial.recv(1)
    machine.serial.sendall(b"\n")


def request(machine, timeout=120):
    """The next request block the desktop relays: its number and its
    look line."""
    block = until_text(machine, b"model-request end", timeout).decode(errors="replace")
    number = int(re.search(r"model-request (\d+) ", block).group(1))
    line = re.search(r"look-line: (.*)\r?\n", block).group(1)
    assert "(judge (choice act " in block, block
    assert "(noul done " in block, block
    return number, line, block


def settled(machine, text):
    """Back at the prompt: it follows the loop's last status at once, so
    it is usually inside `text` already, or split between `text` and what
    is still to come."""
    buffer = bytearray(text[-20:].encode(errors="replace"))
    deadline = time.monotonic() + 30
    machine.serial.settimeout(5)
    try:
        while b"live-desktop> " not in buffer:
            if time.monotonic() > deadline:
                raise TimeoutError(f"no prompt after the loop: {bytes(buffer)!r}")
            try:
                buffer.extend(machine.serial.recv(4096))
            except socket.timeout:
                continue
    finally:
        machine.serial.settimeout(15)


def answer(machine, number, option, confidence, done):
    """The reply line, sent whole: the desktop reads it without echoing
    while it waits, as the bridge sends it."""
    probabilities = " ".join("0" for _ in range(9))
    line = f":model-reply {number} act choice 9 {option} {confidence} {probabilities} done noul {done}\n"
    machine.serial.sendall(line.encode())


with tempfile.TemporaryDirectory(prefix="agel-drive-", dir="/tmp") as directory:
    machine = module.Machine(sys.argv[1], directory)
    try:
        # A blank region was formatted at boot: the listing answers, no
        # error number; formatting again is still allowed.
        listing = machine.submit(":fs-ls /")
        assert "error" not in listing, listing
        assert "formatted" in machine.submit(":fs-format")
        # Nothing to drive yet: the loop says so and does nothing; Tab on
        # an empty world says what to do instead of failing a form; a lone
        # word that names nothing is told how to reach the agent.
        assert "NO PROGRAM TO DRIVE" in machine.submit(":drive 2")
        machine.serial.sendall(b"\t")
        assert "NOTHING TO FOCUS" in machine.until_prompt().decode(errors="replace")
        assert "UNBOUND WORD - A SENTENCE SUMMONS THE AGENT" in machine.submit("hello")
        assert "DESKTOP AGENT READY" in machine.submit(":load desktop-agent"), "the agent did not load"
        with machine.serial_lock:
            send_line(machine, ":drive 4 2000")
            # Step 1: the program sees an empty desktop and asks; the judge
            # says list the files, confidently, and the task is not done.
            number, line, _ = request(machine)
            assert line.startswith("win 0 focus none | run no | last: "), line
            answer(machine, number, "files", 700, 100)
            report = until_text(machine, b"drive: step 1 do ", 60).decode(errors="replace")
            report += until_text(machine, b"drive: ", 60).decode(errors="replace")
            # Step 2: the listing was typed; the program sees the desktop's
            # answer to it as the last line. An unsure judge means waiting.
            number, line, _ = request(machine)
            assert "last: drive: " in line, line
            answer(machine, number, "help", 150, 100)
            until_text(machine, b"drive: step 2 do wait", 60)
            # Step 3: a command the program may not type is refused.
            number, line, _ = request(machine)
            answer(machine, number, "maximize", 900, 100)
            until_text(machine, b"drive: step 3 do :maximize 0", 60)
            # Step 4: the judge says the task is complete.
            number, line, _ = request(machine)
            assert "last: drive: " in line, line
            answer(machine, number, "wait", 0, 950)
            tail = until_text(machine, b"DRIVE DONE AFTER 4 STEPS", 60).decode(errors="replace")
        machine.serial.settimeout(15)
        assert "drive: step 1 do :fs-ls /" in report, report[-2000:]
        assert "drive: step 4 do done" in tail, tail[-2000:]
        assert re.search(r'reason "act choice 9 wait 0 (0 ){9}done noul 950"', tail), tail[-2000:]
        settled(machine, tail)
        # The loop ended and the desktop answers again; a whole run of
        # steps without a completing judge is reported as driven.
        with machine.serial_lock:
            send_line(machine, ":drive 1")
            number, line, _ = request(machine)
            answer(machine, number, "files", 800, 200)
            tail = until_text(machine, b"DROVE 1 STEPS", 60).decode(errors="replace")
        machine.serial.settimeout(15)
        assert "drive: step 1 do :fs-ls /" in tail, tail[-2000:]
        settled(machine, tail)
        # A judge that answers an error instead of a judgment: the program
        # reads no confidence in it, waits, and asks again the next step
        # (the answer has fewer fields than a judgment, and reading a
        # missing one must not fail the form, or the loop stops).
        with machine.serial_lock:
            send_line(machine, ":drive 2")
            number, line, _ = request(machine)
            machine.serial.sendall(
                f":model-reply {number} error provider answered unexpectedly: a probability is not a number: null\n".encode()
            )
            until_text(machine, b"drive: step 1 do wait", 60)
            again, line, _ = request(machine)
            assert again == number + 1, (number, again)
            answer(machine, again, "files", 800, 200)
            tail = until_text(machine, b"DROVE 2 STEPS", 60).decode(errors="replace")
        machine.serial.settimeout(15)
        assert "drive: step 2 do :fs-ls /" in tail, tail[-2000:]
        settled(machine, tail)
        # The refused commands: the loops that would nest, and the halt.
        machine.submit('(def command-for (fn (a) ":shutdown"))')
        with machine.serial_lock:
            send_line(machine, ":drive 1")
            number, line, _ = request(machine)
            answer(machine, number, "help", 900, 0)
            tail = until_text(machine, b"DROVE 1 STEPS", 60).decode(errors="replace")
        machine.serial.settimeout(15)
        assert "drive: step 1 do :shutdown" in tail and "drive: REFUSED" in tail, tail[-2000:]
        settled(machine, tail)
        # A sentence at the prompt summons the agent: the loaded agent
        # drives for it, the sentence relayed with every request as its
        # task line, and the judge's done ends the run.
        machine.submit('(def command-for (fn (a) ":help"))')
        with machine.serial_lock:
            send_line(machine, "show me the help and then finish")
            number, line, block = request(machine)
            assert "task: show me the help and then finish" in block, block
            answer(machine, number, "help", 900, 100)
            until_text(machine, b"drive: step 1 do :help", 60)
            number, line, block = request(machine)
            assert "task: show me the help and then finish" in block, block
            answer(machine, number, "wait", 0, 900)
            tail = until_text(machine, b"DRIVE DONE AFTER 2 STEPS", 60).decode(errors="replace")
        machine.serial.settimeout(15)
        settled(machine, tail)
        # Agents side by side: a program that defines NAME-step and
        # NAME-needs is an agent for `:agents`; it steps only when its needs
        # are met (here a file present), and the run ends when every agent
        # has said done.
        machine.submit('(def dk-step (fn () (drive-step)))')
        machine.submit('(def dk-needs (fn () \'((file "seen"))))')
        waited = machine.submit(":agents 2")
        assert "agents: step 1 dk waits file seen" in waited, waited[-2000:]
        assert "AGENTS RAN 0 STEPS" in waited, waited[-2000:]
        machine.submit('(file-write "seen" "x")')
        machine.submit('(def command-for (fn (a) ":fs-ls /"))')
        with machine.serial_lock:
            send_line(machine, ":agents 3")
            number, line, block = request(machine)
            assert "task: show me the help and then finish" in block, block
            answer(machine, number, "files", 900, 100)
            until_text(machine, b"agents: step 1 dk do :fs-ls /", 60)
            # A sentence said while agents run is heard between steps: it
            # becomes the task the next request carries and the file `task`.
            send_line(machine, "now count the files instead")
            number, line, block = request(machine)
            assert "agents: heard now count the files instead" in block, block
            assert "task: now count the files instead" in block, block
            answer(machine, number, "wait", 0, 900)
            tail = until_text(machine, b"AGENTS DONE AFTER 2 STEPS", 60).decode(errors="replace")
        machine.serial.settimeout(15)
        assert "agents: step 2 dk do done" in tail, tail[-2000:]
        settled(machine, tail)
        assert machine.submit('(file-read "task")').strip().startswith('"now count the files instead"')
        # The reviewer beside the player: it joins the player's world, its
        # needs are the player's summary file, and what it decides to change
        # is a def on the player's tunable cells in the shared world.
        assert "DOOM JUDGE AGENT READY" in machine.submit(":load doom-agent-judge")
        assert "REVIEW AGENT READY" in machine.submit(":join review")
        assert "CELLS 19" in machine.submit(":cells")
        assert machine.submit("follow-steps").strip().startswith("8")
        machine.submit('(rv-apply "follow-longer")')
        assert machine.submit("follow-steps").strip().startswith("12")
        machine.submit('(rv-apply "tolerance-wider")')
        assert machine.submit("facing-tolerance").strip().startswith("20")
        waited = machine.submit(":agents 1")
        assert "agents: step 1 dj waits window" in waited and "rv waits file summary" in waited, waited[-2000:]
    finally:
        machine.close()
print("An Agel program drives the desktop from inside the OS: the desktop's state read "
      "through the look line, the judged command typed, the judge's yes ending the run [ok]")

"""A browser written in Agel, driven by a judge, as a process on the OS.

The hosted runtime runs `browse-agent.agel` from the data region: the
`agel/browse` module reads the site's pages with `file-read`, prints each
page's accessibility tree on the process's console, asks the judge for the
next action through `model-request` — a block on the same console — and
acts on the answer line typed back. This harness is the judge: it reads
each request, checks the page it carries, and answers as the host bridge
would, until the agent says the task is done."""
import re
import socket
import sys
import tempfile
import time
import graphical_console as module


def until_text(machine, wanted, timeout):
    wanted = wanted.encode()
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
    return bytes(result).decode(errors="replace")


def request(machine, timeout=300):
    """The next request block the process prints: its number and text."""
    block = until_text(machine, "model-request end", timeout)
    number = int(re.search(r"model-request (\d+):", block).group(1))
    text = block.split(f"model-request {number}:\r\n", 1)[1].split("\r\nmodel-request end")[0]
    return number, text.replace("\r\n", "\n"), block


def answer(machine, number, line):
    """Typed to the reading process, as the operator would type it."""
    reply = machine.submit(f":model-reply {number} {line}", 120)
    assert "LINE GIVEN TO THE PROGRAM" in reply, reply
    return reply


with tempfile.TemporaryDirectory(prefix="agel-browse-", dir="/tmp") as directory:
    machine = module.Machine(sys.argv[1], directory)
    try:
        assert "formatted" in machine.submit(":fs-format")
        with machine.serial_lock:
            for byte in b":exec agel -- /data/browse.agel":
                machine.serial.sendall(bytes([byte]))
                machine.serial.recv(1)
            machine.serial.sendall(b"\n")
            # The first page: the tree is printed, then the judge is asked
            # with the task, the page and the fills.
            number, text, block = request(machine)
        machine.serial.settimeout(15)
        assert "page: Widget & Co (/data/index.html) form: /data/search.html" in block, block
        assert "3 link blue widgets -> /data/blue.html" in block, block
        assert '(judge (state ("task"' in text, text
        assert '"fills_with" "blue"' in text, text
        assert '"link-3" "blue widgets -> /data/blue.html"' in text, text
        assert '"fill-9" "fill the field q"' in text, text
        assert '"submit" ' in text and '"done" ' in text and '"back"' not in text, text
        # Fill the field with the task's quoted phrase and submit.
        answer(machine, number, "act choice 5 fill-9 800 30 30 800 70 70 done noul 50")
        number, text, block = request(machine)
        assert "browse: step 1 do fill-9" in block, block
        assert '9 field q = "blue"' in block, block
        assert "last: fill 9 q" in block, block
        answer(machine, number, "act choice 5 submit 900 20 20 40 900 20 done noul 100")
        number, text, block = request(machine)
        assert "browse: step 2 do submit" in block, block
        assert "page: Search results (/data/search.html)" in block, block
        assert "last: submit /data/search.html?q=blue" in block, block
        assert '"back" "go back to the previous page"' in text, text
        # The page shows the price: the judge says the task is done.
        answer(machine, number, "act choice 4 done 700 100 100 100 700 done noul 900")
        tail = until_text(machine, "process agel exited with status 0", 120)
        assert "browse: step 3 do done reason act choice 4 done 700 100 100 100 700 done noul 900" in tail, tail
        assert "browse: done after 3 steps" in tail, tail
        assert "process agel exited with status 0" in tail, tail
    finally:
        machine.close()
print("A browser written in Agel, driven by a judge on the OS: the site's pages read from the "
      "data region, the tree printed, a field filled, the form submitted, the task judged done [ok]")

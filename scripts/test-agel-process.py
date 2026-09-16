"""The language in a protection domain: the hosted Agel runtime, built
without `std`, runs as a process the supervisor loaded, installs the
standard library from its own image and evaluates a file from the
filesystem — recursion past the fixed evaluator's depth, the sequence
module, a macro, and an interpreted agent scheduled by the runtime — with
no authority beyond the namespace and console it was given. A failing
form is a transaction that rolls back and an exit status of 1."""
import socket
import sys
import tempfile
import time
import graphical_console as module


def check(machine, form, wanted, seconds=30):
    reply = machine.submit(form, seconds)
    assert wanted in reply, (form, reply)
    return reply


def until_text(machine, needle, seconds):
    """Serial output up to and including `needle`: what a process that
    outlives its `:exec` prints, and the desktop's report of its end."""
    buffer = bytearray()
    deadline = time.monotonic() + seconds
    while needle not in buffer:
        if time.monotonic() > deadline:
            raise TimeoutError(f"no {needle!r} within {seconds} s: {bytes(buffer)!r}")
        try:
            byte = machine.serial.recv(1)
        except socket.timeout:
            continue
        if not byte:
            raise RuntimeError("Agel stopped")
        buffer.extend(byte)
    return buffer.decode("utf-8", errors="replace")


def run(machine, line, seconds):
    """`:exec` and everything the process prints until the desktop reports
    its end: the desktop returns its prompt while a long process runs, and
    reports the end on a later pass, so the rest is read until the console
    is quiet."""
    reply = machine.submit(line, seconds)
    if "PROCESS RUNNING" in reply:
        reply += until_text(machine, b"exited with status", seconds)
        machine.serial.settimeout(2)
        try:
            while True:
                byte = machine.serial.recv(1)
                if not byte:
                    raise RuntimeError("Agel stopped")
                reply += byte.decode("utf-8", errors="replace")
        except socket.timeout:
            pass
        finally:
            machine.serial.settimeout(15)
    return reply


PROGRAM = [
    "(import agel/sequence)\\n(def fib (fn (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))))\\n(fib 15)\\n",
    "(foldl + 0 (map (fn (x) (* x x)) (list 1 2 3 4 5 6 7 8 9 10)))\\n",
    "(defmacro unless (c b) (if c nil b))\\n(unless #f 'expanded)\\n",
    "(import agel/meta)\\n(import agel/meta-agent)\\n(def counter (make-meta-agent \\\"counter\\\" '(fn (self state message) (+ state message)) 0 (meta-base-env)))\\n",
    "(send counter 20)\\n(send counter 22)\\n(run 2)\\n(meta-agent-state counter)\\n",
]

with tempfile.TemporaryDirectory(prefix="agel-language-", dir="/tmp") as directory:
    disk = module.prepared_image(sys.argv[1], directory, blank_sectors=1024)
    machine = module.Machine(disk, directory)
    try:
        check(machine, ":fs-format", "formatted")
        # The desktop's own evaluator writes the program, one piece per
        # form, since a console line holds 256 bytes.
        check(machine, f'(file-write "prog.agel" "{PROGRAM[0]}")', "\r\n")
        for piece in PROGRAM[1:]:
            check(machine, f'(file-append "prog.agel" "{piece}")', "\r\n")
        # The core alone: a file the runtime evaluates without the library.
        check(machine, '(file-write "core.agel" "(def x (+ 20 22))\\nx\\n")', "\r\n")
        started = time.monotonic()
        reply = check(machine, ":exec agel -- --no-stdlib core.agel", "=> 42", 120)
        assert "process agel exited with status 0" in reply, reply
        print(f"core file: {time.monotonic() - started:.1f} s")
        # The standard library installed in the domain, then the program.
        started = time.monotonic()
        reply = check(machine, ":exec agel -- prog.agel", "agel: standard library installed", 600)
        print(f"library and program: {time.monotonic() - started:.1f} s")
        for wanted in ("=> 610", "=> 385", "=> expanded", "=> 42", "process agel exited with status 0"):
            assert wanted in reply, (wanted, reply)
        # An evaluation longer than a process's tick budget: the runtime
        # yields every so many steps, so it is not stopped.
        check(machine, '(file-write "long.agel" "(def fib (fn (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))))\\n(fib 25)\\n")', "\r\n")
        started = time.monotonic()
        reply = run(machine, ":exec agel -- --no-stdlib long.agel", 600)
        assert "=> 75025" in reply, reply
        assert "process agel exited with status 0" in reply, reply
        print(f"long evaluation: {time.monotonic() - started:.1f} s")
        # A failing form: the transaction rolls back, the error is reported,
        # the status is 1.
        check(machine, '(file-write "bad.agel" "(def ok 1)\\n(/ 1 0)\\n")', "\r\n")
        reply = check(machine, ":exec agel -- --no-stdlib bad.agel", "agel: error:", 120)
        assert "division by zero" in reply, reply
        assert "process agel exited with status 1" in reply, reply
        # A file the namespace does not hold.
        reply = check(machine, ":exec agel -- --no-stdlib missing.agel", "agel: missing.agel: error 2", 120)
        assert "process agel exited with status 2" in reply, reply
    finally:
        machine.close()
print("The language in a domain: the hosted runtime and standard library run as a loaded process, evaluating files from the filesystem [ok]")

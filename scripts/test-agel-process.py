"""The language in a protection domain: the hosted Agel runtime, built
without `std`, runs as a process the supervisor loaded, installs the
standard library from its own image and evaluates a file from the
filesystem — recursion past the fixed evaluator's depth, the sequence
module, a macro, and an interpreted agent scheduled by the runtime — with
no authority beyond the namespace and console it was given. A failing
form is a transaction that rolls back and an exit status of 1."""
import socket
import sys
from pathlib import Path
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


def quiet(machine, seconds=2):
    """Everything the console prints until it has been silent for `seconds`:
    what a process prints between prompts, and the desktop's report of its
    end, which comes on a later pass."""
    text = ""
    machine.serial.settimeout(seconds)
    try:
        while True:
            byte = machine.serial.recv(1)
            if not byte:
                raise RuntimeError("Agel stopped")
            text += byte.decode("utf-8", errors="replace")
    except socket.timeout:
        pass
    finally:
        machine.serial.settimeout(15)
    return text


def run(machine, line, seconds):
    """`:exec` and everything the process prints until the desktop reports
    its end: the desktop returns its prompt while a long process runs."""
    reply = machine.submit(line, seconds)
    if "PROCESS RUNNING" in reply:
        reply += until_text(machine, b"exited with status", seconds) + quiet(machine)
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
        # Effect words: the namespace, the console, the clock and the program
        # table through the process protocol, each behind a capability. The
        # evaluation holds every kind; an agent holds what it was spawned
        # with, so a bare agent's write fails its turn and a keeper's lands.
        check(machine, '(file-write "effects.agel" "(file-write \\"out.txt\\" \\"hello from the runtime\\")\\n(file-append \\"out.txt\\" \\" +1\\")\\n")', "\r\n")
        check(machine, '(file-append "effects.agel" "(file-read \\"out.txt\\")\\n(file-list \\"/\\")\\n(console-log \\"logged from the runtime\\")\\n(type-of (clock))\\n(exec \\"hello\\")\\n")', "\r\n")
        check(machine, '(file-append "effects.agel" "(def scribe (fn (self heap message) (file-write \\"agent.txt\\" message)))\\n(def bare (spawn \\"bare\\" scribe nil nil))\\n(send bare \\"without\\")\\n(run 1)\\n")', "\r\n")
        check(machine, '(file-append "effects.agel" "(get (agent-info bare) (quote status))\\n(def cap (request-capability (quote file/write) \\"*\\"))\\n")', "\r\n")
        check(machine, '(file-append "effects.agel" "(def keeper (spawn \\"keeper\\" scribe nil nil nil (quote stop) 0 (list cap)))\\n(send keeper \\"kept by a capability\\")\\n(run 1)\\n(file-read \\"agent.txt\\")\\n")', "\r\n")
        reply = run(machine, ":exec agel -- --no-stdlib effects.agel", 120)
        for wanted in (
            "=> 22", "=> 3", '=> "hello from the runtime +1"', '"out.txt"', "logged from the runtime",
            "=> int", "hello from a loaded process", "=> 42", "=> stopped", '=> "kept by a capability"',
            "process agel exited with status 0",
        ):
            assert wanted in reply, (wanted, reply)
        # The files are the filesystem's: the desktop's evaluator reads them.
        check(machine, '(file-read "out.txt")', "hello from the runtime +1")
        check(machine, '(file-read "agent.txt")', "kept by a capability")
        # The compiler in the guest: the reader, linker and compiler written
        # in Agel run in the process, on a module from the data region; the
        # linked definition is written as source the desktop loads, and the
        # adapted one is byte-equal to what the host toolchain produces.
        check(machine, '(file-write "compile.agel" "(import agel/native-reader)\\n(import agel/native-modules)\\n(import agel/native)\\n(def forms (native-read (file-read \\"/data/dock.agel\\") 65536 64))\\n")', "\r\n")
        check(machine, '(file-append "compile.agel" "(def linked (native-link forms (quote dock) (quote behavior)))\\n(def ir (native-compile linked))\\n(type-of ir)\\n")', "\r\n")
        check(machine, '(file-append "compile.agel" "(def adapt (fn (b) (list (quote def) (quote behavior) (list (quote fn) (quote (s h m)) (list (quote (fn (n) (begin (paint s n) n))) (list b (quote s) (quote h) (quote m)))))))\\n")', "\r\n")
        check(machine, '(file-append "compile.agel" "(file-write \\"behavior.agel\\" (print-form (adapt linked)))\\n(file-write \\"linked.agel\\" (print-form (list (quote def) (quote behavior) linked)))\\n")', "\r\n")
        started = time.monotonic()
        reply = run(machine, ":exec agel -- compile.agel", 600)
        assert "=> list" in reply, reply
        assert "process agel exited with status 0" in reply, reply
        print(f"read, link, compile: {time.monotonic() - started:.1f} s")
        check(machine, ":load-file /linked.agel", "LOADED 1 FORMS FROM /linked.agel")
        check(machine, "(behavior nil 40 1)", "\r\n42\r\n")
        expected = module.compile_module(Path("examples/jit-module-dock.agel").read_text())
        reply = machine.submit('(file-read "behavior.agel")')
        assert expected in reply, (expected, reply)
        # A session: without a file the runtime reads the console line by
        # line, each line a transaction in the same world; the desktop hands
        # the prompt back while it reads and gives it the lines typed, and
        # `:eof` ends its input.
        def converse(line, wanted, seconds=30):
            reply = check(machine, line, "LINE GIVEN TO THE PROGRAM")
            return reply + until_text(machine, wanted.encode(), seconds) + quiet(machine, 1)
        reply = check(machine, ":exec agel -- --no-stdlib", "agel: session")
        assert "PROCESS READING" in reply, reply
        converse("(def x 40)", "=> 40")
        converse("(+ x 2)", "=> 42")
        reply = converse("(begin (def x 0) (/ 1 0))", "agel: error:")
        assert "division by zero" in reply, reply
        converse("x", "=> 40")
        reply = check(machine, ":eof", "END OF INPUT")
        reply += until_text(machine, b"PROCESS ENDED", 30) + quiet(machine)
        assert "agel: end of input at revision 3" in reply, reply
        assert "process agel exited with status 0" in reply, reply
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

"""Programs from files: the OS loads Agel from its own filesystem.

An Agel form writes a program to a file with `file-write`; `:load-file`
evaluates it form by form; `:load NAME` reads `/NAME.agel` for a name the
desktop does not carry; a failing form stops a load and the forms before
it stay; and a program left at `/init.agel` runs at the next boot before
the first prompt. Nothing here is in the kernel image: the code the
desktop runs came from its own disk."""
import sys
import tempfile
import time
import graphical_console as module


def check(machine, form, wanted):
    reply = machine.submit(form)
    assert wanted in reply, (form, reply)
    return reply


with tempfile.TemporaryDirectory(prefix="agel-programs-", dir="/tmp") as directory:
    disk = module.prepared_image(sys.argv[1], directory, blank_sectors=1024)
    machine = module.Machine(disk, directory)
    try:
        check(machine, ":fs-format", "formatted")
        # An Agel form writes a program of two forms, one per line.
        check(
            machine,
            '(file-write "tool.agel" "(def double (fn (x) (* x 2)))\\n(def greet (fn () (console-log \\"tool loaded\\")))\\n")',
            "\r\n",
        )
        check(machine, ":load-file /tool.agel", "LOADED 2 FORMS FROM /tool.agel")
        check(machine, "(double 21)", "\r\n42\r\n")
        check(machine, "(greet)", "tool loaded")
        # A name the desktop does not carry is read as /NAME.agel.
        check(machine, '(file-write "more.agel" "(def triple (fn (x) (* x 3)))")', "\r\n")
        check(machine, ":load more", "LOADED 1 FORMS FROM /more.agel")
        check(machine, "(triple 14)", "\r\n42\r\n")
        check(machine, ":load nothing", "NO SUCH FILE")
        # A failing form stops the load; the forms before it stay, the ones
        # after never run.
        check(machine, '(file-write "bad.agel" "(def ok 1)\\n(/ 1 0)\\n(def never 2)")', "\r\n")
        check(machine, ":load-file /bad.agel", "LOADED 1 FORMS THEN error: division by zero")
        check(machine, "ok", "\r\n1\r\n")
        check(machine, "never", "UNBOUND WORD - A SENTENCE SUMMONS THE AGENT")
        # A program left at /init.agel runs at the next boot, before the
        # first prompt, and can use every effect word.
        check(machine, '(file-write "init.agel" "(def booted 42)\\n(console-log \\"init ran\\")")', "\r\n")
        check(machine, ":fs-ls /", "init.agel")
    finally:
        machine.close()
    # The same disk, booted again: a fresh directory for the sockets, since
    # QEMU binds new ones and a client must not find the first boot's.
    with tempfile.TemporaryDirectory(prefix="agel-programs-boot2-", dir="/tmp") as second:
        # The harness reads the boot up to the first prompt and keeps it.
        machine = module.Machine(disk, second)
        try:
            text = machine.boot
            assert "init.agel: LOADED 2 FORMS FROM /init.agel" in text, text[-800:]
            assert "init ran" in text, text[-800:]
            check(machine, "booted", "\r\n42\r\n")
        finally:
            machine.close()
print("Programs from files: written by Agel, loaded by name or path, stopped at the first failing form, run at boot from /init.agel [ok]")

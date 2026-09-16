"""Programs from files: the desktop installs a program into the program
region from hex text in a file the OS can read, and runs it. The hello
program's ELF, written as hex into the data region by the host, becomes
`hello2` by `:install`, runs by `:exec`, is replaced by a second
`:install`, and a bad name or a missing file is refused."""
import sys
import tempfile
import graphical_console as module


def check(machine, form, wanted, seconds=30):
    reply = machine.submit(form, seconds)
    assert wanted in reply, (form, reply)
    return reply


with tempfile.TemporaryDirectory(prefix="agel-install-", dir="/tmp") as directory:
    disk = module.prepared_image(sys.argv[1], directory, blank_sectors=1024)
    machine = module.Machine(disk, directory)
    try:
        check(machine, ":fs-format", "formatted")
        reply = check(machine, ":install hello2 /data/hello.hex", "INSTALLED hello2: ")
        assert "BYTES AT SECTOR" in reply, reply
        reply = check(machine, ":exec hello2", "hello from a loaded process")
        assert "process hello2 exited with status 42" in reply, reply
        # Replaced in place in the table; the old sectors are a hole.
        check(machine, ":install hello2 /data/hello.hex", "INSTALLED hello2: ")
        check(machine, ":exec hello2", "process hello2 exited with status 42")
        check(machine, ":install hello3 /data/missing.hex", "NO SUCH FILE")
        check(machine, ":install a-name-far-too-long /data/hello.hex", "a program name is 1 to 16 printable ASCII bytes")
        # A file that is not hex text is refused before the table changes.
        check(machine, '(file-write "junk.hex" "zz")', "\r\n")
        check(machine, ":install junk /junk.hex", "the file is not hex text")
        check(machine, ":exec junk", "NOT STARTED")
    finally:
        machine.close()
print("Install: the desktop installs a program from hex text in a file into the program region and runs it [ok]")

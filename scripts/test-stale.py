"""A descriptor held across a filesystem restart fails closed: the `stale`
program opens `notes`, reads it, sleeps while the operator restarts the
service with `:fs-restart`, and its next read through the same descriptor
answers `ESTALE` (116); the file itself survives, read again by a fresh
descriptor from the desktop's evaluator."""
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
    """Everything until the console has been silent for `seconds`."""
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


with tempfile.TemporaryDirectory(prefix="agel-stale-", dir="/tmp") as directory:
    disk = module.prepared_image(sys.argv[1], directory, blank_sectors=1024)
    machine = module.Machine(disk, directory)
    try:
        check(machine, ":fs-format", "formatted")
        check(machine, '(file-write "notes" "kept across the restart")', "\r\n")
        # The process sleeps holding its descriptor; the desktop hands the
        # prompt back while it does.
        reply = check(machine, ":exec stale", "stale: read 23 bytes before the restart")
        assert "PROCESS SLEEPING" in reply, reply
        reply = check(machine, ":fs-restart", "filesystem restarted: generation 2")
        # The sleep ends, the read answers ESTALE, the process exits 116.
        reply = until_text(machine, b"exited with status", 30) + quiet(machine)
        assert "stale: read after the restart: error 116 (ESTALE)" in reply, reply
        assert "process stale exited with status 116" in reply, reply
        # The file is on the disk, not in the service: a fresh descriptor
        # reads it.
        check(machine, '(file-read "notes")', "kept across the restart")
    finally:
        machine.close()
print("ESTALE: a descriptor held across a filesystem restart fails closed with 116, and the file survives [ok]")

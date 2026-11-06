"""Run the same evolution contract as a protection-domain Agel process."""
import json
from pathlib import Path
import sys
import tempfile

import graphical_console as console


with tempfile.TemporaryDirectory(prefix="agel-evolution-", dir="/tmp") as directory:
    disk = console.prepared_image(sys.argv[1], directory, blank_sectors=1024)
    machine = console.Machine(disk, directory)
    try:
        assert "formatted" in machine.submit(":fs-format")
        fixture = Path("crates/agel-stdlib/tests/evolution.agel").read_text()
        source = "(begin\n" + fixture + "\n)"
        for offset in range(0, len(source), 80):
            word = "file-write" if offset == 0 else "file-append"
            command = f'({word} "evolve.agel" {json.dumps(source[offset:offset + 80])})'
            reply = machine.submit(command)
            assert "error" not in reply.lower(), (command, reply)
        reply = machine.submit(":exec agel -- evolve.agel", 600)
        assert "=> #t" in reply, reply
        assert "process agel exited with status 0" in reply, reply
        print("Evolution: hosted and native protection-domain contracts passed")
    finally:
        machine.close()

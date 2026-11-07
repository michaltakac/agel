"""An Agel judge wrapper writes protected OS bytes that survive reboot."""
import json
from pathlib import Path
import sys
import tempfile

import graphical_console as console


with tempfile.TemporaryDirectory(prefix="agel-judge-log-", dir="/tmp") as directory:
    disk = console.prepared_image(sys.argv[1], directory, blank_sectors=1024)
    machine = console.Machine(disk, directory, snapshot=False)
    try:
        assert "formatted" in machine.submit(":fs-format")
        source = "(begin\n" + Path("examples/logged-judge.agel").read_text() + "\n)"
        for offset in range(0, len(source), 80):
            word = "file-write" if offset == 0 else "file-append"
            command = f'({word} "audit.agel" {json.dumps(source[offset:offset + 80])})'
            reply = machine.submit(command)
            assert "error" not in reply.lower(), (command, reply)
        reply = machine.submit(":exec agel -- audit.agel", 600)
        assert "=> #t" in reply and "process agel exited with status 0" in reply, reply
        before = machine.submit('(file-read "judge-log")')
        assert "judge-start" in before and "judge-result" in before, before
        assert "example-1" in before and "cost-microusd 7" in before, before
        assert "example-2" in before and "provider unavailable" in before, before
        assert "cost-microusd nil" in before, before
        reply = machine.submit(":exec journal-check", 120)
        assert "journal-check: append and denial checks passed" in reply, reply
        assert "process journal-check exited with status 0" in reply, reply
        reply = machine.submit('(file-write "judge-log" "erased")')
        assert "refused the effect" in reply, reply
        reply = machine.submit(":fs-format")
        assert "formatted" not in reply.lower(), reply
        assert "example-1" in machine.submit('(file-read "judge-log")')
    finally:
        machine.close()
    machine = console.Machine(disk, directory, snapshot=False)
    try:
        after = machine.submit('(file-read "judge-log")')
        assert "example-1" in after and "example-2" in after and "probe" in after, after
        reply = machine.submit('(file-write "./judge-log" "erased")')
        assert "refused the effect" in reply, reply
        assert "example-1" in machine.submit('(file-read "judge-log")')
        reply = machine.submit('(file-append "judge-log" "after-reboot")')
        assert "error" not in reply.lower(), reply
        after = machine.submit('(file-read "judge-log")')
        assert "example-1" in after and "after-reboot" in after, after
        print("Judge log: receipts, failure costs, protected append and reboot passed")
    finally:
        machine.close()

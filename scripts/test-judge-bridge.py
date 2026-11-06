"""Judge transport must persist Agel accounting records on the guest disk."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import graphical_console as console


def run(command, **kwargs):
    return subprocess.run(command, text=True, capture_output=True, timeout=180, **kwargs)


def read_log(machine):
    parts = []
    while True:
        offset = sum(map(len, parts))
        reply = machine.submit(f'(let ((p (file-read "judge-log" {offset}))) (text-slice p 0 (if (< 200 (text-bytes p)) 200 (text-bytes p))))')
        value = next((line for line in reply.splitlines() if line.startswith('"')), None)
        assert value is not None, reply
        part = json.loads(value)
        if not part:
            return "".join(parts)
        parts.append(part)


def inspect(disk, directory, action=read_log):
    machine = console.Machine(disk, directory)
    try:
        return action(machine)
    finally:
        machine.close()


with tempfile.TemporaryDirectory(prefix="agel-judge-bridge-", dir="/tmp") as temporary:
    root = Path(temporary)
    curl = root / "curl"
    marker = root / "calls"
    curl.write_text("#!/usr/bin/env python3\n"
                    "import json, sys\nfrom pathlib import Path\n"
                    "sys.stdin.buffer.read()\n"
                    f"with Path({str(marker)!r}).open('a') as output: output.write('x')\n"
                    "request = json.loads(Path(sys.argv[sys.argv.index('--data-binary') + 1][1:]).read_text())\n"
                    "answers = {}\n"
                    "for name, question in request['questions'].items():\n"
                    "    if question['type'] == 'noul': answers[name] = {'type': 'noul', 'noul': 1}\n"
                    "    else:\n"
                    "        options = question['criteria']\n"
                    "        choice = 'done' if 'done' in options else next(iter(options))\n"
                    "        answers[name] = {'type': 'choice', 'choice': choice, 'confidence': 1,\n"
                    "                         'probabilities': {key: int(key == choice) for key in options}}\n"
                    "print(json.dumps({'model': 'stand-in', 'answers': answers,\n"
                    "                  'usage': {'input_tokens': 509, 'output_tokens': 89}}))\n"
                    "print('200', end='')\n")
    curl.chmod(0o700)
    env = dict(os.environ, TYPESAFEAI_API_KEY="audit-test-key")
    out = root / "run"
    command = ["target/release/agel-play", "--scene", "desktop", "--policy", "jev",
               "--image", sys.argv[1], "--out", str(out), "--curl-bin", str(curl),
               "--steps", "1", "--hold", "1"]
    result = run(command, env=env)
    assert result.returncode == 0, (result.stdout, result.stderr)
    disk = out / "disk.img"
    first = inspect(disk, root)
    assert first.count("(judge-start ") == 1 and first.count("(judge-result ") == 1, first
    assert "cost-nanousd 21378" in first and "cost-microusd 22" in first, first
    assert "time-unix-nanos" in first and "audit-test-key" not in first, first
    before = hashlib.sha256(disk.read_bytes()).hexdigest()
    refused = run(command, env=env)
    assert refused.returncode != 0 and "run disk already exists" in refused.stderr, refused
    assert hashlib.sha256(disk.read_bytes()).hexdigest() == before
    assert marker.read_text() == "x"
    result = run(command + ["--resume"], env=env)
    assert result.returncode == 0, (result.stdout, result.stderr)
    second = inspect(disk, root)
    assert second.startswith(first) and second.count("(judge-start ") == 2, second
    assert second.count("(judge-result ") == 2 and marker.read_text() == "xx"

    def fill(machine):
        reply = machine.submit('(def payload "' + "z" * 180 + '")')
        assert "error" not in reply.lower(), reply
        reply = machine.submit('(begin (def payload (text-concat payload (text-concat payload (text-concat payload (text-concat payload payload))))) #t)')
        assert "#t" in reply, reply
        reply = machine.submit('(text-bytes payload)')
        assert "900" in reply, reply
        for _ in range(80):
            reply = machine.submit('(file-append "judge-log" payload)')
            if "refused the effect" in reply or "the effect failed" in reply:
                break
            assert "900" in reply, reply
        else:
            raise AssertionError("guest log did not fill")
        return read_log(machine)

    filled = inspect(disk, root, fill)
    assert filled.startswith(second)
    assert len(filled.encode()) > 65536 - 900, len(filled)
    result = run(command + ["--resume"], env=env)
    assert result.returncode != 0 and "judge log" in result.stderr, (result.stdout, result.stderr)
    assert marker.read_text() == "xx", "full guest storage must prevent the network call"
    retained = inspect(disk, root)
    assert retained.startswith(filled)

    # The browser uses the separate POSIX Agel model-request transport.
    browse = root / "browse"
    result = run(["target/release/agel-play", "--scene", "browse", "--policy", "jev",
                  "--image", sys.argv[1], "--agel", sys.argv[2], "--out", str(browse),
                  "--curl-bin", str(curl), "--task", "finish", "--steps", "1"], env=env)
    assert result.returncode == 0, (result.stdout, result.stderr)
    logged = inspect(browse / "disk.img", root)
    assert "(judge-start " in logged and "(judge-result " in logged, logged
    assert "cost-nanousd 21378" in logged, logged
    print("Judge bridge: guest receipts, cost, resume, full-disk dispatch denial and POSIX transport passed")

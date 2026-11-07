"""Guest append-store recovery, capacity reservation and Agel frame decoding."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

import graphical_console as console

FIRST = 131072
DATA = FIRST + 2
CAPACITY = 131070


def checksum(block):
    value = 0xCBF29CE484222325
    for byte in block:
        value = ((value ^ byte) * 0x100000001B3) & ((1 << 64) - 1)
    return value


def encode(generation, sectors):
    block = bytearray(512)
    block[:8] = b"AGELJNL1"
    block[8:16] = generation.to_bytes(8, "little")
    block[16:20] = sectors.to_bytes(4, "little")
    block[24:32] = checksum(block).to_bytes(8, "little")
    return block


def head(disk):
    found = []
    with disk.open("rb") as source:
        source.seek(FIRST * 512)
        for _ in range(2):
            block = bytearray(source.read(512))
            digest = int.from_bytes(block[24:32], "little")
            block[24:32] = bytes(8)
            if block[:8] == b"AGELJNL1" and checksum(block) == digest:
                found.append((int.from_bytes(block[8:16], "little"),
                              int.from_bytes(block[16:20], "little")))
    return max(found)


def raw(disk, limit=None):
    size = head(disk)[1] * 512 if limit is None else limit
    with disk.open("rb") as source:
        source.seek(DATA * 512)
        return source.read(size)


def decode(data, directory):
    source = directory / "frames"
    source.write_bytes(data)
    result = subprocess.run(["target/debug/examples/journal", str(source)],
                            capture_output=True, text=True, timeout=60)
    assert result.returncode == 0, result.stderr
    return result.stdout, result.stderr


with tempfile.TemporaryDirectory(prefix="agel-ledger-", dir="/tmp") as temporary:
    root = Path(temporary)
    curl = root / "curl"
    marker = root / "calls"
    curl.write_text("#!/usr/bin/env python3\n"
                    "import json, sys\nfrom pathlib import Path\n"
                    "sys.stdin.buffer.read()\n"
                    f"with Path({str(marker)!r}).open('a') as output: output.write('x')\n"
                    "request=json.loads(Path(sys.argv[sys.argv.index('--data-binary')+1][1:]).read_text())\n"
                    "answers={}\n"
                    "for name,q in request['questions'].items():\n"
                    " if q['type']=='noul': answers[name]={'type':'noul','noul':1}\n"
                    " else:\n"
                    "  opts=q['criteria']; choice='done' if 'done' in opts else next(iter(opts))\n"
                    "  answers[name]={'type':'choice','choice':choice,'confidence':1,'probabilities':{k:int(k==choice) for k in opts}}\n"
                    "print(json.dumps({'model':'fixture','answers':answers,'usage':{'input_tokens':509,'output_tokens':89}}))\n"
                    "print('200',end='')\n")
    curl.chmod(0o700)
    env = dict(os.environ, TYPESAFEAI_API_KEY="ledger-test-key")

    def episode(out, resume=False, scene="desktop"):
        command = ["target/release/agel-play", "--scene", scene, "--policy", "jev",
                   "--image", sys.argv[1], "--agel", sys.argv[2], "--out", str(out),
                   "--curl-bin", str(curl), "--steps", "1", "--hold", "1", "--task", "finish"]
        if resume:
            command.append("--resume")
        return subprocess.run(command, env=env, capture_output=True, text=True, timeout=180)

    out = root / "run"
    result = episode(out)
    assert result.returncode == 0, (result.stdout, result.stderr)
    disk = out / "disk.img"
    first = raw(disk)
    records, health = decode(first, root)
    assert records.count("(judge-start ") == 1 and records.count("(judge-result ") == 1, records
    assert "cost-nanousd 21378" in records and "(0 #f)" in health, (records, health)
    assert "ledger-test-key" not in records
    print("ledger: native judge request/result decoded in Agel", flush=True)

    for _ in range(8):
        result = episode(out, resume=True)
        assert result.returncode == 0, (result.stdout, result.stderr)
    base = raw(disk)
    records, health = decode(base, root)
    assert len(base) > 65536 and base.startswith(first), len(base)
    assert records.count("(judge-result ") == 9 and "(0 #f)" in health, (records, health)
    print("ledger: resumed history exceeds the old 64 KiB limit", flush=True)

    machine = console.Machine(disk, root)
    try:
        reply = machine.submit('(let ((p (file-read "judge-ledger"))) (text-slice p 0 512))')
        value = json.loads(next(line for line in reply.splitlines() if line.startswith('"')))
        assert value.encode() == base[:512], reply
        assert "refused the effect" in machine.submit('(file-write "judge-ledger" "erase")')
    finally:
        machine.close()

    generation, sectors = head(disk)
    baseline = root / "baseline.img"
    shutil.copyfile(disk, baseline)
    result = episode(out, resume=True)
    assert result.returncode == 0, (result.stdout, result.stderr)
    candidate = raw(disk)[len(base):len(base) + 512]
    assert len(candidate) == 512

    for cut in [0, 1, 31, 256, 511, 512]:
        case = root / f"cut-{cut}"
        case.mkdir()
        target = case / "disk.img"
        shutil.copyfile(baseline, target)
        with target.open("r+b") as stream:
            stream.seek((DATA + sectors) * 512)
            stream.write(candidate[:cut])
        result = episode(case, resume=True)
        assert result.returncode == 0, (cut, result.stdout, result.stderr)
        after = raw(target)
        assert after.startswith(base), cut
        if cut:
            assert after[len(base):len(base) + cut] == candidate[:cut], cut
        recovered, _ = decode(after, root)
        assert recovered.count("(judge-result ") == 10, (cut, recovered)
    print("ledger: torn append sectors remain immutable; later records recover", flush=True)

    for cut in [1, 256, 511]:
        case = root / f"head-{cut}"
        case.mkdir()
        target = case / "disk.img"
        shutil.copyfile(baseline, target)
        with target.open("r+b") as stream:
            stream.seek((DATA + sectors) * 512)
            stream.write(candidate)
            stream.seek((FIRST + (generation + 1) % 2) * 512)
            stream.write(encode(generation + 1, sectors))
            stream.seek((FIRST + (generation + 2) % 2) * 512)
            stream.write(encode(generation + 2, sectors + 1)[:cut])
        result = episode(case, resume=True)
        assert result.returncode == 0, (cut, result.stdout, result.stderr)
        after = raw(target)
        assert after.startswith(base + candidate), cut
        recovered, _ = decode(after, root)
        assert recovered.count("(judge-result ") == 10, cut
    print("ledger: torn metadata recovers through the other head and tail scan", flush=True)

    full = root / "full"
    full.mkdir()
    target = full / "disk.img"
    shutil.copyfile(baseline, target)
    with target.open("r+b") as stream:
        for slot in range(2):
            stream.seek((FIRST + slot) * 512)
            stream.write(encode(generation + 10 + slot, CAPACITY - 1))
    calls = marker.read_bytes()
    result = episode(full, resume=True)
    assert result.returncode != 0 and "reservation" in result.stderr, (result.stdout, result.stderr)
    assert marker.read_bytes() == calls
    assert raw(target, len(base)) == base
    print("ledger: insufficient result reservation prevents the network call", flush=True)

    browse = root / "browse"
    result = episode(browse, scene="browse")
    assert result.returncode == 0, (result.stdout, result.stderr)
    records, health = decode(raw(browse / "disk.img"), root)
    assert "cost-nanousd 21378" in records and "(0 #f)" in health, (records, health)
    print("ledger: POSIX Agel judge transport passed", flush=True)

    fresh = root / "fresh-head"
    fresh.mkdir()
    new_disk = Path(console.prepared_image(sys.argv[1], fresh, blank_sectors=1024))
    machine = console.Machine(new_disk, fresh)
    try:
        assert "formatted" in machine.submit(":fs-format")
    finally:
        machine.close()
    with new_disk.open("r+b") as stream:
        stream.seek((FIRST + 1) * 512)
        stream.write(encode(1, 0)[:16])
    result = episode(fresh, resume=True)
    assert result.returncode == 0, (result.stdout, result.stderr)
    records, health = decode(raw(new_disk), root)
    assert records.count("(judge-result ") == 1 and "(0 #f)" in health, (records, health)
    print("ledger: interrupted first reservation recovers without a valid checkpoint", flush=True)

    if len(sys.argv) > 3:
        quota = root / "quota.img"
        shutil.copyfile(baseline, quota)
        installed = subprocess.run([sys.executable, "scripts/install-program.py", str(quota),
                                    "ledger-check", sys.argv[3]], capture_output=True, text=True)
        assert installed.returncode == 0, installed.stderr
        with quota.open("r+b") as stream:
            for slot in range(2):
                stream.seek((FIRST + slot) * 512)
                stream.write(encode(generation + 20 + slot, CAPACITY - 2))
        machine = console.Machine(quota, root)
        try:
            reply = machine.submit(":exec ledger-check / ro", 120)
            assert "process ledger-check exited with status 1" in reply, reply
            reply = machine.submit(":exec ledger-check", 120)
            assert "ledger-check: reserved space and stale-token checks passed" in reply, reply
            assert "process ledger-check exited with status 0" in reply, reply
        finally:
            machine.close()
        print("ledger: ordinary writers cannot consume reserved result space; stale tokens fail", flush=True)

        small = root / "small"
        small.mkdir()
        small_disk = small / "disk.img"
        shutil.copyfile(baseline, small_disk)
        installed = subprocess.run([sys.executable, "scripts/install-program.py", str(small_disk),
                                    "ledger-check", sys.argv[3]], capture_output=True, text=True)
        assert installed.returncode == 0, installed.stderr
        with small_disk.open("r+b") as stream:
            stream.truncate(100 * 1024 * 1024)
        machine = console.Machine(small_disk, root)
        try:
            reply = machine.submit(":exec ledger-check", 120)
            assert "reserve failed" in reply, reply
            assert "process ledger-check exited with status 1" in reply, reply
        finally:
            machine.close()
        result = episode(small, resume=True)
        assert result.returncode == 0, (result.stdout, result.stderr)
        assert small_disk.stat().st_size >= 128 * 1024 * 1024
        assert raw(small_disk).startswith(base)
        print("ledger: undersized disk refuses reservations; bridge growth preserves its history", flush=True)

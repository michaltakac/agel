"""Boot a fresh OS and carry its inference bytes to existing providers."""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
import time

import graphical_console as console


live = sys.argv[1:] == ['--live']
if sys.argv[1:] not in ([], ['--live']):
    raise SystemExit('usage: test-native-repair.py [--live]')
environment = dict(os.environ)
if live and Path('.env').is_file():
    for line in Path('.env').read_text().splitlines():
        if line.startswith('TYPESAFEAI_API_KEY='):
            values = shlex.split(line.partition('=')[2])
            if values:
                environment.setdefault('TYPESAFEAI_API_KEY', values[0])
if live and not environment.get('TYPESAFEAI_API_KEY'):
    raise SystemExit('TYPESAFEAI_API_KEY is required for the real native text judge')
directory = Path('target') / ('native-repair-' + ('live-' if live else 'fixture-') + str(time.time_ns()))
directory.mkdir(mode=0o700)
(directory / 'mode.txt').write_text('live\n' if live else 'fixture\n')
if live:
    environment.pop('AGEL_CODEX_BIN', None)
    environment.pop('AGEL_CURL_BIN', None)
if not live:
    environment['TYPESAFEAI_API_KEY'] = 'fixture-secret-never-a-model-name'
    writer = directory / 'codex-fixture'
    writer.write_text("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"(dict 'diagnosis \\\"division truncates partial batches\\\" 'after '(fn (n size) (/ (+ n (- size 1)) size)) 'predictions '(2 0))\"\n")
    judge = directory / 'curl-fixture'
    response = {'model': 'native-repair-fixture', 'answers': {
        'patch': {'type': 'noul', 'noul': .95}, 'reason': {'type': 'choice', 'choice': 'none', 'confidence': .9,
        'probabilities': {'none': .9, 'contract': .025, 'boundary': .025, 'syntax': .025, 'effects': .025}}},
        'usage': {'input_tokens': 100, 'output_tokens': 30}}
    judge.write_text("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n200' '" + json.dumps(response) + "'\n")
    for executable in (writer, judge):
        executable.chmod(0o700)
    environment.update(AGEL_CODEX_BIN=str(writer.resolve()), AGEL_CURL_BIN=str(judge.resolve()))
disk = console.prepared_image('target/boot/agel-v1.img', directory, blank_sectors=1024)
catalog = Path('crates/agel-stdlib/stdlib.agel').read_text()
source = ''
for name in ['sequence', 'meta', 'judgment']:
    start = catalog.index('(module agel/' + name + '\n')
    end = catalog.find('\n(module ', start + 1)
    source += catalog[start:end if end >= 0 else len(catalog)] + '\n'
for module, export, filename in [('native-reader', 'native-read', 'native-reader.agel'),
                                  ('native', 'native-compile', 'native-compiler.agel')]:
    source += '(module agel/' + module + ' (export ' + export + ') (def ' + export + ' ' + Path('crates/agel-stdlib/' + filename).read_text() + '))\n'
source += ''.join(Path('crates/agel-stdlib/' + name).read_text() + '\n' for name in
                 ['native-x86.agel', 'evolution.agel', 'judge-log.agel', 'bytes.agel', 'program-store.agel', 'native-program.agel', 'repair.agel', 'inference.agel'])
source += Path('examples/native-repair.agel').read_text()
script = directory / 'native-repair.agel'
script.write_text(source)
for command in [
    ['python3', 'scripts/install-program.py', disk, 'agel', 'boot/posix/target/x86_64-unknown-none/release/agel'],
    ['python3', 'scripts/install-program.py', '--region', 'data', disk, 'native-repair', str(script)],
]:
    subprocess.run(command, check=True, capture_output=True)
with tempfile.TemporaryDirectory(prefix='agel-native-repair-', dir='/tmp') as sockets:
    serial = Path(sockets) / 'serial'
    machine = subprocess.Popen(['qemu-system-x86_64', '-machine', 'pc,accel=tcg', '-m', '64M',
        '-display', 'none', '-no-reboot', '-vga', 'std', '-serial', f'unix:{serial},server=on,wait=on',
        '-drive', f'format=raw,file={Path(disk).resolve()},if=ide,index=0,media=disk'],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        deadline = time.monotonic() + 15
        while not serial.exists():
            if machine.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('QEMU serial socket unavailable')
            time.sleep(.05)
        started = time.monotonic()
        print('Evidence:', directory, flush=True)
        print('Mode:', 'real providers' if live else 'deterministic transport fixtures', flush=True)
        with (directory / 'serial.log').open('w') as log:
            result = subprocess.run(['target/release/examples/guest_inference', str(serial),
                str(directory.resolve()), 'process agel exited with status', ':fs-format',
                ':exec agel -- --no-stdlib /data/native-repair'], stdout=log, stderr=subprocess.STDOUT,
                env=environment, timeout=960)
        print('Elapsed seconds:', round(time.monotonic() - started, 3), flush=True)
        output = (directory / 'serial.log').read_text()
        print('\n'.join(line for line in output.splitlines()
                        if 'native-repair:' in line or 'exited with status' in line), flush=True)
        passed = result.returncode == 0 and 'native-repair: PASS:' in output and 'process agel exited with status 0' in output
    finally:
        machine.terminate()
        machine.wait(timeout=10)


def head_checksum(block):
    value = 0xcbf29ce484222325
    for byte in block:
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return value


heads = []
with Path(disk).open('rb') as source:
    source.seek(131072 * 512)
    for _ in range(2):
        block = bytearray(source.read(512))
        digest = int.from_bytes(block[24:32], 'little')
        block[24:32] = bytes(8)
        if block[:8] == b'AGELJNL1' and head_checksum(block) == digest:
            heads.append((int.from_bytes(block[8:16], 'little'), int.from_bytes(block[16:20], 'little')))
    if passed:
        assert heads, 'no durable judge ledger head'
    sectors = max(heads)[1] if heads else 0
    source.seek(131074 * 512)
    frames = source.read(sectors * 512)
framed = directory / 'judge.frames'
framed.write_bytes(frames)
decoded = subprocess.run(['target/debug/examples/journal', str(framed)], capture_output=True, text=True, check=True)
assert '(0 #f)' in decoded.stderr, decoded.stderr
records = decoded.stdout
if passed:
    assert records.count('(judge-start ') == records.count('(judge-result ') and '(judge-result ' in records
    assert 'cost-status published-rate' in records and 'input-tokens' in records and 'output-tokens' in records
assert environment['TYPESAFEAI_API_KEY'] not in records
(directory / 'judge.agel').write_text(records)
costs = [int(n) for n in re.findall(r'cost-nanousd (\d+)', records)]
print('Durable guest judge calls:', len(costs), 'real cost-nanousd:' if live else 'synthetic priced-record nanousd:', sum(costs), flush=True)
print('Codex dollar cost:', 'unknown' if live else 'fixture, no paid inference', flush=True)
print('Ledger SHA-256:', hashlib.sha256(frames).hexdigest(), flush=True)
if not passed:
    raise SystemExit('Native repair failed; retained disk, serial output and decoded judge records')
print('PASS: native repair pipeline; ' + ('real model-written code' if live else 'scripted provider fixtures') + '; judge records in protected guest ledger', flush=True)

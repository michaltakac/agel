"""Provider setup and protected-ledger evidence shared by native test runners."""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess


def provider_environment(directory, live, delay=0):
    environment = dict(os.environ)
    if live:
        if Path('.env').is_file():
            for line in Path('.env').read_text().splitlines():
                if line.startswith('TYPESAFEAI_API_KEY='):
                    values = shlex.split(line.partition('=')[2])
                    if values:
                        environment.setdefault('TYPESAFEAI_API_KEY', values[0])
        if not environment.get('TYPESAFEAI_API_KEY'):
            raise RuntimeError('TYPESAFEAI_API_KEY is required for the real text judge')
        environment.pop('AGEL_CODEX_BIN', None)
        environment.pop('AGEL_CURL_BIN', None)
    else:
        environment['TYPESAFEAI_API_KEY'] = 'fixture-secret-never-a-model-name'
        writer = directory / 'codex-fixture'
        proposal = "(dict 'diagnosis \"division truncates partial batches\" 'after '(fn (n size) (/ (+ n (- size 1)) size)) 'predictions '(2 0))"
        writer.write_text("#!/bin/sh\ncat >/dev/null\nsleep " + str(int(delay)) + "\nprintf '%s\\n' " + shlex.quote(proposal) + "\n")
        judge = directory / 'curl-fixture'
        response = {'model': 'native-session-fixture', 'answers': {
            'patch': {'type': 'noul', 'noul': .95}, 'reason': {'type': 'choice', 'choice': 'none', 'confidence': .9,
            'probabilities': {'none': .9, 'contract': .025, 'boundary': .025, 'syntax': .025, 'effects': .025}}},
            'usage': {'input_tokens': 100, 'output_tokens': 30}}
        judge.write_text("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n200' '" + json.dumps(response) + "'\n")
        for executable in (writer, judge):
            executable.chmod(0o700)
        environment.update(AGEL_CODEX_BIN=str(writer.resolve()), AGEL_CURL_BIN=str(judge.resolve()))
    (directory / 'mode.txt').write_text('live\n' if live else 'fixture\n')
    return environment


def decode_judge_ledger(disk, directory, secret, live, required=True):
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
        source.seek(131074 * 512)
        frames = source.read((max(heads)[1] if heads else 0) * 512)
    framed = directory / 'judge.frames'
    framed.write_bytes(frames)
    decoded = subprocess.run(['target/debug/examples/journal', str(framed)], capture_output=True, text=True, check=True)
    assert '(0 #f)' in decoded.stderr, decoded.stderr
    records = decoded.stdout
    assert secret not in records
    (directory / 'judge.agel').write_text(records)
    costs = [int(n) for n in re.findall(r'cost-nanousd (\d+)', records)]
    calls = records.count('(judge-start ')
    print('Durable guest judge calls:', calls, 'known real cost-nanousd:' if live else 'synthetic priced-record nanousd:', sum(costs), 'unknown receipts:', max(0, calls-len(costs)), flush=True)
    print('Codex dollar cost:', 'unknown' if live else 'fixture, no paid inference', flush=True)
    print('Ledger SHA-256:', hashlib.sha256(frames).hexdigest(), flush=True)
    if not required and not costs:
        return records
    assert heads and costs, 'no durable judge receipts'
    assert records.count('(judge-start ') == records.count('(judge-result '), 'unmatched judge receipt'
    assert 'cost-status published-rate' in records and 'input-tokens' in records and 'output-tokens' in records
    return records

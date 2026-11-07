"""Provider setup and protected-ledger evidence shared by native test runners."""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile


def provider_environment(directory, live, delay=0, proposal=None):
    environment = dict(os.environ)
    environment.pop('AGEL_CODEX_USAGE', None)
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
        environment['AGEL_CODEX_USAGE'] = '1'
    else:
        environment['TYPESAFEAI_API_KEY'] = 'fixture-secret-never-a-model-name'
        writer = directory / 'codex-fixture'
        if proposal is None:
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


def judge_frames(disk):
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
            sectors = int.from_bytes(block[16:20], 'little')
            if (len(block) == 512 and block[:8] == b'AGELJNL1'
                    and sectors <= 131070 and head_checksum(block) == digest):
                heads.append((int.from_bytes(block[8:16], 'little'), sectors))
        size = (max(reversed(heads), key=lambda head: head[0])[1] if heads else 0) * 512
        frames = source.read(size)
        if len(frames) != size:
            raise ValueError('truncated guest judge ledger')
        return frames


def decode_judge_frames(framed):
    decoded = subprocess.run(['target/debug/examples/journal', str(framed)], capture_output=True, text=True, check=True)
    if decoded.stderr.strip() != 'journal health: (0 #f)':
        raise ValueError('incomplete or corrupt guest judge journal')
    return decoded.stdout


def verified_judge_ledger(directory):
    frames = judge_frames(directory / 'disk.img')
    if frames != (directory / 'judge.frames').read_bytes():
        raise ValueError(f'{directory}: judge frames differ from the guest journal')
    with tempfile.TemporaryDirectory(prefix='agel-judge-') as scratch:
        path = Path(scratch) / 'judge.frames'
        path.write_bytes(frames)
        records = decode_judge_frames(path)
    if records != (directory / 'judge.agel').read_text():
        raise ValueError(f'{directory}: judge records differ from the guest journal')
    recovery = directory / 'judge-recovery.agel'
    if recovery.exists() and recovery.read_text() != records:
        raise ValueError(f'{directory}: host recovery records differ from the guest journal')
    return records, frames


def judge_receipts_for_trace(ledger, trace):
    starts = re.findall(r'^\(judge-start .*$', ledger, re.M)
    receipts = re.findall(r'^\(judge-result .*$', ledger, re.M)
    identity = re.compile(r'^\(judge-(?:start|result) id "([^"]+)".*? attempt (\d+)\b')
    started = [identity.match(line) for line in starts]
    completed = [identity.match(line) for line in receipts]
    if not all(started) or not all(completed):
        raise ValueError('missing judge call identity')
    keys = [match.groups() for match in started]
    if len(set(keys)) != len(keys) or sorted(keys) != sorted(match.groups() for match in completed):
        raise ValueError('unmatched or duplicate judge receipts')
    dispatched = re.findall(r'^inference-bridge: started job (\d+) provider jev ', trace, re.M)
    jobs = {name.rsplit('/', 1)[-1] for name, _ in keys}
    if len(set(dispatched)) != len(dispatched) or jobs != set(dispatched):
        raise ValueError('judge ledger differs from bridge dispatches')
    return receipts


def judge_cost(receipt):
    cost = re.search(r'\bcost-nanousd (\d+) cost-microusd \d+ cost-status published-rate\b',
                     receipt.partition(' price-basis ')[0])
    return int(cost[1]) if cost else None


def writer_usage_for_trace(trace):
    jobs = re.findall(r'^inference-bridge: started job (\d+) provider codex ', trace, re.M)
    receipts = {}
    for line in trace.splitlines():
        if not line.startswith('inference-bridge: writer usage '):
            continue
        receipt = re.fullmatch(
            r'inference-bridge: writer usage job (\d+) (?:unavailable|input (\d+) cached-input (\d+|unknown) output (\d+))',
            line)
        if not receipt or receipt[1] not in jobs or receipt[1] in receipts:
            raise ValueError('invalid or unmatched writer usage receipt')
        usage = None
        if receipt[2] is not None:
            inputs, outputs = int(receipt[2]), int(receipt[4])
            cached = None if receipt[3] == 'unknown' else int(receipt[3])
            if cached is not None and cached > inputs:
                raise ValueError('cached writer tokens exceed input tokens')
            usage = (inputs, cached, outputs)
        receipts[receipt[1]] = usage
    if len(set(jobs)) != len(jobs):
        raise ValueError('duplicate writer dispatch')
    return jobs, [receipts.get(job) for job in jobs]


def decode_judge_ledger(disk, directory, secret, live, required=True):
    frames = judge_frames(disk)
    framed = directory / 'judge.frames'
    framed.write_bytes(frames)
    records = decode_judge_frames(framed)
    assert secret not in records
    (directory / 'judge.agel').write_text(records)
    recovery = directory / 'judge-recovery.agel'
    if recovery.exists():
        recovered = recovery.read_text()
        assert secret not in recovered
        if recovered != records:
            print('Judge recovery file differs from the guest ledger:', recovery, flush=True)
            assert not required, 'guest ledger did not acknowledge all recovery records'
    receipts = re.findall(r'^\(judge-result .*$', records, re.M)
    costs = [cost for receipt in receipts if (cost := judge_cost(receipt)) is not None]
    calls = len(re.findall(r'^\(judge-start ', records, re.M))
    print('Durable guest judge calls:', calls, 'known real cost-nanousd:' if live else 'synthetic priced-record nanousd:', sum(costs), 'unknown receipts:', max(0, calls-len(costs)), flush=True)
    print('Codex dollar cost:', 'unknown' if live else 'fixture, no paid inference', flush=True)
    print('Ledger SHA-256:', hashlib.sha256(frames).hexdigest(), flush=True)
    if not required:
        return records
    assert frames and receipts, 'no durable judge receipts'
    assert calls == len(receipts), 'unmatched judge receipt'
    assert all('input-tokens' in receipt and 'output-tokens' in receipt for receipt in receipts)
    return records

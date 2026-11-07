#!/usr/bin/env python3
"""Verify Agel rat combat or bounded Attack training on a fresh normal account."""
import argparse
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time
sys.path.insert(0, "scripts/osrs")
from test_support import combat_xp, combat_xp_saved, evaluate, form


def notice(text):
    print(text, flush=True)
    path = Path('target/osrs-live-os/status.txt')
    if path.parent.exists():
        path.write_text(text)


parser = argparse.ArgumentParser(description=__doc__)
mode = parser.add_mutually_exclusive_group()
mode.add_argument('--live-tools', action='store_true')
mode.add_argument('--attack-xp', type=int, help='train against rats until this Attack XP')
options = parser.parse_args()
if options.attack_xp is not None and not 1 <= options.attack_xp <= 512:
    parser.error('--attack-xp must be between 1 and 512')

username = 'agel' + secrets.token_hex(4)
password = secrets.token_hex(8)
directory = Path('target/osrs-login-probes')
directory.mkdir(mode=0o700, exist_ok=True)
with os.fdopen(os.open(directory / (username + '.json'), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600), 'w') as output:
    json.dump({'username': username, 'password': password}, output)
key = subprocess.check_output(['docker', 'exec', 'agel-osrs-world', 'cat', '/world/.data/client.key'], text=True)
assert key.split('Exponent: ')[1].splitlines()[0].strip() == '10001'
modulus = key.split('Modulus: ')[1].strip()
source = '(import agel/osrs-wire) (import agel/osrs-login) (import agel/osrs-crypto) (import agel/osrs-game) '
source += '(osrs-js5-open 241 \'(1 2 3 4)) (def crcs (osrs-cache-crcs (osrs-js5-group 255 255))) (wire-close) '
source += '(def context (rsa-context ' + form(modulus) + ')) (def seed (osrs-random-seed)) (def game (osrs-game-state seed)) '
source += '(osrs-login ' + form(username) + ' ' + form(password) + ' context crcs seed (entropy-read 24)) '
source += Path('examples/osrs/combat-test.agel').read_text()
if options.live_tools:
    source += Path('crates/agel-stdlib/live-tools.agel').read_text()
    source += Path('examples/osrs/live-tools-test.agel').read_text()
    source += "\n(import agel/osrs-live-tools-test) (get (test-live-combat game) 'report)"
elif options.attack_xp is not None:
    source += Path('examples/osrs/combat-training.agel').read_text()
    max_fights = min(64, (options.attack_xp + 7) // 8 + 3)
    source += f"\n(import agel/osrs-combat-training) (train-rat-attack game {options.attack_xp} {max_fights})"
else:
    source += "\n(import agel/osrs-combat-test) (get (test-rat-combat game) 'report)"
notice('RuneScape combat: discovering targets, attacking and waiting for XP and removal')
started = time.monotonic()
result = evaluate(source, 43594, timeout=120 if options.attack_xp is None else 30 + max_fights * 300)
assert password not in result.stdout + result.stderr, 'credential output'
assert result.returncode == 0, result.stderr
assert 'passed #t' in result.stdout and 'target-removed #t' in result.stdout, result.stdout
if options.live_tools:
    assert 'live-tool-version 2' in result.stdout and 'preserved-calls 3' in result.stdout, result.stdout
expected_xp = combat_xp(result.stdout)
if options.attack_xp is not None:
    outcome = re.search(r'test rat-training passed #t goal-attack-xp (\d+) fights (\d+) ', result.stdout)
    assert outcome and int(outcome[1]) == options.attack_xp, result.stdout
    fights = int(outcome[2])
    assert 1 <= fights <= max_fights and expected_xp[0] >= options.attack_xp, result.stdout
print(result.stdout.strip(), flush=True)
notice('RuneScape combat: kill observed; verifying persisted Attack and Hitpoints XP')

compiler = Path(os.environ.get('JAVA_HOME', '')) / 'bin/javac'
if not compiler.is_file():
    compiler = Path('/opt/homebrew/opt/openjdk@17/bin/javac')
Path('target/osrs-admin').mkdir(exist_ok=True)
subprocess.run([str(compiler) if compiler.is_file() else 'javac', '-d', 'target/osrs-admin',
                'scripts/osrs/AccountState.java'], check=True)
subprocess.run(['docker', 'cp', 'target/osrs-admin/AccountState.class', 'agel-osrs-world:/tmp/AccountState.class'], check=True, capture_output=True)
for _ in range(45):
    oracle = subprocess.run(['docker', 'exec', 'agel-osrs-world', 'sh', '-c',
        'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" AccountState "$1"', 'probe', username], capture_output=True, text=True)
    if oracle.returncode == 0 and combat_xp_saved(expected_xp, oracle.stdout):
        break
    time.sleep(1)
else:
    raise AssertionError('combat XP did not persist: ' + oracle.stdout + oracle.stderr)
identity = dict(part.split('=', 1) for part in oracle.stdout.splitlines()[0].split())
assert identity['rights'] == '' and int(identity['xp_rate_in_hundreds']) == 100, oracle.stdout
print('persisted:', '\n'.join(line for line in oracle.stdout.splitlines() if line.startswith(('account=', 'stat=0 ', 'stat=3 '))), flush=True)
if options.attack_xp is not None:
    level = re.search(r' attack-level (\d+) encounters ', result.stdout)
    assert level and re.search(rf'^stat=0 base={level[1]} visible={level[1]} ', oracle.stdout, re.M), oracle.stdout
    print(f'Training and verification elapsed seconds: {time.monotonic() - started:.3f}', flush=True)
    notice(f'RuneScape PASS: {fights} encounters reached {expected_xp[0]} saved Attack XP at normal rights')
else:
    notice('RuneScape PASS: observed rat kill, Attack/Hitpoints XP and normal-account persistence')

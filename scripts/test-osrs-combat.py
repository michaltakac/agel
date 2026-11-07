#!/usr/bin/env python3
"""Run the Agel rat-combat smoke test against a fresh normal account."""
import json
import os
from pathlib import Path
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
source += Path('examples/osrs/combat-test.agel').read_text() + "\n(import agel/osrs-combat-test) (get (test-rat-combat game) 'report)"
if '--live-tools' in sys.argv:
    source = source.rsplit("(get (test-rat-combat game) 'report)", 1)[0]
    source += Path('crates/agel-stdlib/live-tools.agel').read_text()
    source += Path('examples/osrs/live-tools-test.agel').read_text()
    source += "\n(import agel/osrs-live-tools-test) (get (test-live-combat game) 'report)"
notice('RuneScape combat: discovering a rat, attacking and waiting for XP and removal')
result = evaluate(source, 43594)
assert password not in result.stdout + result.stderr, 'credential output'
assert result.returncode == 0, result.stderr
assert 'passed #t' in result.stdout and 'target-removed #t' in result.stdout, result.stdout
if '--live-tools' in sys.argv:
    assert 'live-tool-version 2' in result.stdout and 'preserved-calls 3' in result.stdout, result.stdout
expected_xp = combat_xp(result.stdout)
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
notice('RuneScape PASS: observed rat kill, Attack/Hitpoints XP and normal-account persistence')

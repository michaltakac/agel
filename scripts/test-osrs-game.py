#!/usr/bin/env python3
"""Run Agel gameplay tests and verify persisted state on the private server."""
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time
sys.path.insert(0, "scripts/osrs")
from test_support import evaluate, form


def run(*args):
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout


def notice(text):
    print(text, flush=True)
    path = Path('target/osrs-live-os/status.txt')
    if path.parent.exists():
        path.write_text(text)


compiler = Path(os.environ.get('JAVA_HOME', '')) / 'bin/javac'
if not compiler.is_file():
    compiler = Path('/opt/homebrew/opt/openjdk@17/bin/javac')
Path('target/osrs-admin').mkdir(exist_ok=True)
run(str(compiler) if compiler.is_file() else 'javac', '-d', 'target/osrs-admin',
    'scripts/osrs/AccountState.java', 'scripts/osrs/SetupAccount.java')
for name in ['AccountState', 'SetupAccount']:
    run('docker', 'cp', f'target/osrs-admin/{name}.class', f'agel-osrs-world:/tmp/{name}.class')


def oracle(username):
    return run('docker', 'exec', 'agel-osrs-world', 'sh', '-c',
        'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" AccountState "$1"', 'probe', username)


directory = Path('target/osrs-login-probes')
directory.mkdir(mode=0o700, exist_ok=True)


def credentials(username):
    path = directory / (username + '.json')
    if not path.exists():
        fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(fd, 'w') as output:
            json.dump({'username': username, 'password': secrets.token_hex(8)}, output)
    return json.loads(path.read_text())


key = run('docker', 'exec', 'agel-osrs-world', 'cat', '/world/.data/client.key')
assert key.split('Exponent: ')[1].splitlines()[0].strip() == '10001'
modulus = key.split('Modulus: ')[1].strip()
tests = Path('examples/osrs/game-tests.agel').read_text()


def program(account, test=None):
    source = '(import agel/osrs-wire) (import agel/osrs-login) (import agel/osrs-crypto) (import agel/osrs-game) '
    source += '(osrs-js5-open 241 \'(1 2 3 4)) (def crcs (osrs-cache-crcs (osrs-js5-group 255 255))) (wire-close) '
    source += '(def context (rsa-context ' + form(modulus) + ')) (def seed (osrs-random-seed)) '
    source += '(def game (osrs-game-state seed)) (osrs-login ' + form(account['username']) + ' ' + form(account['password'])
    source += ' context crcs seed (entropy-read 24)) '
    if test:
        source += tests + '\n(import agel/osrs-tests) (get (' + test + " game) 'report)"
    return source


def execute(account, test=None):
    for _ in range(12):
        result = evaluate(program(account, test), 43594)
        assert account['password'] not in result.stdout + result.stderr, 'credential output'
        if result.returncode == 0:
            return result.stdout.strip()
        if 'osrs/login-refused' not in result.stderr or 'data: Int(5)' not in result.stderr:
            raise AssertionError(result.stderr)
        notice('RuneScape: waiting for the server to retire the previous session')
        time.sleep(1)
    raise AssertionError('previous session did not retire')


setup = credentials('agelsetup')
try:
    oracle('agelsetup')
except subprocess.CalledProcessError:
    notice('RuneScape: creating the dedicated setup account')
    execute(setup)
run('docker', 'exec', 'agel-osrs-world', 'sh', '-c',
    'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" SetupAccount')
notice('RuneScape: running Agel skill and item setup test')
admin = execute(setup, 'test-admin-setup')
assert 'passed #t' in admin and 'smithing-level 70' in admin and 'coins 100' in admin, admin
notice('RuneScape PASS: Smithing 1 -> 70 and inventory 0 -> 100 coins observed from live packets')
normal = credentials('agel' + secrets.token_hex(4))
control = execute(normal, 'test-walk-control')
assert 'passed #t' in control, control
notice('RuneScape: movement and normal-account permission checks observed; checking persistence')
for _ in range(45):
    saved_admin = oracle('agelsetup')
    saved_normal = oracle(normal['username'])
    if 'x=3223 z=3218 level=0' in saved_normal:
        break
    time.sleep(1)
else:
    raise AssertionError('movement was not persisted')
assert 'rights= ' in saved_normal and 'xp_rate_in_hundreds=100' in saved_normal, saved_normal
assert 'stat=13 base=70 visible=70 fine_xp=7376270' in saved_admin, saved_admin
assert 'item=obj.coins count=100' in saved_admin, saved_admin
print(admin); print(control)
print('persisted control:', saved_normal.splitlines()[0])
print('persisted setup:', '\n'.join(line for line in saved_admin.splitlines() if line.startswith('account=') or 'stat=13 ' in line or 'item=obj.coins ' in line))
notice('RuneScape PASS: live movement, skill/item setup, permission separation and persistence')

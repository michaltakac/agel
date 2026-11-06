#!/usr/bin/env python3
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time
sys.path.insert(0, "scripts/osrs")
from test_support import evaluate, form

state = json.loads(subprocess.check_output(["docker", "inspect", "agel-osrs-world"], text=True))[0]
assert state['State']['Running'] and 'AGEL_PRIVATE_NO_POW=1' in state['Config']['Env']
public_key = subprocess.check_output(["docker", "exec", "agel-osrs-world", "cat", "/world/.data/client.key"], text=True)
assert public_key.split('Exponent: ')[1].splitlines()[0].strip() == '10001'
modulus = public_key.split('Modulus: ')[1].strip()
username = 'agel' + secrets.token_hex(4)
password = secrets.token_hex(8)
directory = Path('target/osrs-login-probes')
directory.mkdir(mode=0o700, exist_ok=True)
descriptor = os.open(directory / (username + '.json'), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
with os.fdopen(descriptor, 'w') as output:
    json.dump({'username': username, 'password': password}, output)


def login(password_attempt):
    source = '(import agel/osrs-wire) (import agel/osrs-login) (import agel/osrs-crypto) ' + \
        '(osrs-js5-open 241 \'(1 2 3 4)) ' + \
        '(def crcs (osrs-cache-crcs (osrs-js5-group 255 255))) (wire-close) ' + \
        '(def context (rsa-context ' + form(modulus) + ')) ' + \
        '(osrs-login ' + form(username) + ' ' + form(password_attempt) + \
        ' context crcs (osrs-random-seed) (entropy-read 24))'
    result = evaluate(source, 43594)
    assert password not in result.stdout + result.stderr, 'credentials appeared in output'
    return result


def settled_login(password_attempt):
    for _ in range(12):
        result = login(password_attempt)
        if 'osrs/login-refused' not in result.stderr or 'data: Int(5)' not in result.stderr:
            return result
        print('live server is still retiring the previous session (status 5)', flush=True)
        time.sleep(1)
    raise AssertionError('server did not retire the previous session within the retry bound')


for attempt, correct in [(password, True), (password + 'bad', False), (password, True)]:
    result = settled_login(attempt)
    if correct:
        assert result.returncode == 0 and 'phase logged-in' in result.stdout, result.stderr
        print('live account login accepted')
    else:
        assert result.returncode != 0 and 'osrs/login-refused' in result.stderr and 'data: Int(3)' in result.stderr, result.stderr
        print('live wrong password refused with status 3')

compiler = Path(os.environ.get('JAVA_HOME', '')) / 'bin/javac'
if not compiler.is_file():
    compiler = Path('/opt/homebrew/opt/openjdk@17/bin/javac')
subprocess.run([str(compiler) if compiler.is_file() else 'javac', '-d', 'target/osrs-admin',
                'scripts/osrs/AccountState.java'], check=True)
subprocess.run(['docker', 'cp', 'target/osrs-admin/AccountState.class',
                'agel-osrs-world:/tmp/AccountState.class'], check=True, capture_output=True)
oracle = subprocess.check_output(['docker', 'exec', 'agel-osrs-world', '/bin/sh', '-c',
    'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" AccountState "$1"', 'probe', username], text=True)
assert 'rights= ' in oracle and 'xp_rate_in_hundreds=100' in oracle and 'x=3221 z=3218 level=0' in oracle, oracle
print('live persisted character:', oracle.strip())
print('OSRS live login: new account, wrong password, reconnect and authoritative persistence passed')

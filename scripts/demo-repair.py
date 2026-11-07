#!/usr/bin/env python3
"""Byte/process carrier for the general Agel repair demonstration."""
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time


environment = dict(os.environ)
if Path('.env').is_file():
    for line in Path('.env').read_text().splitlines():
        if line.startswith('TYPESAFEAI_API_KEY='):
            values = shlex.split(line.partition('=')[2])
            if values:
                environment.setdefault('TYPESAFEAI_API_KEY', values[0])
if not environment.get('TYPESAFEAI_API_KEY'):
    raise SystemExit('Set TYPESAFEAI_API_KEY for the real text judge')

subprocess.run(['cargo', 'build', '-q', '--release', '-p', 'agel-cli'], check=True)
if len(sys.argv) == 3 and sys.argv[1] == '--resume':
    directory = Path(sys.argv[2])
    if not (directory / 'session.image').is_file():
        raise SystemExit('Missing session.image for resume')
    source = ''
elif len(sys.argv) == 1:
    directory = Path('target') / ('repair-demo-' + str(time.time_ns()))
    directory.mkdir(mode=0o700)
    source = Path('crates/agel-stdlib/live-tools.agel').read_text() + '\n'
    source += Path('crates/agel-stdlib/repair.agel').read_text() + '\n'
    source += Path('examples/general-repair.agel').read_text() + '\n'
else:
    raise SystemExit('usage: demo-repair.py [--resume DIRECTORY]')
# Give each agent its own deterministic evaluation budget.
source += '\n:dispatch\n(run 1)\n(run 1)\n' * 4
source += '''
(list 'repair-report (get (repair-state batches) 'status) (get (repair-state readiness) 'status)
  (list (get (live-tools-call (repair-state batches) 'batches '(5 3)) 'value)
        (get (live-tools-call (repair-state batches) 'batches '(10 3)) 'value)
        (get (live-tools-call (repair-state readiness) 'choose '(nil 41)) 'value)
        (get (live-tools-call (repair-state readiness) 'choose '(#t 73)) 'value))
  (list (meta-apply (meta-eval batch-source (meta-base-env)) '(5 3))
        (meta-apply (meta-eval batch-source (meta-base-env)) '(10 3))
        (meta-apply (meta-eval readiness-source (meta-base-env)) '(nil 41))
        (meta-apply (meta-eval readiness-source (meta-base-env)) '(#t 73))))
(get (repair-state batches) 'candidate)
(get (repair-state readiness) 'candidate)
(begin (def batch-state (repair-state batches)) (def ready-state (repair-state readiness)) nil)
(list 'batch-score (get (repair-evaluate batch-state 1) 'correct)
  (get (repair-evaluate batch-state (evolution-current (get batch-state 'versions) 'cell 'batches)) 'correct)
  (get (repair-evaluate batch-state 1) 'total))
(list 'readiness-score (get (repair-evaluate ready-state 1) 'correct)
  (get (repair-evaluate ready-state (evolution-current (get ready-state 'versions) 'cell 'choose)) 'correct)
  (get (repair-evaluate ready-state 1) 'total))
:requests
:quit
'''
started = time.monotonic()
result = subprocess.run(['target/release/agel-cli', '--enable-codex', '--enable-jev',
    '--model-workspace', str(directory.resolve()), '--model-timeout-seconds', '90',
    '--image', str(directory / 'session.image')], input=source, capture_output=True,
    text=True, env=environment, timeout=420)
log = directory / ('output-' + str(time.time_ns()) + '.txt')
with os.fdopen(os.open(log, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600), 'w') as output:
    output.write(result.stdout + result.stderr)
print('Evidence:', directory, flush=True)
print('Elapsed seconds:', round(time.monotonic() - started, 3), flush=True)
print(result.stdout[-4500:], flush=True)
expected = ['(repair-report done done (2 4 nil 73) (1 3 41 73))',
            '(batch-score 4 8 8)', '(readiness-score 3 6 6)']
if result.returncode or any(value not in result.stdout for value in expected) or 'evaluation error:' in result.stderr:
    print(result.stderr[-4000:], flush=True)
    raise SystemExit('Real repair demonstration failed; retained source, results and journal for inspection')
print('PASS: both application-neutral tools repaired; frozen controls retain their failures')

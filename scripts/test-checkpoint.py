"""Exercise Agel checkpoint recovery across native OS reboots."""
from pathlib import Path
import socket
import re
import subprocess
import tempfile
import time
import graphical_console as console


def run(machine, command):
    reply = machine.submit(command, 600)
    deadline = time.monotonic() + 600
    while not re.search(r'process agel exited with status [0-9]+\r?\n', reply):
        if time.monotonic() > deadline:
            raise TimeoutError(reply[-2000:])
        try:
            part = machine.serial.recv(4096)
        except socket.timeout:
            continue
        if not part:
            raise RuntimeError('guest closed before process completion')
        reply += part.decode('utf-8', errors='replace')
    print(reply, flush=True)
    assert 'checkpoint: PASS:' in reply and 'process agel exited with status 0' in reply, reply[-2000:]


output = Path('target') / ('checkpoint-' + str(time.time_ns()))
output.mkdir()
print('Evidence:', output, flush=True)
disk = console.prepared_image('target/boot/agel-v1.img', output, blank_sectors=1024)
catalog = Path('crates/agel-stdlib/stdlib.agel').read_text()
common = ''
for name in ['sequence', 'meta']:
    start = catalog.index('(module agel/' + name + '\n')
    end = catalog.find('\n(module ', start + 1)
    common += catalog[start:end if end >= 0 else len(catalog)] + '\n'
common += '(module agel/native-reader (export native-read) (def native-read ' + Path('crates/agel-stdlib/native-reader.agel').read_text() + '))\n'
common += ''.join(Path('crates/agel-stdlib/' + name + '.agel').read_text() + '\n' for name in
                  ['evolution', 'judge-log', 'bytes', 'data-codec', 'checkpoint', 'live-runtime'])
common += Path('examples/native-checkpoint-common.agel').read_text()
subprocess.run(['python3', 'scripts/install-program.py', disk, 'agel',
                'boot/posix/target/x86_64-unknown-none/release/agel'], check=True, capture_output=True)
for phase in ['write', 'recover', 'revert']:
    source = output / (phase + '.agel')
    source.write_text(common + Path('examples/native-checkpoint-' + phase + '.agel').read_text())
    subprocess.run(['python3', 'scripts/install-program.py', '--region', 'data', disk, phase, str(source)], check=True, capture_output=True)
with tempfile.TemporaryDirectory(prefix='agel-checkpoint-', dir='/tmp') as sockets:
    for index, phase in enumerate(['write', 'recover', 'revert']):
        directory = Path(sockets) / phase
        directory.mkdir()
        machine = console.Machine(disk, str(directory))
        try:
            if index == 0:
                assert 'formatted' in machine.submit(':fs-format')
            run(machine, ':exec agel -- --no-stdlib --fuel 65000000 /data/' + phase)
            assert 'refused the effect' in machine.submit('(file-write "judge-log" "erased")')
        finally:
            machine.close()
print('PASS: Agel recovery, torn-tail fallback, continued commits and guarded revert across two OS reboots', flush=True)

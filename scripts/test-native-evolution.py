"""Bootstrap bytes once, then let Agel compile and register real programs."""
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

import graphical_console as console


def run(machine, line, seconds=600):
    reply = machine.submit(line, seconds)
    if 'exited with status' not in reply:
        deadline = time.monotonic() + seconds
        while 'exited with status' not in reply:
            if time.monotonic() > deadline:
                raise TimeoutError(reply[-3000:])
            try:
                piece = machine.serial.recv(1)
            except socket.timeout:
                continue
            if not piece:
                raise RuntimeError('guest stopped')
            reply += piece.decode('utf-8', errors='replace')
        machine.serial.settimeout(1)
        try:
            while True:
                piece = machine.serial.recv(4096)
                if not piece:
                    break
                reply += piece.decode('utf-8', errors='replace')
        except socket.timeout:
            pass
        finally:
            machine.serial.settimeout(15)
    print(reply, flush=True)
    return reply


with tempfile.TemporaryDirectory(prefix='agel-native-evolve-', dir='/tmp') as directory:
    disk = console.prepared_image(sys.argv[1], directory, blank_sectors=1024)
    for name, source in [('agel', sys.argv[2]), ('program-io-check', sys.argv[3])]:
        subprocess.run(['python3', 'scripts/install-program.py', disk, name, source], check=True, capture_output=True)
    text = ''.join(Path('crates/agel-stdlib/' + name).read_text() + '\n'
                   for name in ['bytes.agel', 'program-store.agel', 'native-program.agel'])
    text += Path('examples/native-evolution.agel').read_text()
    source = Path(directory) / 'native-evo.agel'
    source.write_text(text)
    subprocess.run(['python3', 'scripts/install-program.py', '--region', 'data', disk,
                    'native-evo.agel', str(source)], check=True, capture_output=True)
    machine = console.Machine(disk, directory)
    try:
        assert 'formatted' in machine.submit(':fs-format')
        reply = run(machine, ':exec program-io-check')
        assert 'bounded-program-io' in reply and 'exited with status 0' in reply
        reply = run(machine, ':exec program-io-check / ro')
        assert 'read-only' in reply and 'exited with status 0' in reply
        reply = run(machine, ':exec agel -- /data/native-evo.agel')
        assert 'native-program: PASS:' in reply and 'process agel exited with status 0' in reply, reply[-4000:]
        assert 'native-domain: PASS:' in reply, reply[-4000:]
        assert reply.count('judge-start') == 9 and reply.count('judge-result') == 9 and 'cost-microusd 0' in reply, reply
        assert 'refused the effect' in machine.submit('(file-write "judge-log" "erased")')
        # After the Agel controller exits, its retained ELF versions still run.
        for name, expected in [('v1a0c0', 41), ('v2a4c0', 42)]:
            reply = run(machine, ':exec ' + name)
            assert 'exited with status ' + str(expected) in reply
        machine.close()
        reboot = Path(directory) / 'reboot'
        reboot.mkdir()
        machine = console.Machine(disk, str(reboot))
        for name, expected in [('v1a0c0', 41), ('v2a4c0', 42)]:
            reply = run(machine, ':exec ' + name)
            assert 'exited with status ' + str(expected) in reply
        assert 'judge-start' in machine.submit('(file-read "judge-log")')
        print('PASS: retained ELF versions and judge log survive reboot', flush=True)
        print('PASS: native program bytes installed and selected by Agel; controller state and previous images retained', flush=True)
    finally:
        machine.close()

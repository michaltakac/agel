"""Verify that a delayed judge receipt survives native Agel exit without retrying."""
import json
from pathlib import Path
import subprocess
import tempfile
import time

from graphical_console import prepared_image
from native_provider_support import decode_judge_ledger, provider_environment


def verify_exit(status, cost, *, lose_guest=False):
    with tempfile.TemporaryDirectory(prefix='agel-judge-exit-', dir='/tmp') as root:
        directory = Path(root)
        disk = prepared_image('target/boot/agel-v1.img', directory, blank_sectors=1024)
        source = directory / 'async.agel'
        prompt = ('(judge "Exit fixture" (noul "patch" "Accept?") '
                  '(choice "reason" "Why?" ("none" "No defect") ("contract" "Contract") '
                  '("boundary" "Boundary") ("syntax" "Syntax") ("effects" "Effects")))')
        request = json.dumps('AGEL-INFER/2 start jev\n' + prompt.encode().hex())
        source.write_text(f'(def pending (model-request {request}))\n(console-log pending)\n')
        for arguments in (
            [disk, 'agel', 'boot/posix/target/x86_64-unknown-none/release/agel'],
            ['--region', 'data', disk, 'async', str(source)],
        ):
            subprocess.run(['python3', 'scripts/install-program.py', *arguments], check=True)
        environment = provider_environment(directory, False)
        curl = directory / 'curl-fixture'
        delayed = curl.read_text().replace(
            'cat >/dev/null\n', 'cat >/dev/null\ntouch "$0.dispatched"\nsleep 6\n')
        curl.write_text(delayed.replace(r'\n200', rf'\n{status}'))
        socket = directory / 'serial'
        machine = subprocess.Popen([
            'qemu-system-x86_64', '-machine', 'pc,accel=tcg', '-m', '128M', '-display', 'none',
            '-serial', f'unix:{socket},server=on,wait=on', '-drive', f'format=raw,file={disk},if=ide',
        ], stderr=subprocess.DEVNULL)
        try:
            deadline = time.monotonic() + 10
            while not socket.exists():
                if machine.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError('guest serial socket unavailable')
                time.sleep(0.05)
            bridge = [
                'target/release/examples/guest_inference', str(socket), root,
                'process agel exited with status', ':exec agel -- --no-stdlib /data/async',
            ]
            if lose_guest:
                with subprocess.Popen(bridge, env=environment, text=True, stdout=subprocess.PIPE,
                                      stderr=subprocess.STDOUT) as run:
                    dispatched = curl.with_suffix('.dispatched')
                    deadline = time.monotonic() + 30
                    while not dispatched.exists():
                        if run.poll() is not None or time.monotonic() >= deadline:
                            run.kill()
                            raise RuntimeError('judge fixture did not dispatch')
                        time.sleep(0.05)
                    machine.terminate()
                    output, _ = run.communicate(timeout=60)
                    assert run.returncode != 0, output
            else:
                run = subprocess.run(bridge, env=environment, text=True, capture_output=True, timeout=90)
                assert run.returncode == 0, (run.stdout, run.stderr)
                assert 'provider jev' in run.stdout and 'pending ' in run.stdout
                assert 'process agel exited with status 0' in run.stdout
                assert 'completed job' not in run.stdout
        finally:
            if machine.poll() is None:
                machine.terminate()
            machine.wait(timeout=10)
        records = decode_judge_ledger(disk, directory, environment['TYPESAFEAI_API_KEY'],
                                      False, required=not lose_guest)
        recovery = (directory / 'judge-recovery.agel').read_text()
        if lose_guest:
            assert records.count('(judge-start ') == 1 and '(judge-result ' not in records
            assert recovery.startswith(records)
            records = recovery
        else:
            assert recovery == records
        assert records.count('(judge-start ') == records.count('(judge-result ') == 1
        assert f'status {status} ' in records and f'cost-nanousd {cost} ' in records, records
        outcome = 'guest power loss' if lose_guest else 'guest process exit'
        print(f'PASS: judge status {status} and its cost survived {outcome} without another call')


if __name__ == '__main__':
    verify_exit(200, 4200)
    verify_exit(429, 0)
    verify_exit(200, 4200, lose_guest=True)

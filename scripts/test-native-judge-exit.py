"""Verify that a delayed judge receipt survives a native Agel process exit."""
import json
from pathlib import Path
import subprocess
import tempfile
import time

from graphical_console import prepared_image
from native_provider_support import decode_judge_ledger, provider_environment


def main():
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
        curl.write_text(curl.read_text().replace('cat >/dev/null\n', 'cat >/dev/null\nsleep 6\n'))
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
            run = subprocess.run([
                'target/release/examples/guest_inference', str(socket), root,
                'process agel exited with status', ':exec agel -- --no-stdlib /data/async',
            ], env=environment, text=True, capture_output=True, timeout=90)
            assert run.returncode == 0, (run.stdout, run.stderr)
            assert 'provider jev' in run.stdout and 'pending ' in run.stdout
            assert 'process agel exited with status 0' in run.stdout
            assert 'completed job' not in run.stdout
        finally:
            machine.terminate()
            machine.wait(timeout=10)
        records = decode_judge_ledger(disk, directory, environment['TYPESAFEAI_API_KEY'], False)
        assert records.count('(judge-start ') == records.count('(judge-result ') == 1
        assert 'cost-nanousd 4200' in records
        print('PASS: the delayed judge receipt survived guest process exit before any poll')


if __name__ == '__main__':
    main()

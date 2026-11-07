"""Run fixed and live adaptive arms with the same runtime artifacts."""

import argparse

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def run_arm(output, runtime, name, options):
    command = [sys.executable, 'scripts/test-native-network.py', *runtime, *options]
    evidence = None
    with (output / f'{name}.log').open('w', buffering=1) as log, subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    ) as run:
        for line in run.stdout:
            print(line, end='', flush=True)
            log.write(line)
            if line.startswith('Evidence: '):
                evidence = line.removeprefix('Evidence: ').strip()
        status = run.wait()
    if evidence is None:
        raise SystemExit(f'{name} exited {status} before creating evidence')
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--craft', nargs=2, metavar=('FIXED_FIXTURE', 'ADAPTIVE_FIXTURE'),
                        help='use two distinct accounts staged with --prepare-only')
    args = parser.parse_args()
    task = 'craft' if args.craft else 'economy'
    if args.craft and args.craft[0] == args.craft[1]:
        parser.error('--craft requires two distinct fixtures')
    output = Path(tempfile.mkdtemp(prefix=f'native-{task}-pair-', dir='target'))
    print('Pair evidence:', output, flush=True)
    runtime = []
    digests = {}
    for option, source in (
        ('--boot-image', 'target/boot/agel-v1.img'),
        ('--agel-executable', 'boot/posix/target/x86_64-unknown-none/release/agel'),
        ('--inference-bridge', 'target/release/examples/guest_inference'),
    ):
        destination = output / Path(source).name
        shutil.copy2(source, destination)
        with destination.open('rb') as copied:
            digests[destination.name] = hashlib.file_digest(copied, 'sha256').hexdigest()
        runtime.extend([option, str(destination)])
    (output / 'runtime.json').write_text(json.dumps(digests, indent=2) + '\n')
    sources = output / 'sources'
    for area in (Path('crates/agel-stdlib'), Path('examples'), Path('examples/osrs')):
        destination = sources / area
        destination.mkdir(parents=True, exist_ok=True)
        for source in area.glob('*.agel'):
            shutil.copy2(source, destination / source.name)
    runtime.extend(['--source-root', str(sources)])
    fixed_options, adaptive_options = [f'--{task}'], [f'--{task}', '--optimize', '--live']
    if args.craft:
        fixed_options.extend(['--craft-fixture', args.craft[0]])
        adaptive_options.extend(['--craft-fixture', args.craft[1]])
    fixed = run_arm(output, runtime, 'fixed', fixed_options)
    adaptive = run_arm(output, runtime, 'adaptive', adaptive_options)
    score = subprocess.check_output([sys.executable, 'scripts/compare-native-combat.py',
                                     f'--{task}', fixed, adaptive])
    (output / 'score.agel').write_bytes(score)
    print('Agel score input:', output / 'score.agel', flush=True)


if __name__ == '__main__':
    main()

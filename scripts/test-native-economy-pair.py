"""Run sequential native benchmark arms with matching runtime artifacts."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def run_arm(output, runtime, name, options):
    command = [sys.executable, str(output / 'harness/test-native-network.py'), *runtime, *options]
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


def pin_harness(output):
    harness = output / 'harness'
    (harness / 'osrs').mkdir(parents=True)
    digests = {}
    for name in ('test-native-network.py', 'compare-native-combat.py', 'native_provider_support.py',
                 'graphical_console.py', 'install-program.py', 'osrs/test_support.py',
                 'osrs/world_manifest.py', 'osrs/AccountState.java'):
        copy = harness / name
        shutil.copy2(Path(__file__).resolve().parent / name, copy)
        digests[f'harness/{name}'] = hashlib.sha256(copy.read_bytes()).hexdigest()
    reader = harness / 'journal-reader'
    shutil.copy2('target/debug/examples/journal', reader)
    with reader.open('rb') as copied:
        digests['harness/journal-reader'] = hashlib.file_digest(copied, 'sha256').hexdigest()
    return digests


def pin_transfer(output, evidence):
    donor = output / 'donor'
    donor.mkdir()
    for name in ('account.json', 'world.json', 'serial.log', 'saved-account.txt',
                 'judge.agel', 'judge.frames', 'disk.img'):
        shutil.copy2(evidence / name, donor / name)
    recovery = evidence / 'judge-recovery.agel'
    if recovery.is_file():
        shutil.copy2(recovery, donor / recovery.name)

    fixed_seed = output / 'transfer-fixed-seed.img'
    shutil.copy2(donor / 'disk.img', fixed_seed)
    # The native runner clears workspace and filesystem sectors on fixed
    # boot. Remove the separate protected journal from its seed as well.
    journal_start = 131072 * 512
    size = fixed_seed.stat().st_size
    if size < journal_start:
        raise SystemExit('donor disk is too short for a protected journal')
    with fixed_seed.open('r+b') as image:
        image.truncate(journal_start)
        image.truncate(size)
    return donor, fixed_seed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    tasks = parser.add_mutually_exclusive_group()
    tasks.add_argument('--transfer', type=Path, metavar='DONOR',
                       help='compare a retained one-rat policy against a fresh fixed account')
    tasks.add_argument('--quest', action='store_true', help='compare fresh Cook’s Assistant runs')
    tasks.add_argument('--craft', nargs=2, metavar=('FIXED_FIXTURE', 'ADAPTIVE_FIXTURE'),
                        help='use two distinct accounts staged with --prepare-only')
    args = parser.parse_args()
    task = 'combat' if args.transfer else 'craft' if args.craft else 'quest' if args.quest else 'economy'
    if args.craft and args.craft[0] == args.craft[1]:
        parser.error('--craft requires two distinct fixtures')
    output = Path(tempfile.mkdtemp(prefix=f'native-{task}-pair-', dir='target'))
    print('Pair evidence:', output, flush=True)
    donor = None
    boot_image = Path('target/boot/agel-v1.img')
    if args.transfer:
        donor, boot_image = pin_transfer(output, args.transfer)
    runtime = []
    digests = pin_harness(output)
    for option, source in (
        ('--boot-image', boot_image),
        ('--agel-executable', 'boot/posix/target/x86_64-unknown-none/release/agel'),
        ('--inference-bridge', 'target/release/examples/guest_inference'),
    ):
        destination = output / Path(source).name
        if source != destination:
            shutil.copy2(source, destination)
        with destination.open('rb') as copied:
            digests[destination.name] = hashlib.file_digest(copied, 'sha256').hexdigest()
        runtime.extend([option, str(destination)])
    if donor:
        with (donor / 'disk.img').open('rb') as copied:
            digests['donor/disk.img'] = hashlib.file_digest(copied, 'sha256').hexdigest()
    sources = output / 'sources'
    for area in (Path('crates/agel-stdlib'), Path('examples'), Path('examples/osrs')):
        destination = sources / area
        destination.mkdir(parents=True, exist_ok=True)
        for source in area.glob('*.agel'):
            copy = destination / source.name
            shutil.copy2(source, copy)
            digests[str(copy.relative_to(output))] = hashlib.sha256(copy.read_bytes()).hexdigest()
    (output / 'runtime.json').write_text(json.dumps(digests, indent=2) + '\n')
    runtime.extend(['--source-root', str(sources)])
    fixed_options, adaptive_options = [f'--{task}'], [f'--{task}', '--optimize', '--live']
    if donor:
        fixed_options.append('--pace-combat')
        adaptive_options = ['--combat', '--pace-combat', '--transfer-from', str(donor)]
    if args.craft:
        fixed_options.extend(['--craft-fixture', args.craft[0]])
        adaptive_options.extend(['--craft-fixture', args.craft[1]])
    fixed = run_arm(output, runtime, 'fixed', fixed_options)
    adaptive = run_arm(output, runtime, 'adaptive', adaptive_options)
    scoring_options = ['--donor', str(donor)] if donor else [f'--{task}']
    score = subprocess.check_output([sys.executable, str(output / 'harness/compare-native-combat.py'),
                                     *scoring_options, fixed, adaptive])
    (output / 'score.agel').write_bytes(score)
    print('Agel score input:', output / 'score.agel', flush=True)


if __name__ == '__main__':
    main()

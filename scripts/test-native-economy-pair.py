"""Run sequential native benchmark arms with matching runtime artifacts."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time

import graphical_console as console


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
                 'judge.agel', 'judge.frames', 'disk.img', 'mode.txt', 'client.agel'):
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


def evaluate_score(output, boot_image, executable, memory_mib):
    library = output / 'sources/crates/agel-stdlib'
    sequence = (library / 'stdlib.agel').read_text().split('\n(module agel/result', 1)[0]
    score = output / 'score.agel'
    with tempfile.TemporaryDirectory(prefix='agel-score-', dir='/tmp') as scratch:
        disk = console.prepared_image(boot_image, scratch, blank_sectors=1024)
        program = Path(scratch) / 'benchmark.agel'
        program.write_text(sequence + '\n' + (library / 'benchmark.agel').read_text() +
                           '\n(console-log (text-concat "benchmark-result: " (print-form (begin\n' +
                           score.read_text() + '\n))))\n')
        install = [sys.executable, str(output / 'harness/install-program.py')]
        subprocess.run([*install, disk, 'agel', str(executable)], check=True, capture_output=True)
        subprocess.run([*install, '--region', 'data', disk, program.name, str(program)],
                       check=True, capture_output=True)
        machine = console.Machine(disk, scratch, memory_mib=memory_mib)
        try:
            reply = machine.submit(':exec agel -- --no-stdlib /data/benchmark.agel', seconds=120)
            deadline = time.monotonic() + 120
            while 'process agel exited with status ' not in reply:
                if time.monotonic() >= deadline:
                    raise TimeoutError('guest benchmark evaluator did not exit')
                try:
                    chunk = machine.serial.recv(4096)
                except socket.timeout:
                    continue
                if not chunk:
                    raise RuntimeError('guest closed the benchmark connection')
                reply += chunk.decode('utf-8', errors='replace')
            (output / 'score-native.log').write_text(reply)
            reports = [line.removeprefix('benchmark-result: ') for line in reply.splitlines()
                       if line.startswith('benchmark-result: ')]
            if 'process agel exited with status 0' not in reply or len(reports) != 1:
                raise RuntimeError(f'guest benchmark evaluation failed: {output / "score-native.log"}')
            (output / 'score-result.agel').write_text(reports[0] + '\n')
        finally:
            machine.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    tasks = parser.add_mutually_exclusive_group()
    tasks.add_argument('--transfer', type=Path, metavar='DONOR',
                       help='compare a retained combat policy against a fresh fixed account')
    tasks.add_argument('--combat', action='store_true', help='compare in-session one-rat combat adaptation')
    tasks.add_argument('--melee-xp', type=int, metavar='GOAL',
                       help='compare fixed and adaptive three-skill melee training')
    tasks.add_argument('--quest', action='store_true', help='compare fresh Cook’s Assistant runs')
    tasks.add_argument('--craft', nargs=2, metavar=('FIXED_FIXTURE', 'ADAPTIVE_FIXTURE'),
                        help='use two distinct accounts staged with --prepare-only')
    parser.add_argument('--order', choices=('fixed-first', 'adaptive-first'), default='fixed-first',
                        help='choose the order of the two sequential arms')
    parser.add_argument('--source-root', type=Path, default=Path('.'),
                        help='Agel source tree to pin for both arms')
    args = parser.parse_args()
    for source in ('crates/agel-stdlib/stdlib.agel', 'examples/osrs/osrs-wire.agel'):
        if not (args.source_root / source).is_file():
            parser.error(f'source tree is missing {source}: {args.source_root}')
    melee_goal = args.melee_xp
    if args.transfer:
        donor_manifest = json.loads((args.transfer / 'world.json').read_text())
        melee_goal = donor_manifest.get('goal_xp')
        policy_sources = [('combat_policy_sha256', 'examples/osrs/combat-test.agel')]
        if melee_goal is not None:
            policy_sources.extend([
                ('training_policy_sha256', 'examples/osrs/combat-training.agel'),
                ('service_policy_sha256', 'examples/native-service-optimize.agel'),
            ])
        for field, source in policy_sources:
            path = args.source_root / source
            if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != donor_manifest.get(field):
                parser.error(f'donor policy differs from {path}; select its matching source tree')
    if melee_goal is not None and (type(melee_goal) is not int or not 1 <= melee_goal <= 512):
        parser.error('melee goal must be between 1 and 512 XP')
    task = ('melee' if melee_goal is not None else 'combat' if args.transfer or args.combat else
            'craft' if args.craft else 'quest' if args.quest else 'economy')
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
        for source in (args.source_root / area).glob('*.agel'):
            copy = destination / source.name
            shutil.copy2(source, copy)
            digests[str(copy.relative_to(output))] = hashlib.sha256(copy.read_bytes()).hexdigest()
    (output / 'runtime.json').write_text(json.dumps(digests, indent=2) + '\n')
    runtime.extend(['--source-root', str(sources)])
    scenario = ['--melee-xp', str(melee_goal)] if task == 'melee' else [f'--{task}']
    if task == 'combat':
        scenario.append('--pace-combat')
    fixed_options = list(scenario)
    adaptive_options = [*scenario, '--transfer-from', str(donor)] if donor else [*scenario, '--optimize', '--live']
    if args.craft:
        fixed_options.extend(['--craft-fixture', args.craft[0]])
        adaptive_options.extend(['--craft-fixture', args.craft[1]])
    arms = {'fixed': fixed_options, 'adaptive': adaptive_options}
    order = ('fixed', 'adaptive') if args.order == 'fixed-first' else ('adaptive', 'fixed')
    (output / 'plan.json').write_text(json.dumps({
        'task': task, 'goal_xp': melee_goal, 'order': order,
    }, indent=2) + '\n')
    results = {arm: run_arm(output, runtime, arm, arms[arm]) for arm in order}
    fixed, adaptive = results['fixed'], results['adaptive']
    scoring_options = [f'--{task}'] if task != 'combat' or args.combat else []
    if donor:
        scoring_options.extend(['--donor', str(donor)])
        if task == 'melee':
            scoring_options.append('--transfer')
    score = subprocess.check_output([sys.executable, str(output / 'harness/compare-native-combat.py'),
                                     *scoring_options, fixed, adaptive])
    (output / 'score.agel').write_bytes(score)
    print('Agel score input:', output / 'score.agel', flush=True)
    memory_mib = json.loads((Path(fixed) / 'world.json').read_text())['guest_memory_mib']
    evaluate_score(output, output / boot_image.name, output / 'agel', memory_mib)
    print('Agel OS score:', output / 'score-result.agel', flush=True)


if __name__ == '__main__':
    main()

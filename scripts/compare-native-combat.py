"""Package verified native combat runs as input for Agel's paired scorer."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import sys
import tempfile

from native_provider_support import decode_judge_frames, judge_cost, judge_frames

sys.path.insert(0, str(Path(__file__).with_name('osrs')))
from test_support import combat_xp, combat_xp_saved, form, melee_levels_saved, melee_xp


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def boot_identity(directory, read_table):
    disk = directory / 'disk.img'
    program = next(row for row in read_table(disk) if row['name'] == 'agel')
    with disk.open('rb') as source:
        boot = hashlib.sha256(source.read(1024 * 512)).hexdigest()
        source.seek(65536 * 512)
        kernel = hashlib.sha256(source.read(4096 * 512)).hexdigest()
        source.seek(program['start'] * 512)
        executable = hashlib.sha256(source.read(program['length'])).hexdigest()
    return {'boot': boot, 'kernel_slot_a': kernel, 'agel': executable}


def installed_client(directory, read_table):
    disk = directory / 'disk.img'
    row = next(entry for entry in read_table(disk, 'data') if entry['name'] == 'client')
    with disk.open('rb') as image:
        image.seek(row['start'] * 512)
        source = image.read(row['length'])
    require(source == (directory / 'client.agel').read_bytes(),
            f'{directory}: installed Agel program differs from recorded source')
    return source.decode()


def installed_judge_ledger(directory):
    frames = judge_frames(directory / 'disk.img')
    require(frames == (directory / 'judge.frames').read_bytes(),
            f'{directory}: judge frames differ from the guest journal')
    with tempfile.TemporaryDirectory(prefix='agel-judge-') as scratch:
        path = Path(scratch) / 'judge.frames'
        path.write_bytes(frames)
        ledger = decode_judge_frames(path)
    require(ledger == (directory / 'judge.agel').read_text(),
            f'{directory}: judge records differ from the guest journal')
    return ledger


def load_arm(directory, policy, read_table):
    manifest = json.loads((directory / 'world.json').read_text())
    trace = (directory / 'serial.log').read_text()
    saved = (directory / 'saved-account.txt').read_text()
    require(manifest['encoder_policy'] == policy, f'{directory}: unexpected policy')
    require('process agel exited with status 0' in trace, f'{directory}: no successful guest exit')
    goal = manifest.get('goal_xp')
    charges = []
    if goal is None:
        expected = combat_xp(trace)
        require(re.search(r'^native-combat: \{test rat-combat passed #t .*target-removed #t\}', trace, re.M),
                f'{directory}: no verified combat report')
    else:
        expected, levels = melee_xp(trace, goal)
        require(melee_levels_saved(levels, saved), f'{directory}: saved melee levels or stance differ')
    if policy == 'adaptive':
        activated = ('native-opt: policy committed after text judgment and measured replay' in trace
                     and re.search(r'^native-opt: live uses [1-9]\d*$', trace, re.M))
        rolled_back = ('native-opt: continued with fixed policy after exhausted retries' in trace
                       and re.search(r'^native-opt: live uses 0$', trace, re.M))
        require(goal is not None and 'native-opt: recorded workload verified in append-only log' in trace
                and bool(activated) != bool(rolled_back),
                f'{directory}: no verified in-session policy decision')
        require((directory / 'mode.txt').read_text().strip() == 'live',
                f'{directory}: adaptive scoring requires live providers')
        ledger = installed_judge_ledger(directory)
        starts = re.findall(r'^\(judge-start .*$', ledger, re.M)
        receipts = re.findall(r'^\(judge-result .*$', ledger, re.M)
        identity_pattern = re.compile(r'^\(judge-(?:start|result) id "([^"]+)".*? attempt (\d+)\b')
        started = [identity_pattern.match(line) for line in starts]
        completed = [identity_pattern.match(line) for line in receipts]
        dispatched = re.findall(r'^inference-bridge: started job (\d+) provider jev ', trace, re.M)
        require(all(started) and all(completed), f'{directory}: missing judge call identity')
        keys = [match.groups() for match in started]
        require(len(set(keys)) == len(keys) and sorted(keys) == sorted(match.groups() for match in completed),
                f'{directory}: unmatched or duplicate judge receipts')
        receipt_jobs = {identity.rsplit('/', 1)[-1] for identity, _ in keys}
        require(len(set(dispatched)) == len(dispatched) and receipt_jobs == set(dispatched),
                f'{directory}: judge ledger differs from bridge dispatches')
        for receipt in receipts:
            charges.append(('judge', judge_cost(receipt)))
        writers = re.findall(r'^inference-bridge: started job \d+ provider codex ', trace, re.M)
        require(writers, f'{directory}: no writer dispatch')
        charges.extend(('model', None) for _ in writers)
        print(f'; adaptive policy: {"activated" if activated else "retained fixed after refusal"}')
    else:
        require('inference-bridge: started job' not in trace, f'{directory}: evaluation incurred inference')
    require(combat_xp_saved(expected, saved), f'{directory}: saved XP differs from observations')
    identity = re.search(r'^account=(\d+) character=\d+ rights= x=\d+ z=\d+ level=0 xp_rate_in_hundreds=100$',
                         saved, re.M)
    require(identity, f'{directory}: expected saved normal account at 1× XP')
    if policy == 'transferred':
        require(re.search(r'^native-transfer: retained policy uses [1-9]\d*$', trace, re.M)
                and 'native-transfer: score {correct #t ' in trace,
                f'{directory}: no successful live use of the retained policy')
    timings = re.findall(r'^native-combat: task-us (\d+)$', trace, re.M)
    require(len(timings) == 1, f'{directory}: expected one task clock')
    source = installed_client(directory, read_table)
    start = source.index('(module agel/osrs-wire\n')
    end = source.index('(module agel/record-chain\n', start)
    contract = {key: manifest[key] for key in ('image', 'manifest', 'combat_policy_sha256')}
    if goal is not None:
        contract.update({key: manifest[key] for key in (
            'goal_xp', 'training_policy_sha256', 'service_policy_sha256',
            'evaluation_fuel', 'keepalive_interval_us')})
        contract['guest_memory_mib'] = manifest.get('guest_memory_mib', 64)
        pool = re.findall(r'^x86 frame pool ends at (\d+) MiB$', trace, re.M)
        require(len(pool) <= 1, f'{directory}: ambiguous guest frame pool')
        if pool:
            contract['guest_pool_end_mib'] = int(pool[0])
        boundaries = (
            ('stream_adapter', '(module agel/byte-stream\n', '(module agel/osrs-wire\n'),
            ('combat_source', '(module agel/osrs-combat-test\n', '(module agel/osrs-combat-training\n'),
            ('training_source', '(module agel/osrs-combat-training\n', '(module agel/meta\n'),
            ('starting_encoder', '(def initial-policy ', '(import agel/osrs-wire)\n'),
        )
        for key, first, last in boundaries:
            begin = source.index(first)
            stop = source.index(last, begin)
            contract[key] = hashlib.sha256(source[begin:stop].encode()).hexdigest()
    contract['game_adapter'] = hashlib.sha256(source[start:end].encode()).hexdigest()
    contract.update(boot_identity(directory, read_table))
    return contract, identity[1], int(timings[0]), charges


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixed', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--melee', action='store_true', help='compare in-session adaptation on three-skill melee')
    parser.add_argument('--transfer', action='store_true', help='score a melee run using a retained policy')
    args = parser.parse_args()
    if args.transfer and not args.melee:
        parser.error('--transfer applies to --melee; one-rat comparisons already use transfer')
    read_table = runpy.run_path(str(Path(__file__).with_name('install-program.py')))['read_table']
    fixed, fixed_account, fixed_time, fixed_charges = load_arm(args.fixed, 'fixed', read_table)
    transfer, transfer_account, transfer_time, adaptive_charges = load_arm(
        args.candidate, 'adaptive' if args.melee and not args.transfer else 'transferred', read_table)
    require(fixed == transfer, 'world, boot image, Agel executable, game adapter, or combat source differs')
    require(fixed_account != transfer_account, 'combat accounts must differ')
    digest = hashlib.sha256(json.dumps(fixed, sort_keys=True).encode()).hexdigest()
    print("(import agel/benchmark)")
    task = 'three-skill-melee' if args.melee else 'one-rat'
    phase = 'in-session-adaptation' if args.melee and not args.transfer else 'fresh-account-evaluation'
    print(f"(def comparison (dict 'task '{task} 'phase '{phase} "
          f"'contract {form(digest)} 'randomness 'uncontrolled))")
    for arm, elapsed, charges in [('fixed', fixed_time, fixed_charges), ('adaptive', transfer_time, adaptive_charges)]:
        values = ' '.join(f"(dict 'kind '{kind} 'cost-nanousd {cost if cost is not None else 'nil'})"
                          for kind, cost in charges)
        print(f"(def {arm} (dict 'arm '{arm} 'comparison comparison 'account '{arm}-account "
              f"'correct #t 'elapsed-us {elapsed} 'billing-complete #t 'charges (list {values})))")
    print("(benchmark-pair fixed adaptive)")


if __name__ == '__main__':
    main()

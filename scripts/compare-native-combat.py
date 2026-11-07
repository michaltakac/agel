"""Package verified native combat runs as input for Agel's paired scorer."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import sys

sys.path.insert(0, str(Path(__file__).with_name('osrs')))
from test_support import combat_xp, combat_xp_saved, form


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def boot_identity(directory, read_table):
    disk = directory / 'disk.img'
    program = next(row for row in read_table(disk) if row['name'] == 'agel')
    with disk.open('rb') as source:
        boot = hashlib.sha256(source.read(1024 * 512)).hexdigest()
        source.seek(program['start'] * 512)
        executable = hashlib.sha256(source.read(program['length'])).hexdigest()
    return {'boot': boot, 'agel': executable}


def load_arm(directory, policy, read_table):
    manifest = json.loads((directory / 'world.json').read_text())
    trace = (directory / 'serial.log').read_text()
    saved = (directory / 'saved-account.txt').read_text()
    require(manifest['encoder_policy'] == policy, f'{directory}: unexpected policy')
    require('process agel exited with status 0' in trace, f'{directory}: no successful guest exit')
    require(re.search(r'^native-combat: \{test rat-combat passed #t .*target-removed #t\}', trace, re.M),
            f'{directory}: no verified combat report')
    require('inference-bridge: started job' not in trace, f'{directory}: evaluation incurred inference')
    require(combat_xp_saved(combat_xp(trace), saved), f'{directory}: saved XP differs from observations')
    identity = re.search(r'^account=(\d+) character=\d+ rights= x=\d+ z=\d+ level=0 xp_rate_in_hundreds=100$',
                         saved, re.M)
    require(identity, f'{directory}: expected saved normal account at 1× XP')
    if policy == 'transferred':
        require(re.search(r'^native-transfer: retained policy uses [1-9]\d*$', trace, re.M)
                and 'native-transfer: score {correct #t ' in trace,
                f'{directory}: no successful live use of the retained policy')
    timings = re.findall(r'^native-combat: task-us (\d+)$', trace, re.M)
    require(len(timings) == 1, f'{directory}: expected one task clock')
    source = (directory / 'client.agel').read_text()
    start = source.index('(module agel/osrs-wire\n')
    end = source.index('(module agel/record-chain\n', start)
    contract = {key: manifest[key] for key in ('image', 'manifest', 'combat_policy_sha256')}
    contract['game_adapter'] = hashlib.sha256(source[start:end].encode()).hexdigest()
    contract.update(boot_identity(directory, read_table))
    return contract, identity[1], int(timings[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixed', type=Path)
    parser.add_argument('transfer', type=Path)
    args = parser.parse_args()
    read_table = runpy.run_path(str(Path(__file__).with_name('install-program.py')))['read_table']
    fixed, fixed_account, fixed_time = load_arm(args.fixed, 'fixed', read_table)
    transfer, transfer_account, transfer_time = load_arm(args.transfer, 'transferred', read_table)
    require(fixed == transfer, 'world, boot image, Agel executable, game adapter, or combat source differs')
    require(fixed_account != transfer_account, 'combat accounts must differ')
    digest = hashlib.sha256(json.dumps(fixed, sort_keys=True).encode()).hexdigest()
    print("(import agel/benchmark)")
    print(f"(def comparison (dict 'task 'one-rat 'phase 'fresh-account-evaluation "
          f"'contract {form(digest)} 'randomness 'uncontrolled))")
    for arm, elapsed in [('fixed', fixed_time), ('adaptive', transfer_time)]:
        print(f"(def {arm} (dict 'arm '{arm} 'comparison comparison 'account '{arm}-account "
              f"'correct #t 'elapsed-us {elapsed} 'billing-complete #t 'charges nil))")
    print("(benchmark-pair fixed adaptive)")


if __name__ == '__main__':
    main()

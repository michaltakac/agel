"""Package verified native task runs and optimizer receipts for Agel scoring."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import sys
from native_provider_support import (
    judge_cost, judge_frames, judge_receipts_for_trace, verified_judge_ledger, writer_usage_for_trace,
)

sys.path.insert(0, str(Path(__file__).with_name('osrs')))
from test_support import combat_xp, combat_xp_saved, craft_saved, craft_stock, economy_saved, form, melee_failure_saved, melee_levels_saved, melee_xp, quest_saved, quest_unfinished


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def saved_outcome(directory):
    path = directory / 'saved-account.txt'
    require(path.is_file(), f'{directory}: no server outcome was captured; this run cannot be scored')
    return path.read_text()


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


def verify_optimizer_approval(directory, trace, receipts):
    commits = list(re.finditer(r'^native-opt: policy committed after text judgment and measured replay$', trace, re.M))
    probes = list(re.finditer(r'^native-opt: probe \{passed #t .* active 1\}$', trace, re.M))
    require(len(commits) == len(probes) == 1 and probes[0].end() < commits[0].start(),
            f'{directory}: expected one successful measured probe before commit')
    read_table = runpy.run_path(str(Path(__file__).with_name('install-program.py')))['read_table']
    source = installed_client(directory, read_table)
    thresholds = re.findall(r"'max-attempts \d+ 'threshold (\d+) 'proposal-provider 'writer", source)
    require(len(thresholds) == 1 and receipts, f'{directory}: missing optimizer threshold or judge receipt')
    job = re.match(r'^\(judge-result id "[^"]*/(\d+)"', receipts[-1])
    require(job and re.search(rf'^inference-bridge: completed job {job[1]} at-ms \d+ status ok$',
                              trace[:probes[0].start()], re.M),
            f'{directory}: no completed judge dispatch before the measured probe')
    response = re.search(r' status 200 .*? response ("(?:\\.|[^"\\])*")\)$', receipts[-1])
    require(response, f'{directory}: final judge response was not successful')
    answers = json.loads(json.loads(response[1])).get('answers', {})
    patch, reason = answers.get('patch', {}), answers.get('reason', {})
    probability = patch.get('noul')
    require(type(probability) in (float, int) and 0 <= probability <= 1
            and int(probability * 1000 + 0.5) >= int(thresholds[0])
            and patch.get('type') == 'noul' and reason.get('type') == 'choice'
            and reason.get('choice') == 'none',
            f'{directory}: final protected judge receipt did not approve the edit')


def optimizer_charges(directory, trace, *, interrupted=False):
    require((directory / 'mode.txt').read_text().strip() == 'live',
            f'{directory}: adaptive scoring requires live providers')
    ledger, _ = verified_judge_ledger(directory)
    receipts = judge_receipts_for_trace(ledger, trace)
    if 'native-opt: policy committed after text judgment and measured replay' in trace:
        verify_optimizer_approval(directory, trace, receipts)
    charges = [('judge', judge_cost(receipt)) for receipt in receipts]
    writers, usage = writer_usage_for_trace(trace)
    require(writers or interrupted, f'{directory}: no writer dispatch')
    charges.extend(('model', None) for _ in writers)
    return charges, usage


def adaptation_evidence(directory, trace, correct):
    skips = re.findall(r'^native-opt: skipped because combat already completed$', trace, re.M)
    if skips:
        require(len(skips) == 1 and correct and (directory / 'mode.txt').read_text().strip() == 'live'
                and 'native-opt: policy committed' not in trace
                and 'native-combat: {test rat-combat passed #t ' in trace
                and 'inference-bridge: started job ' not in trace
                and re.findall(r'^native-opt: live uses (\d+)$', trace, re.M) == ['0']
                and not judge_frames(directory / 'disk.img'),
                f'{directory}: invalid combat optimization skip evidence')
        return [], 'skipped-completed', []
    activated = 'native-opt: policy committed after text judgment and measured replay' in trace
    rolled_back = 'native-opt: continued with fixed policy after exhausted retries' in trace
    interrupted = not correct and bool(re.search(r'^native-(?:combat|craft|economy|quest): optimizer failure \{', trace, re.M))
    require(activated != rolled_back or (interrupted and not activated and not rolled_back),
            f'{directory}: no verified in-session policy decision')
    uses = re.findall(r'^native-opt: live uses (0|[1-9]\d*)$', trace, re.M)
    require(len(uses) <= 1 and (not rolled_back or not uses or uses == ['0']),
            f'{directory}: inconsistent in-session policy use')
    if correct:
        require('native-opt: recorded workload verified in append-only log' in trace and uses,
                f'{directory}: no verified in-session policy use')
    charges, usage = optimizer_charges(directory, trace, interrupted=interrupted and not (activated or rolled_back))
    decision = 'committed' if activated else 'refused-retained-fixed' if rolled_back else 'interrupted'
    if activated:
        if not uses:
            decision = 'committed-use-unknown'
        elif uses == ['0']:
            decision = 'committed-unused'
    return charges, decision, usage


def load_arm(directory, policy, read_table):
    manifest = json.loads((directory / 'world.json').read_text())
    trace = (directory / 'serial.log').read_text()
    saved = saved_outcome(directory)
    require(manifest['encoder_policy'] == policy, f'{directory}: unexpected policy')
    exits = re.findall(r'^process agel exited with status (\d+)$', trace, re.M)
    require(len(exits) == 1, f'{directory}: expected one terminal guest exit')
    correct = exits[0] == '0'
    goal = manifest.get('goal_xp')
    charges = []
    usage = []
    decision = policy
    if not correct:
        require(re.search(r'^native-combat: (?:optimizer )?failure \{', trace, re.M),
                f'{directory}: no measured combat task failure')
        if goal is None:
            require(combat_xp_saved({0: 0, 3: 1154}, saved, minimum=True),
                    f'{directory}: no saved combat stats')
        else:
            require(policy != 'transferred' and melee_failure_saved(trace, saved),
                    f'{directory}: saved account does not corroborate observed training progress')
    elif goal is None:
        expected = combat_xp(trace)
        require(re.search(r'^native-combat: \{test rat-combat passed #t .*target-removed #t\}', trace, re.M),
                f'{directory}: no verified combat report')
    else:
        expected, levels = melee_xp(trace, goal)
        require(melee_levels_saved(levels, saved), f'{directory}: saved melee levels or stance differ')
    if policy == 'adaptive':
        charges, decision, usage = adaptation_evidence(directory, trace, correct)
    else:
        require('inference-bridge: started job' not in trace, f'{directory}: evaluation incurred inference')
        if policy == 'fixed':
            require(not judge_frames(directory / 'disk.img'),
                    f'{directory}: fixed control inherited a judge journal')
    if correct:
        require(combat_xp_saved(expected, saved, minimum=goal is not None),
                f'{directory}: saved XP does not corroborate observations')
    identity = re.search(r'^account=(\d+) character=\d+ rights= x=\d+ z=\d+ level=0 xp_rate_in_hundreds=100$',
                         saved, re.M)
    require(identity, f'{directory}: expected saved normal account at 1× XP')
    if policy == 'transferred':
        if correct:
            require(re.search(r'^native-transfer: retained policy uses [1-9]\d*$', trace, re.M)
                    and 'native-transfer: score {correct #t ' in trace,
                    f'{directory}: no successful live use of the retained policy')
        else:
            require(re.search(r'^native-transfer: recovered \{version 2 ', trace, re.M),
                    f'{directory}: no verified retained policy before combat failure')
            decision = 'transferred-task-failed'
        if 'donor_judge_frames_sha256' in manifest:
            frames = judge_frames(directory / 'disk.img')
            require(frames and hashlib.sha256(frames).hexdigest() == manifest['donor_judge_frames_sha256'],
                    f'{directory}: transferred judge journal differs from the donor')
    timings = re.findall(r'^native-combat: task-us (\d+)$', trace, re.M)
    require(len(timings) == 1, f'{directory}: expected one task clock')
    source = installed_client(directory, read_table)
    if policy == 'transferred' and 'native-transfer: tape replay ' in source:
        require(re.search(r'^(?:live-desktop> )?native-transfer: tape replay \{bytes \d+ quantum \d+ calls \d+\}$',
                          trace, re.M),
                f'{directory}: no retained workload replay')
    start = source.index('(module agel/osrs-wire\n')
    end = source.index('(module agel/record-chain\n', start)
    contract = {key: manifest[key] for key in ('image', 'manifest', 'combat_policy_sha256')}
    contract['guest_memory_mib'] = manifest.get('guest_memory_mib', 64)
    contract['evaluation_fuel'] = manifest.get('evaluation_fuel', 50_000_000)
    contract['keepalive_interval_us'] = manifest.get('keepalive_interval_us')
    pool = re.findall(r'^x86 frame pool ends at (\d+) MiB$', trace, re.M)
    require(len(pool) <= 1, f'{directory}: ambiguous guest frame pool')
    if pool:
        contract['guest_pool_end_mib'] = int(pool[0])
    if goal is not None:
        contract.update({key: manifest[key] for key in (
            'goal_xp', 'training_policy_sha256', 'service_policy_sha256')})
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
    return contract, identity[1], int(timings[0]), charges, correct, decision, usage


def load_economy(directory, policy, read_table):
    manifest = json.loads((directory / 'world.json').read_text())
    trace = (directory / 'serial.log').read_text()
    saved = saved_outcome(directory)
    require(manifest['encoder_policy'] == policy, f'{directory}: unexpected economy policy')
    exits = re.findall(r'^process agel exited with status (\d+)$', trace, re.M)
    require(len(exits) == 1, f'{directory}: expected one terminal guest exit')
    correct = exits[0] == '0'
    sales = re.findall(r'^native-economy: \{test first-sale passed #t .*coins-before 0 coins-after ([1-9]\d*)\}$', trace, re.M)
    if correct:
        require(len(sales) == 1 and 'native-network: client completed:' in trace,
                f'{directory}: missing verified sale')
        coins = int(sales[0])
        guest_cost = '0' if policy == 'fixed' else 'nil'
        clocks = re.findall(
            rf'^native-economy: score \{{correct #t elapsed-us ([1-9]\d*) .* '
            rf'cost-nanousd {guest_cost} coins {coins} coins-per-dollar nil\}}$', trace, re.M)
    else:
        require(not sales and re.search(r'^native-economy: (?:optimizer )?failure \{', trace, re.M),
                f'{directory}: no measured sale failure')
        coins = 0
        clocks = re.findall(r'^native-economy: task-us (\d+)$', trace, re.M)
    require(len(clocks) == 1 and economy_saved(coins, saved), f'{directory}: sale clock or saved evidence differs')
    identity = re.search(r'^account=(\d+) character=\d+ rights= x=\d+ z=\d+ level=0 xp_rate_in_hundreds=100$',
                         saved, re.M)
    require(identity, f'{directory}: expected saved normal account at 1× XP')
    charges, decision, usage = [], policy, []
    if policy == 'adaptive':
        charges, decision, usage = adaptation_evidence(directory, trace, correct)
    else:
        require('inference-bridge: started job' not in trace, f'{directory}: fixed economy incurred inference')
    contract = {key: manifest[key] for key in (
        'image', 'manifest', 'scope', 'guest_memory_mib', 'evaluation_fuel',
        'keepalive_interval_us', 'economy_policy_sha256', 'service_policy_sha256')}
    source = installed_client(directory, read_table)
    for key, first, last in (
        ('stream_adapter', '(module agel/byte-stream\n', '(module agel/osrs-wire\n'),
        ('game_and_economy', '(module agel/osrs-wire\n', '(module agel/meta\n'),
        ('starting_encoder', '(def initial-policy ', '(import agel/osrs-wire)\n'),
        ('task', '(import agel/osrs-mining-test) (import agel/benchmark)', '(if (< 0 (get completed'),
    ):
        begin = source.index(first)
        body = source[begin:source.index(last, begin)].split('\n(if policy-activated', 1)[0].strip()
        contract[key] = hashlib.sha256(body.encode()).hexdigest()
    contract.update(boot_identity(directory, read_table))
    return contract, identity[1], int(clocks[0]), charges, correct, decision, usage, coins


def load_quest(directory, policy, read_table):
    manifest = json.loads((directory / 'world.json').read_text())
    trace = (directory / 'serial.log').read_text()
    saved = saved_outcome(directory)
    require(manifest['encoder_policy'] == policy, f'{directory}: unexpected quest policy')
    require(manifest['scope'] == "native Cook's Assistant with gathered ingredients on a fresh normal account",
            f'{directory}: quest benchmark requires a fresh gathering run')
    exits = re.findall(r'^process agel exited with status (\d+)$', trace, re.M)
    require(len(exits) == 1, f'{directory}: expected one terminal guest exit')
    correct = exits[0] == '0'
    report = ('native-quest: {test cooks-assistant passed #t choices 2 quest-points 1 '
              'cooking-xp 300 ingredients-after (0 0 0) source gathered}')
    if correct:
        require(report in trace and quest_saved(saved), f'{directory}: quest reward differs from report')
        guest_cost = '0' if policy == 'fixed' else 'nil'
        clocks = re.findall(rf'^native-quest: score \{{correct #t elapsed-us ([1-9]\d*) .* '
                            rf'cost-nanousd {guest_cost} coins nil coins-per-dollar nil\}}$', trace, re.M)
    else:
        require(report not in trace and re.search(r'^native-quest: (?:optimizer )?failure \{', trace, re.M) and quest_unfinished(saved),
                f'{directory}: expected a measured failure with no saved quest reward')
        clocks = re.findall(r'^native-quest: task-us ([1-9]\d*)$', trace, re.M)
    identity = re.search(r'^account=(\d+) character=\d+ rights= x=\d+ z=\d+ level=0 xp_rate_in_hundreds=100$',
                         saved, re.M)
    require(identity and len(clocks) == 1, f'{directory}: expected one task clock and a normal saved account')
    charges, decision, usage = [], policy, []
    if policy == 'adaptive':
        charges, decision, usage = adaptation_evidence(directory, trace, correct)
    else:
        require('inference-bridge: started job' not in trace, f'{directory}: fixed quest incurred inference')

    source = installed_client(directory, read_table)
    common = shared_setup(directory, source)
    start = source.index('(import agel/osrs-quest-test)')
    end = source.index('\n(if policy-activated', start) if policy == 'adaptive' else source.index('\n(wire-close)', start)
    contract = {key: manifest[key] for key in (
        'image', 'manifest', 'scope', 'guest_memory_mib', 'evaluation_fuel',
        'keepalive_interval_us', 'quest_policy_sha256', 'economy_policy_sha256', 'service_policy_sha256')}
    contract.update(boot_identity(directory, read_table))
    contract['common_setup'] = hashlib.sha256(common.encode()).hexdigest()
    contract['task'] = hashlib.sha256(source[start:end].strip().encode()).hexdigest()
    return contract, identity[1], int(clocks[0]), charges, correct, decision, usage


def shared_setup(directory, source):
    common = source[:source.index('(def task-start (clock-us))')]
    common, logins = re.subn(r'^  \(begin \(osrs-login "[^"]+" "[^"]+" context ',
                             '  (begin (osrs-login <fixture> context ', common, flags=re.M)
    require(logins == 1, f'{directory}: expected one account login in installed program')
    return common


def load_craft(directory, policy, read_table):
    manifest = json.loads((directory / 'world.json').read_text())
    trace = (directory / 'serial.log').read_text()
    require(manifest['encoder_policy'] == policy, f'{directory}: unexpected crafting policy')
    exits = re.findall(r'^process agel exited with status (\d+)$', trace, re.M)
    require(len(exits) == 1, f'{directory}: expected one terminal guest exit')
    correct = exits[0] == '0'
    before = craft_stock((directory / 'before-account.txt').read_text())
    saved = saved_outcome(directory)
    clocks = re.findall(r'^native-craft: task-us ([1-9]\d*)$', trace, re.M)
    require(before is not None and len(clocks) == 1, f'{directory}: missing crafting stock or task clock')
    if correct:
        require('native-network: client completed: login, observations and adamant platebody' in trace
                and craft_saved(trace, before, craft_stock(saved, finished=True)),
                f'{directory}: crafting report and saved result differ')
    else:
        require(re.search(r'^native-craft: (?:optimizer )?failure \{', trace, re.M)
                and 'native-craft: {test adamant-platebody passed #t ' not in trace
                and craft_stock(saved, at_anvil=False) == before,
                f'{directory}: failure did not leave the original stock and XP intact')

    charges, decision, usage = [], policy, []
    if policy == 'adaptive':
        charges, decision, usage = adaptation_evidence(directory, trace, correct)
    else:
        require('inference-bridge: started job' not in trace, f'{directory}: fixed crafting incurred inference')
    source = installed_client(directory, read_table)
    common = shared_setup(directory, source)
    start = source.index('(import agel/osrs-crafting-test)')
    end = source.index('\n(if (< 0 (get completed', start)
    task = source[start:end].split('\n(if policy-activated', 1)[0].strip()
    contract = {key: manifest[key] for key in (
        'image', 'manifest', 'scope', 'guest_memory_mib', 'evaluation_fuel',
        'keepalive_interval_us', 'craft_policy_sha256', 'service_policy_sha256')}
    contract.update(boot_identity(directory, read_table))
    contract['initial_smithing_fine_xp'] = before[1]
    contract['common_setup'] = hashlib.sha256(common.encode()).hexdigest()
    contract['task'] = hashlib.sha256(task.encode()).hexdigest()
    return contract, before[0], int(clocks[0]), charges, correct, decision, usage


def charge_forms(charges):
    return ' '.join(f"(dict 'kind '{kind} 'cost-nanousd {cost if cost is not None else 'nil'})"
                    for kind, cost in charges)


def writer_forms(usage):
    return ' '.join('nil' if call is None else
                    f"(dict 'input {call[0]} 'cached-input {call[1] if call[1] is not None else 'nil'} 'output {call[2]})"
                    for call in usage)


def writer_estimate_form(usage, rates):
    inputs, cached, outputs = rates
    return (f"(benchmark-writer-estimate (list {writer_forms(usage)}) "
            f"(dict 'input {inputs} 'cached-input {cached} 'output {outputs}))")


def optimizer_score(directory):
    trace = (directory / 'serial.log').read_text()
    require(len(re.findall(r'^process agel exited with status \d+$', trace, re.M)) == 1,
            f'{directory}: expected a terminal guest')
    activated = 'native-opt: policy committed after text judgment and measured replay' in trace
    refused = 'native-opt: last refusal ' in trace
    require(activated != refused, f'{directory}: no resolved optimizer decision')
    charges, usage = optimizer_charges(directory, trace)
    clocks = re.findall(r'^native-opt: task-us (\d+)$', trace, re.M)
    require(len(clocks) <= 1, f'{directory}: ambiguous optimizer clock')
    if clocks:
        elapsed, clock = int(clocks[0]), 'guest-optimization'
    else:
        started = re.findall(r'^inference-bridge: started job (\d+) provider (?:codex|jev) at-ms (\d+)$', trace, re.M)
        ended = re.findall(r'^inference-bridge: completed job (\d+) at-ms (\d+) status \w+$', trace, re.M)
        require(started and [job for job, _ in started] == [job for job, _ in ended],
                f'{directory}: legacy inference window is incomplete')
        require(all(int(stop) >= int(begin) for (_, begin), (_, stop) in zip(started, ended)),
                f'{directory}: legacy inference clock went backwards')
        elapsed = (int(ended[-1][1]) - int(started[0][1])) * 1000
        clock = 'bridge-inference-window'
    print("(import agel/benchmark)")
    print(f"(dict 'task 'encoder-optimization 'clock '{clock} "
          f"'decision '{'committed' if activated else 'refused'} "
          f"'score (benchmark-run (dict 'correct {'#t' if activated else '#f'} "
          f"'elapsed-us {elapsed} 'billing-complete #t 'charges (list {charge_forms(charges)}))) "
          f"'writer-usage (benchmark-writer-usage (list {writer_forms(usage)})))")


def verify_transfer_donor(donor, recipient, evaluation_accounts):
    origin = json.loads((donor / 'world.json').read_text())
    target = json.loads((recipient / 'world.json').read_text())
    require(origin.get('encoder_policy') == 'adaptive' and target.get('encoder_policy') == 'transferred',
            'expected an adaptive donor and a transferred recipient')
    for key in ('image', 'manifest', 'combat_policy_sha256', 'goal_xp'):
        require(origin.get(key) == target.get(key), f'donor differs from recipient on {key}')
    with (donor / 'disk.img').open('rb') as image:
        digest = hashlib.file_digest(image, 'sha256').hexdigest()
    require(target.get('donor_disk_sha256') == digest, 'recipient names a different donor disk')
    _, frames = verified_judge_ledger(donor)
    require(frames and frames == judge_frames(recipient / 'disk.img'),
            'recipient does not retain the donor judge journal')
    trace = (donor / 'serial.log').read_text()
    saved = saved_outcome(donor)
    identity = re.search(r'^account=(\d+) character=\d+ rights= x=\d+ z=\d+ level=0 xp_rate_in_hundreds=100$',
                         saved, re.M)
    require(identity and identity[1] not in evaluation_accounts,
            'donor must be a distinct normal account')
    require(re.findall(r'^process agel exited with status (\d+)$', trace, re.M) == ['0'],
            'donor combat did not complete')
    goal = origin.get('goal_xp')
    if goal is None:
        xp = combat_xp(trace)
        require(re.search(r'^native-combat: \{test rat-combat passed #t .*target-removed #t\}$', trace, re.M),
                'donor has no successful combat report')
    else:
        xp, levels = melee_xp(trace, goal)
        require(melee_levels_saved(levels, saved), 'donor melee levels differ from its report')
    require(combat_xp_saved(xp, saved, minimum=goal is not None), 'donor XP differs from its report')
    charges, decision, usage = adaptation_evidence(donor, trace, True)
    clocks = re.findall(r'^native-combat: task-us ([1-9]\d*)$', trace, re.M)
    require(decision == 'committed' and len(clocks) == 1, 'donor has no timed committed policy')
    return int(clocks[0]), charges, usage


def transfer_recovery_time(directory, read_table):
    source = installed_client(directory, read_table)
    if '"native-transfer: recovery-us "' not in source:
        return None
    trace = (directory / 'serial.log').read_text()
    receipts = re.findall(
        r'^native-transfer: recovered \{[^\n]+\}\n'
        r'native-transfer: recovery-us ([1-9]\d*)$', trace, re.M)
    require(len(receipts) == 1, f'{directory}: expected one recovery clock after lineage verification')
    return int(receipts[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixed', type=Path)
    parser.add_argument('candidate', type=Path, nargs='?')
    tasks = parser.add_mutually_exclusive_group()
    tasks.add_argument('--combat', action='store_true', help='compare in-session one-rat combat adaptation')
    tasks.add_argument('--melee', action='store_true', help='compare in-session adaptation on three-skill melee')
    tasks.add_argument('--economy', action='store_true', help='compare in-session adaptation on the first bronze sale')
    tasks.add_argument('--craft', action='store_true', help='compare measured adamant platebody runs')
    tasks.add_argument('--quest', action='store_true', help='compare fresh Cook’s Assistant runs')
    tasks.add_argument('--optimizer', action='store_true', help='score the optimizer decision independently of the game')
    parser.add_argument('--transfer', action='store_true', help='score a melee run using a retained policy')
    parser.add_argument('--donor', type=Path, help='verify and separately score the original transfer donor')
    parser.add_argument('--writer-rates', type=int, nargs=3, metavar=('INPUT', 'CACHED', 'OUTPUT'),
                        help='append writer estimates using supplied nanodollars per token')
    args = parser.parse_args()
    if args.writer_rates is not None:
        inputs, cached, outputs = args.writer_rates
        if not (0 <= cached <= inputs <= 2**63 - 1 and 0 <= outputs <= 2**63 - 1):
            parser.error('--writer-rates requires nonnegative signed 64-bit rates with CACHED <= INPUT')
    if args.transfer and not args.melee:
        parser.error('--transfer applies to --melee; one-rat comparisons already use transfer')
    if args.optimizer:
        if args.candidate or args.donor or args.writer_rates is not None:
            parser.error('--optimizer takes one run directory; writer estimates require paired scoring')
        optimizer_score(args.fixed)
        return
    if args.candidate is None:
        parser.error('paired scoring requires two run directories')
    read_table = runpy.run_path(str(Path(__file__).with_name('install-program.py')))['read_table']
    load = load_economy if args.economy else load_craft if args.craft else load_quest if args.quest else load_arm
    fixed, fixed_account, fixed_time, fixed_charges, fixed_correct, fixed_decision, _, *fixed_coins = load(args.fixed, 'fixed', read_table)
    adaptive_policy = 'adaptive' if (args.combat or args.economy or args.melee or args.craft or args.quest) and not args.transfer else 'transferred'
    transfer, transfer_account, transfer_time, adaptive_charges, adaptive_correct, adaptive_decision, usage, *adaptive_coins = load(
        args.candidate, adaptive_policy, read_table)
    if args.combat:
        require('goal_xp' not in fixed and 'goal_xp' not in transfer, '--combat requires one-rat arms')
    require(fixed == transfer, 'world, runtime, game adapter, task source, or starting state differs')
    require(fixed_account != transfer_account, 'task accounts must differ')
    donor_score = None
    if args.donor:
        require(adaptive_policy == 'transferred', '--donor requires a transferred policy')
        donor_score = verify_transfer_donor(args.donor, args.candidate, {fixed_account, transfer_account})
    digest = hashlib.sha256(json.dumps(fixed, sort_keys=True).encode()).hexdigest()
    print("(import agel/benchmark)")
    task = 'first-bronze-sale' if args.economy else 'adamant-platebody' if args.craft else 'cooks-assistant' if args.quest else 'three-skill-melee' if args.melee else 'one-rat'
    phase = 'in-session-adaptation' if adaptive_policy == 'adaptive' else 'fresh-account-evaluation'
    print(f"(def comparison (dict 'task '{task} 'phase '{phase} "
          f"'contract {form(digest)} 'randomness 'uncontrolled))")
    for arm, elapsed, charges, correct, coins in [('fixed', fixed_time, fixed_charges, fixed_correct, fixed_coins),
        ('adaptive', transfer_time, adaptive_charges, adaptive_correct, adaptive_coins)]:
        values = charge_forms(charges)
        proceeds = f" 'coins-before 0 'coins-after {coins[0]}" if coins else ''
        print(f"(def {arm} (dict 'arm '{arm} 'comparison comparison 'account '{arm}-account "
              f"'correct {'#t' if correct else '#f'} 'elapsed-us {elapsed} 'billing-complete #t 'charges (list {values}){proceeds}))")
    pair = (f"(assoc (assoc (benchmark-pair fixed adaptive) 'policy-outcomes "
            f"(dict 'fixed '{fixed_decision} 'adaptive '{adaptive_decision})) "
            f"'writer-usage (benchmark-writer-usage (list {writer_forms(usage)})))")
    if args.writer_rates is not None:
        pair = f"(assoc {pair} 'writer-estimate {writer_estimate_form(usage, args.writer_rates)})"
    if donor_score:
        elapsed, charges, donor_usage = donor_score
        donor_estimate = (f" 'writer-estimate {writer_estimate_form(donor_usage, args.writer_rates)}"
                          if args.writer_rates is not None else '')
        pair = (f"(assoc {pair} 'donor-learning (dict "
                f"'score (benchmark-run (dict 'correct #t 'elapsed-us {elapsed} "
                f"'billing-complete #t 'charges (list {charge_forms(charges)}))) "
                f"'writer-usage (benchmark-writer-usage (list {writer_forms(donor_usage)})){donor_estimate}))")
    if adaptive_policy == 'transferred':
        recovery = transfer_recovery_time(args.candidate, read_table)
        if recovery is not None:
            pair = f"(benchmark-transfer-time {pair} {recovery})"
    print(pair)



if __name__ == '__main__':
    main()

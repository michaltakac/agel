"""Independent protocol-test helpers, not the game client or agent policy."""
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import time

CLIENT = "target/release/examples/osrs"


def form(value):
    if isinstance(value, (list, tuple, bytes)):
        return "(" + " ".join(map(form, value)) + ")"
    if isinstance(value, str):
        return json.dumps(value)
    return str(value)


def evaluate(source, port=1, timeout=120, *, progress=None):
    with tempfile.TemporaryDirectory(prefix="agel-login-", dir="target") as directory:
        path = Path(directory) / "probe.agel"
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as output:
            output.write(source)
        command = [CLIENT, str(port), str(path)]
        if progress is None:
            return subprocess.run(command, capture_output=True, text=True, timeout=timeout)
        stderr_path = Path(directory) / "stderr.log"
        with stderr_path.open("w") as stderr, stderr_path.open() as reader, subprocess.Popen(
            command, stdout=subprocess.PIPE, stderr=stderr, text=True
        ) as child:
            deadline = time.monotonic() + timeout
            errors = []
            try:
                while True:
                    output = None
                    try:
                        output, _ = child.communicate(timeout=max(0, min(1, deadline - time.monotonic())))
                    except subprocess.TimeoutExpired:
                        if time.monotonic() >= deadline:
                            raise
                    for line in reader:
                        errors.append(line)
                        if line.startswith("agel-progress: "):
                            progress(line.removeprefix("agel-progress: ").rstrip("\n"))
                    if output is not None:
                        return subprocess.CompletedProcess(command, child.returncode, output, "".join(errors))
            finally:
                if child.poll() is None:
                    child.kill()


def xtea(data, key, decrypt=False):
    data = bytearray(data)
    mask = (1 << 32) - 1
    for at in range(0, len(data) - 7, 8):
        a = int.from_bytes(data[at:at + 4], "big")
        b = int.from_bytes(data[at + 4:at + 8], "big")
        total = (0x9e3779b9 * 32) & mask if decrypt else 0
        for _ in range(32):
            if decrypt:
                b = (b - (((((a << 4) ^ (a >> 5)) + a) & mask) ^ ((total + key[(total >> 11) & 3]) & mask))) & mask
                total = (total - 0x9e3779b9) & mask
                a = (a - (((((b << 4) ^ (b >> 5)) + b) & mask) ^ ((total + key[total & 3]) & mask))) & mask
            else:
                a = (a + (((((b << 4) ^ (b >> 5)) + b) & mask) ^ ((total + key[total & 3]) & mask))) & mask
                total = (total + 0x9e3779b9) & mask
                b = (b + (((((a << 4) ^ (a >> 5)) + a) & mask) ^ ((total + key[(total >> 11) & 3]) & mask))) & mask
        data[at:at + 8] = a.to_bytes(4, "big") + b.to_bytes(4, "big")
    return bytes(data)


def combat_xp(report):
    """Return whole XP from the Agel combat report and require fresh stats."""
    match = re.search(
        r'attack-before (\d+) attack-after (\d+) hitpoints-before (\d+) hitpoints-after (\d+)',
        report,
    )
    assert match, report
    before_attack, after_attack, before_hp, after_hp = map(int, match.groups())
    assert before_attack == 0 and before_hp == 1154, report
    assert after_attack > before_attack and after_hp > before_hp, report
    return {0: after_attack, 3: after_hp}


def combat_xp_saved(expected, oracle, *, minimum=False):
    """Compare whole guest XP with server tenths, optionally as a lower bound."""
    stats = {int(stat): int(xp) for stat, xp in re.findall(
        r'^stat=(\d+) base=\d+ visible=\d+ fine_xp=(\d+)$', oracle, re.M)}
    if minimum:
        return all(stats.get(stat, -1) >= xp * 10 for stat, xp in expected.items())
    return all(stats.get(stat, -1) // 10 == xp for stat, xp in expected.items())


def craft_stock(snapshot, *, finished=False, at_anvil=True):
    header = re.search(
        r'^account=(\d+) character=\d+ rights= x=(\d+) z=(\d+) level=0 xp_rate_in_hundreds=100$',
        snapshot, re.M)
    xp = re.search(r'^stat=13 base=88 visible=88 fine_xp=(\d+)$', snapshot, re.M)
    inventory = sorted(line for line in snapshot.splitlines() if line.startswith('inv='))
    product = 'adamant_platebody count=1' if finished else 'adamantite_bar count=5'
    if header and xp and inventory == [f'inv=inv.inv item=obj.{product}', 'inv=inv.inv item=obj.hammer count=1']:
        if finished or not at_anvil or header.groups()[1:] == ('3187', '3425'):
            return int(header[1]), int(xp[1])
    return None


def smelt_stock(snapshot, count, *, finished=False, at_furnace=True):
    header = re.search(
        r'^account=(\d+) character=\d+ rights= x=(\d+) z=(\d+) level=0 xp_rate_in_hundreds=100$',
        snapshot, re.M)
    xp = re.search(r'^stat=13 base=88 visible=88 fine_xp=(\d+)$', snapshot, re.M)
    items = re.findall(r'^inv=inv\.inv item=obj\.([a-z0-9_]+) count=(\d+)$', snapshot, re.M)
    if not header or not xp or len(items) != sum(line.startswith('inv=') for line in snapshot.splitlines()):
        return None
    amounts = {}
    for name, quantity in items:
        amounts[name] = amounts.get(name, 0) + int(quantity)
    expected = {'adamantite_bar': count} if finished else {'adamantite_ore': count, 'coal': 6 * count}
    if amounts == expected and (finished or not at_furnace or header.groups()[1:] == ('3226', '3255')):
        return int(header[1]), int(xp[1])
    return None


def smelt_saved(report, count, before, after):
    observed = re.search(
        rf'\{{test adamant-bar passed #t ore-before {count} ore-after 0 coal-before {6 * count} coal-after 0 '
        rf'bars-before 0 bars-after {count} smithing-xp-before (\d+) smithing-xp-after (\d+)\}}',
        report)
    return (before is not None and after is not None and observed is not None
            and before[0] == after[0] and after[1] - before[1] == 375 * count
            and tuple(map(int, observed.groups())) == (before[1] // 10, after[1] // 10))


def craft_saved(report, before, after):
    observed = re.search(
        r'^native-craft: \{test adamant-platebody passed #t bars-before 5 bars-after 0 '
        r'platebody-before 0 platebody-after 1 smithing-xp-before (\d+) smithing-xp-after (\d+)\}$',
        report, re.M)
    return (before is not None and after is not None and observed is not None
            and before[0] == after[0] and after[1] - before[1] == 3125
            and tuple(map(int, observed.groups())) == (before[1] // 10, after[1] // 10))

def economy_saved(coins, snapshot):
    if coins == 0:
        return 'item=obj.coins ' not in snapshot
    lines = snapshot.splitlines()
    inventory = {line for line in lines if line.startswith('inv=')}
    return (inventory == {f'inv=inv.inv item=obj.coins count={coins}',
                          'inv=inv.inv item=obj.bronze_pickaxe count=1'}
            and {'stat=13 base=1 visible=1 fine_xp=62',
                 'stat=14 base=1 visible=1 fine_xp=350'}.issubset(lines))


def quest_saved(snapshot):
    varps = {key: int(value) for key, value in re.findall(r'^(varp\.\w+)=(-?\d+)$', snapshot, re.M)}
    return (varps.get('varp.cookquest', 0) % 4 == 2 and varps.get('varp.qp') == 1
            and varps.get('varp.cooks_assistant_ingredients', 0) % 8 == 7
            and 'stat=7 base=4 visible=4 fine_xp=3000' in snapshot.splitlines()
            and all(f'item=obj.{item} ' not in snapshot for item in ('bucket_milk', 'egg', 'pot_flour')))


def quest_unfinished(snapshot):
    lines = snapshot.splitlines()
    progress = re.search(r'^varp\.cookquest=(-?\d+)$', snapshot, re.M)
    return ('stat=7 base=1 visible=1 fine_xp=0' in lines
            and (not progress or int(progress[1]) % 4 != 2)
            and all(line == 'varp.qp=0' for line in lines if line.startswith('varp.qp=')))


def melee_xp(report, goal):
    """Check the guest's three completed phases against its final observation."""
    summary = re.search(
        r'\{test rat-melee passed #t goal-xp (\d+) stance (?:[0-3]|nil) attack-xp (\d+) '
        r'strength-xp (\d+) defence-xp (\d+) hitpoints-xp (\d+)(?: phases |\})', report)
    if not summary or int(summary[1]) != goal:
        raise ValueError('missing melee result at the requested XP goal')
    expected = {0: int(summary[2]), 2: int(summary[3]), 1: int(summary[4]), 3: int(summary[5])}
    phases = re.findall(
        r'\{skill (\d+) xp-before 0 xp-after (\d+) level (\d+) fights (\d+) encounters ', report)
    if len(phases) != 3 or {int(skill) for skill, *_ in phases} != {0, 1, 2}:
        raise ValueError('missing or duplicate fresh melee phase')
    if expected[3] <= 1154 or any(
        not goal <= int(xp) <= expected[int(skill)]
        or not 1 <= int(fights) <= min(64, (goal + 7) // 8 + 3)
        for skill, xp, _, fights in phases
    ):
        raise ValueError('melee phase disagrees with the final observation')
    return expected, {int(skill): int(level) for skill, _, level, _ in phases}


def melee_levels_saved(levels, oracle):
    return 'varp.com_mode=3' in oracle.splitlines() and all(
        (match := re.search(rf'^stat={skill} base=(\d+) visible=\1 ', oracle, re.M))
        and int(match[1]) >= level
        for skill, level in levels.items()
    )


def melee_failure_saved(trace, oracle):
    saved = {int(skill): int(xp) for skill, xp in re.findall(
        r'^stat=([012]) base=\d+ visible=\d+ fine_xp=(\d+)$', oracle, re.M)}
    observed = re.findall(
        r'^native-combat: encounter \(([012]) \{[^\n]*? (?:attack-after|xp-after) (\d+) ',
        trace, re.M)
    return (len(saved) == 3
            and all(saved[int(skill)] >= int(xp) * 10 for skill, xp in observed))

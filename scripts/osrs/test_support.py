"""Independent protocol-test helpers, not the game client or agent policy."""
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile

CLIENT = "target/release/examples/osrs"


def form(value):
    if isinstance(value, (list, tuple, bytes)):
        return "(" + " ".join(map(form, value)) + ")"
    if isinstance(value, str):
        return json.dumps(value)
    return str(value)


def evaluate(source, port=1, timeout=120):
    with tempfile.TemporaryDirectory(prefix="agel-login-", dir="target") as directory:
        path = Path(directory) / "probe.agel"
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as output:
            output.write(source)
        return subprocess.run([CLIENT, str(port), str(path)], capture_output=True, text=True, timeout=timeout)


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


def economy_saved(coins, snapshot):
    if coins == 0:
        return 'item=obj.coins ' not in snapshot
    return {f'inv=inv.inv item=obj.coins count={coins}',
            'inv=inv.inv item=obj.bronze_pickaxe count=1',
            'stat=13 base=1 visible=1 fine_xp=62',
            'stat=14 base=1 visible=1 fine_xp=350'}.issubset(snapshot.splitlines())


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


def melee_failure_saved(trace, goal, oracle):
    saved = {int(skill): int(xp) for skill, xp in re.findall(
        r'^stat=([012]) base=\d+ visible=\d+ fine_xp=(\d+)$', oracle, re.M)}
    observed = re.findall(
        r'^native-combat: encounter \(([012]) \{[^\n]*? (?:attack-after|xp-after) (\d+) ',
        trace, re.M)
    return (len(saved) == 3 and min(saved.values()) < goal * 10
            and all(saved[int(skill)] >= int(xp) * 10 for skill, xp in observed))

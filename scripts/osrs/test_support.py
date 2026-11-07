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


def evaluate(source, port=1):
    with tempfile.TemporaryDirectory(prefix="agel-login-", dir="target") as directory:
        path = Path(directory) / "probe.agel"
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as output:
            output.write(source)
        return subprocess.run([CLIENT, str(port), str(path)], capture_output=True, text=True, timeout=120)


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


def combat_xp_saved(expected, oracle):
    """The protocol reports whole XP and the oracle reports tenths."""
    stats = {int(stat): int(xp) for stat, xp in re.findall(
        r'^stat=(\d+) base=\d+ visible=\d+ fine_xp=(\d+)$', oracle, re.M)}
    return all(stats.get(stat, -1) // 10 == xp for stat, xp in expected.items())

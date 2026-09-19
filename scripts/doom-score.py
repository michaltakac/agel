"""Score a recorded DOOM episode: `python3 scripts/doom-score.py steps.jsonl`.

Reads the dataset `agel-play` writes — one record a step with the engine's
state line before the step and the keys the program held — and prints what
can be read off it without judgment: steps, distance travelled, kills,
health and ammo at the end, how often the player did not move, and which
key sets were held. One run is one run; no significance is claimed."""
import json
import re
import sys
from collections import Counter

FIELDS = re.compile(r"x (-?\d+) y (-?\d+) angle (-?\d+) health (\d+) armor (\d+) ammo (\d+) kills (\d+)")


def score(path):
    rows = [json.loads(line) for line in open(path) if line.strip()]
    states = [FIELDS.search(row.get("state", "")) for row in rows]
    states = [tuple(int(v) for v in m.groups()) for m in states if m]
    distance = sum(abs(b[0] - a[0]) + abs(b[1] - a[1]) for a, b in zip(states, states[1:]))
    still = sum(1 for a, b in zip(states, states[1:]) if a[0] == b[0] and a[1] == b[1])
    held = Counter(row.get("keys", "") for row in rows)
    last = states[-1] if states else None
    first = states[0] if states else None
    print(f"{path}: {len(rows)} steps, {len(states)} with a state line")
    if last:
        print(f"  distance {distance} map units, still {still} steps, "
              f"kills {last[6]}, health {first[3]} -> {last[3]}, ammo {first[5]} -> {last[5]}")
    print("  held: " + ", ".join(f"{keys} x{n}" for keys, n in held.most_common()))
    if any(row.get("judgment") for row in rows):
        good = [row["good"] for row in rows if "good" in row]
        print(f"  judged good: mean {sum(good) // max(1, len(good))} thousandths over {len(good)} steps")


for path in sys.argv[1:]:
    score(path)

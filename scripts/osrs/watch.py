#!/usr/bin/env python3
"""Mirror test status into a running Agel OS graphical console."""
import argparse
import json
from pathlib import Path
import re
import time
import urllib.parse
import urllib.request


def native_status(trace):
    xp = dict.fromkeys(("Attack", "Strength", "Defence"), 0)
    names = {"0": "Attack", "2": "Strength", "1": "Defence"}
    milestone = "booting native guest"
    for line in trace.splitlines():
        if line.startswith("native-combat: {test rat-melee passed #t "):
            final = re.search(r" attack-xp (\d+) strength-xp (\d+) defence-xp (\d+) ", line)
            if final:
                milestone = " | ".join(f"{name} {value} XP" for name, value in zip(xp, final.groups()))
        elif line.startswith("native-combat: encounter "):
            encounter = re.match(r"native-combat: encounter \(([012]) .*? (?:attack-after|xp-after) (\d+) ", line)
            if encounter:
                xp[names[encounter[1]]] = int(encounter[2])
                milestone = " | ".join(f"{name} {value} XP" for name, value in xp.items())
        elif line.startswith(("native-quest: ", "native-economy: ", "native-combat: training ")):
            milestone = line.split(": ", 1)[1]
        elif line.startswith("native-opt: continued with fixed policy"):
            milestone = "retained fixed policy after refused edits"
        elif line.startswith("native-opt: policy committed"):
            milestone = "activated a judged and replayed policy"
    if "process agel exited with status 0" in trace:
        return "Guest completed | " + milestone
    if (re.search(r"^process agel exited with status [1-9]\d*$", trace, re.M)
            or re.search(r'^Error: "guest (?:run exceeded|console closed)', trace, re.M)):
        return "Guest stopped | " + milestone
    return milestone


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("viewer", help="Local graphical-console URL")
    parser.add_argument("--native-log", type=Path, help="Mirror milestones from a native run's serial log")
    args = parser.parse_args()
    url = urllib.parse.urlparse(args.viewer)
    if url.scheme != "http" or url.hostname != "127.0.0.1":
        raise SystemExit("Use the local Agel graphical-console URL")
    endpoint = args.viewer.rstrip("/") + "/input"
    origin = f"{url.scheme}://{url.netloc}"
    source = args.native_log or Path("target/osrs-live-os/status.txt")
    previous = None
    while True:
        text = source.read_text(errors="replace").strip() if source.exists() else ""
        text = native_status(text) if args.native_log else text or "RuneScape tests: waiting for results"
        if text != previous:
            visible = " ".join(text.split())
            command = "(def runescape-status " + json.dumps(visible, ensure_ascii=True) + ")"
            while len(command.encode()) > 240:
                visible = visible[:-5]
                command = "(def runescape-status " + json.dumps(visible, ensure_ascii=True) + ")"
            request = urllib.request.Request(endpoint, data=json.dumps({"source": command}).encode(),
                headers={"Origin": origin, "Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(request, timeout=40) as reply:
                    answer = json.load(reply)
                if "error" in answer:
                    raise RuntimeError(answer["error"])
                previous = text
                print(visible, flush=True)
            except Exception as problem:
                print("OS status bridge:", problem, flush=True)
        time.sleep(2)


if __name__ == "__main__":
    main()

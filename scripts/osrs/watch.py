#!/usr/bin/env python3
"""Mirror test status into a running Agel OS graphical console."""
import argparse
import json
from pathlib import Path
import time
import urllib.parse
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("viewer", help="Local graphical-console URL")
args = parser.parse_args()
url = urllib.parse.urlparse(args.viewer)
if url.scheme != "http" or url.hostname != "127.0.0.1":
    raise SystemExit("Use the local Agel graphical-console URL")
endpoint = args.viewer.rstrip("/") + "/input"
origin = f"{url.scheme}://{url.netloc}"
source = Path("target/osrs-live-os/status.txt")
previous = None
while True:
    text = source.read_text().strip() if source.exists() else "RuneScape tests: waiting for results"
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

#!/usr/bin/env python3
"""Record the running world's image, cache identity and baseline checks."""
import json
import subprocess


def output(*args):
    return subprocess.check_output(args, text=True)


container = json.loads(output("docker", "inspect", "agel-osrs-world"))[0]
if not container["State"]["Running"]:
    raise SystemExit("World is not running")
ports = container["NetworkSettings"]["Ports"]
if ports != {"43594/tcp": [{"HostIp": "127.0.0.1", "HostPort": "43594"}]}:
    raise SystemExit("Unexpected published ports: " + repr(ports))
program = """
import hashlib,json
from pathlib import Path
root=Path('/world/.data/cache/SERVER')
if not root.is_dir(): raise SystemExit('server cache missing')
digest=hashlib.sha256()
files=0
for p in sorted(root.rglob('*')):
 if p.is_file():
  digest.update(str(p.relative_to(root)).encode()+b'\\0')
  digest.update(hashlib.sha256(p.read_bytes()).digest())
  files+=1
if files==0: raise SystemExit('empty cache')
manifest=json.loads(Path('/world/manifest.json').read_text())
manifest['cache_tree_sha256']=digest.hexdigest()
manifest['cache_files']=files
config=Path('/world/game.yml').read_text()
if 'mode: "respect-progress"' not in config: raise SystemExit('quest bypass enabled')
print(json.dumps(manifest))
"""
manifest = json.loads(output("docker", "exec", "agel-osrs-world", "python3", "-c", program))
logs = subprocess.run(["docker", "logs", "agel-osrs-world"], capture_output=True, text=True, check=True)
profiles = [line for line in (logs.stdout + logs.stderr).splitlines() if "Loaded world 255 configuration:" in line]
if not profiles:
    raise SystemExit("No loaded realm configuration")
profile = profiles[-1]
if not all(value in profile for value in ["baseXpRate=1.0,", "globalXpRate=1.0,", "devMode=false,"]):
    raise SystemExit("Restart with the baseline before experiments: " + profile)
print(json.dumps({"image": container["Image"], "ports": ports, "manifest": manifest,
                  "loaded_profile": profile, "scope": "pre-login transport; no gameplay score"}, indent=2))

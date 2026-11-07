#!/usr/bin/env python3
"""Verify a fresh normal account gets a pickaxe and mines copper."""
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).with_name("osrs")))
from test_support import evaluate, form

WORLD = "agel-osrs-world"


def run(*args):
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout


def notice(message):
    print(message, flush=True)
    status = Path("target/osrs-live-os/status.txt")
    if status.parent.exists():
        status.write_text(message)


username = "ore" + secrets.token_hex(4)
password = secrets.token_hex(12)
key = run("docker", "exec", WORLD, "cat", "/world/.data/client.key")
assert key.split("Exponent: ")[1].splitlines()[0].strip() == "10001"
modulus = key.split("Modulus: ")[1].strip()
scenario = Path("examples/osrs/mining-test.agel").read_text()
source = f"""(begin
  (import agel/osrs-wire) (import agel/osrs-login)
  (import agel/osrs-crypto) (import agel/osrs-game)
  (osrs-js5-open 241 '(1 2 3 4))
  (def crcs (osrs-cache-crcs (osrs-js5-group 255 255))) (wire-close)
  (def context (rsa-context {form(modulus)}))
  (def seed (osrs-random-seed)) (def game (osrs-game-state seed))
  (osrs-login {form(username)} {form(password)} context crcs seed (entropy-read 24))
  {scenario}
  (import agel/osrs-mining-test)
  (mine-first-copper game))"""
notice("RuneScape mining: fresh account walking to the tutor for a pickaxe and first copper ore")
started = time.monotonic()
result = evaluate(source, 43594)
assert password not in result.stdout + result.stderr
assert result.returncode == 0, result.stderr
assert "test first-copper passed #t" in result.stdout, result.stdout
print(result.stdout.strip(), flush=True)

compiler = Path(os.environ.get("JAVA_HOME", "")) / "bin/javac"
if not compiler.is_file():
    compiler = Path("/opt/homebrew/opt/openjdk@17/bin/javac")
build = Path("target/osrs-admin")
build.mkdir(exist_ok=True)
run(str(compiler) if compiler.is_file() else "javac", "-d", str(build), "scripts/osrs/AccountState.java")
run("docker", "cp", str(build / "AccountState.class"), f"{WORLD}:/tmp/AccountState.class")
for _ in range(45):
    oracle = subprocess.run(["docker", "exec", WORLD, "sh", "-c",
        'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" AccountState "$1"', "probe", username],
        capture_output=True, text=True)
    snapshot = oracle.stdout
    if oracle.returncode == 0 and "stat=14 base=1 visible=1 fine_xp=175" in snapshot:
        break
    time.sleep(1)
else:
    raise AssertionError("Mining XP did not persist")
assert "rights= " in snapshot and "xp_rate_in_hundreds=100" in snapshot, snapshot
assert "inv=inv.inv item=obj.copper_ore count=1\n" in snapshot, snapshot
assert "inv=inv.inv item=obj.bronze_pickaxe count=1\n" in snapshot, snapshot
for line in snapshot.splitlines():
    if line.startswith(("stat=14 ", "inv=inv.inv item=obj.copper_ore ", "inv=inv.inv item=obj.bronze_pickaxe ")):
        print(line)
print(f"Elapsed seconds: {time.monotonic() - started:.3f}", flush=True)
notice("RuneScape PASS: fresh normal account earned a tutor pickaxe and mined copper with 17.5 XP saved")

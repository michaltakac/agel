#!/usr/bin/env python3
"""Verify fresh-account mining and optional bronze smelting."""
import argparse
import os
import re
from pathlib import Path
import secrets
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).with_name("osrs")))
from test_support import evaluate, form

WORLD = "agel-osrs-world"
parser = argparse.ArgumentParser(description=__doc__)
mode = parser.add_mutually_exclusive_group()
mode.add_argument("--bronze", action="store_true", help="mine both ores and smelt one bronze bar")
mode.add_argument("--sell", action="store_true", help="smelt and sell the bar at the Lumbridge General Store")
options = parser.parse_args()
production = options.bronze or options.sell


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
chain = Path("crates/agel-stdlib/record-chain.agel").read_text()
task, action = (
    ("first-sale", "sell-first-bronze") if options.sell else
    ("first-bronze", "smelt-first-bronze") if production else
    ("first-copper", "mine-first-copper")
)
source = f"""(begin
  (import agel/osrs-wire) (import agel/osrs-login)
  (import agel/osrs-crypto) (import agel/osrs-game)
  (osrs-js5-open 241 '(1 2 3 4))
  (def crcs (osrs-cache-crcs (osrs-js5-group 255 255))) (wire-close)
  (def context (rsa-context {form(modulus)}))
  (def seed (osrs-random-seed)) (def game (osrs-game-state seed))
  (osrs-login {form(username)} {form(password)} context crcs seed (entropy-read 24))
  {chain}
  (import agel/record-chain)
  (def game (assoc game 'history-push record-chain-push))
  {scenario}
  (import agel/osrs-mining-test)
  ({action} game))"""
notice("RuneScape mining: fresh account walking to the tutor for a pickaxe and "
       + ("ores for a bronze bar to sell" if options.sell else "ores for a bronze bar" if production else "first copper ore"))
started = time.monotonic()
result = evaluate(source, 43594, timeout=300 if options.sell else 240 if production else 120)
assert password not in result.stdout + result.stderr
assert result.returncode == 0, result.stderr
assert f"test {task} passed #t" in result.stdout, result.stdout
print(result.stdout.strip(), flush=True)

compiler = Path(os.environ.get("JAVA_HOME", "")) / "bin/javac"
if not compiler.is_file():
    compiler = Path("/opt/homebrew/opt/openjdk@17/bin/javac")
build = Path("target/osrs-admin")
build.mkdir(exist_ok=True)
run(str(compiler) if compiler.is_file() else "javac", "-d", str(build), "scripts/osrs/AccountState.java")
run("docker", "cp", str(build / "AccountState.class"), f"{WORLD}:/tmp/AccountState.class")
mining = "stat=14 base=1 visible=1 fine_xp=" + ("350" if production else "175")
smithing = "stat=13 base=1 visible=1 fine_xp=62"
if options.sell:
    coins = re.search(r"coins-after ([1-9][0-9]*)", result.stdout)
    assert coins, result.stdout
    proceeds = int(coins[1])
    outcome = f"inv=inv.inv item=obj.coins count={proceeds}\n"
elif production:
    outcome = "inv=inv.inv item=obj.bronze_bar count=1\n"
else:
    outcome = "inv=inv.inv item=obj.copper_ore count=1\n"
for _ in range(45):
    oracle = subprocess.run(["docker", "exec", WORLD, "sh", "-c",
        'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" AccountState "$1"', "probe", username],
        capture_output=True, text=True)
    snapshot = oracle.stdout
    if oracle.returncode == 0 and outcome in snapshot and mining + "\n" in snapshot and (
        not production or smithing + "\n" in snapshot
    ):
        break
    time.sleep(1)
else:
    raise AssertionError("Expected XP and inventory did not persist")
assert "rights= " in snapshot and "xp_rate_in_hundreds=100" in snapshot, snapshot
assert "inv=inv.inv item=obj.bronze_pickaxe count=1\n" in snapshot, snapshot
if options.sell:
    assert "item=obj.bronze_bar " not in snapshot, snapshot
if production:
    assert "item=obj.copper_ore " not in snapshot and "item=obj.tin_ore " not in snapshot, snapshot
for line in snapshot.splitlines():
    if line.startswith(("stat=13 ", "stat=14 ", "inv=inv.inv item=obj.copper_ore ",
                        "inv=inv.inv item=obj.bronze_pickaxe ", "inv=inv.inv item=obj.bronze_bar ",
                        "inv=inv.inv item=obj.coins ")):
        print(line)
print(f"Elapsed seconds: {time.monotonic() - started:.3f}", flush=True)
if options.sell:
    notice(f"RuneScape PASS: fresh account mined, smelted, and sold a bronze bar for {proceeds} saved coins")
elif production:
    notice("RuneScape PASS: fresh account mined both ores and smelted one bronze bar at normal rights")
else:
    notice("RuneScape PASS: fresh normal account earned a tutor pickaxe and mined copper with 17.5 XP saved")

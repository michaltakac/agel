#!/usr/bin/env python3
"""Verify the normal-account quest with staged ingredients or acquire containers on a fresh account."""
import argparse
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).with_name("osrs")))
from test_support import evaluate, form

WORLD = "agel-osrs-world"
ACCOUNT = "aq" + secrets.token_hex(4)
password = secrets.token_hex(12)
parser = argparse.ArgumentParser(description=__doc__)
mode = parser.add_mutually_exclusive_group()
mode.add_argument("--containers", action="store_true", help="acquire the empty pot and bucket through ordinary gameplay")
mode.add_argument("--gather", action="store_true", help="complete Cook's Assistant with ingredients acquired through gameplay")
options = parser.parse_args()
VARPS = ("varp.cookquest", "varp.qp", "varp.cooks_assistant_ingredients")

def run(*args):
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout


def oracle_program(program, *args):
    return run("docker", "exec", WORLD, "sh", "-c",
               'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" "$@"', "probe", program, *args)


def notice(message):
    print(message, flush=True)
    status = Path("target/osrs-live-os/status.txt")
    if status.parent.exists():
        status.write_text(message)


compiler = Path(os.environ.get("JAVA_HOME", "")) / "bin/javac"
if not compiler.is_file():
    compiler = Path("/opt/homebrew/opt/openjdk@17/bin/javac")
build = Path("target/osrs-admin")
build.mkdir(exist_ok=True)
run(str(compiler) if compiler.is_file() else "javac", "-d", str(build),
    "scripts/osrs/AccountState.java", "scripts/osrs/SetupAccount.java")
for name in ("AccountState", "SetupAccount"):
    run("docker", "cp", str(build / (name + ".class")), f"{WORLD}:/tmp/{name}.class")

key = run("docker", "exec", WORLD, "cat", "/world/.data/client.key")
assert key.split("Exponent: ")[1].splitlines()[0].strip() == "10001"
modulus = key.split("Modulus: ")[1].strip()
scenario = Path("examples/osrs/quest-test.agel").read_text()
mining = Path("examples/osrs/mining-test.agel").read_text()
chain = Path("crates/agel-stdlib/record-chain.agel").read_text()


def execute(action=None):
    source = f"""(begin
      (import agel/osrs-wire) (import agel/osrs-login)
      (import agel/osrs-crypto) (import agel/osrs-game)
      (osrs-js5-open 241 '(1 2 3 4))
      (def crcs (osrs-cache-crcs (osrs-js5-group 255 255))) (wire-close)
      (def context (rsa-context {form(modulus)}))
      (def seed (osrs-random-seed)) (def game (osrs-game-state seed))
      (osrs-login {form(ACCOUNT)} {form(password)} context crcs seed (entropy-read 24))
      {chain}
      (import agel/record-chain)
      (def game (assoc game 'history-push record-chain-push))
      {mining}
      {scenario}
      (import agel/osrs-quest-test)
      {action or '#t'}
    )"""
    for _ in range(12):
        result = evaluate(source, 43594, timeout=650 if options.gather else 300 if options.containers else 120)
        assert password not in result.stdout + result.stderr
        if result.returncode == 0:
            return result.stdout.strip()
        if "osrs/login-refused" not in result.stderr or "data: Int(5)" not in result.stderr:
            raise AssertionError(result.stderr)
        time.sleep(1)
    raise AssertionError("previous session did not retire")


def await_saved(predicate):
    for _ in range(45):
        try:
            snapshot = oracle_program("AccountState", ACCOUNT, *VARPS)
        except subprocess.CalledProcessError:
            time.sleep(1)
            continue
        if predicate(snapshot):
            return snapshot
        time.sleep(1)
    raise AssertionError("expected state did not persist")


def inventory(snapshot, name):
    matches = re.findall(rf"^inv=inv\.inv item=obj\.{re.escape(name)} count=(\d+)$", snapshot, re.M)
    return sum(map(int, matches))


def varp(snapshot, key):
    match = re.search(rf"^{re.escape(key)}=(-?\d+)$", snapshot, re.M)
    return int(match[1]) if match else 0


items = ("bucket_milk", "egg", "pot_flour")
execute("""(begin (osrs-await game (fn (g)
  (if (osrs-stat g 7) (has-key? (get g 'inventories) 93) #f))) #t)""")
initial = await_saved(lambda s: "stat=7 base=1 visible=1 fine_xp=0" in s)
assert "rights= " in initial and "xp_rate_in_hundreds=100" in initial, initial
assert all(inventory(initial, name) == 0 for name in items), initial
assert all(varp(initial, key) == 0 for key in VARPS), initial
assert "stat=7 base=1 visible=1 fine_xp=0" in initial, initial

if not options.containers and not options.gather:
    notice("RuneScape quest: staging one milk bucket, egg, and pot of flour")
    oracle_program("SetupAccount", "quest-grant", ACCOUNT)
    try:
        execute("(begin (stage-cook-ingredients game) #t)")
        staged = await_saved(lambda s: all(inventory(s, name) == 1 for name in items))
        assert all(varp(staged, key) == 0 for key in VARPS), staged
    finally:
        oracle_program("SetupAccount", "quest-revoke", ACCOUNT)

normal = oracle_program("AccountState", ACCOUNT, *VARPS)
assert "rights= " in normal and "xp_rate_in_hundreds=100" in normal, normal
started = time.monotonic()
if options.containers:
    notice("RuneScape quest: obtaining the pot and earning coins for the bucket without admin setup")
    report = execute("(acquire-cook-containers game)")
    assert ("test cook-containers passed #t pot 1 bucket 1 coins-earned 3 coins-left 1 "
            "pot-on-ground-before 1 ") in report, report
    after = await_saved(lambda s: all(inventory(s, name) == 1 for name in ("pot_empty", "bucket_empty", "coins")))
    assert "rights= " in after and "xp_rate_in_hundreds=100" in after, after
    assert all(varp(after, key) == 0 for key in VARPS), after
    print(report)
    print("Saved supplies: one empty pot, one empty bucket, and one coin at normal rights and 1× XP")
    print(f"Acquisition and verification elapsed seconds: {time.monotonic() - started:.3f}")
    notice("RuneScape PASS: fresh normal account picked up a pot and bought a bucket with earned coins")
    raise SystemExit(0)

if options.gather:
    notice("RuneScape quest: gathering all ingredients and completing Cook's Assistant without admin setup")
    report = execute("(gather-and-finish-cooks-assistant game)")
    assert "source gathered" in report, report
else:
    notice("RuneScape quest: accepting and finishing Cook's Assistant under normal rights")
    report = execute("(finish-cooks-assistant game)")
assert "test cooks-assistant passed #t" in report, report
after = await_saved(lambda s: varp(s, "varp.cookquest") % 4 == 2
                    and varp(s, "varp.qp") == 1
                    and varp(s, "varp.cooks_assistant_ingredients") % 8 == 7
                    and "stat=7 base=4 visible=4 fine_xp=3000" in s
                    and all(inventory(s, name) == 0 for name in items))
assert "rights= " in after and "xp_rate_in_hundreds=100" in after, after
print(report)
for line in after.splitlines():
    if line.startswith(("stat=7 ", *VARPS)):
        print(line)
print("Saved Cook's Assistant state: stage 2, all three hand-ins, one quest point and 300 Cooking XP")
print(f"Quest and verification elapsed seconds: {time.monotonic() - started:.3f}")
notice("RuneScape PASS: Cook's Assistant completion and reward saved at normal rights")

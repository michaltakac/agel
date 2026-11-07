#!/usr/bin/env python3
"""Stage materials separately, then verify normal-account crafting on the private world."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).with_name("osrs")))
from test_support import evaluate, form, smelt_saved, smelt_stock

WORLD = "agel-osrs-world"
parser = argparse.ArgumentParser(description=__doc__)
mode = parser.add_mutually_exclusive_group()
mode.add_argument("--native", action="store_true", help="run the crafting phase inside Agel OS")
mode.add_argument("--bank", action="store_true", help="deposit and withdraw staged ore at Varrock West bank")
mode.add_argument("--smelt", action="store_true", help="smelt adamant bars from staged ore and coal")
parser.add_argument("--smelt-count", type=int, choices=range(1, 4), default=1)
mode.add_argument("--prepare-only", action="store_true", help="stage stock and verify normal rights, without crafting")
parser.add_argument("--fixture", default="agelcraft", help="dedicated account, agelcraft or ac followed by eight hex digits")
options = parser.parse_args()
if not options.smelt and options.smelt_count != 1:
    parser.error("--smelt-count requires --smelt")
COUNT = options.smelt_count
ACCOUNT = options.fixture
if ACCOUNT != "agelcraft" and not re.fullmatch(r"ac[0-9a-f]{8}", ACCOUNT):
    parser.error("fixture must be agelcraft or ac followed by eight hex digits")
fixture_args = [] if ACCOUNT == "agelcraft" else [ACCOUNT]


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

directory = Path("target/osrs-login-probes")
directory.mkdir(mode=0o700, exist_ok=True)
fixture_lock = (directory / (ACCOUNT + ".lock")).open("a")
fcntl.flock(fixture_lock, fcntl.LOCK_EX)
credentials = directory / (ACCOUNT + ".json")
if not credentials.exists():
    fd = os.open(credentials, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as handle:
        json.dump({"password": secrets.token_hex(12)}, handle)
password = json.loads(credentials.read_text())["password"]
key = run("docker", "exec", WORLD, "cat", "/world/.data/client.key")
assert key.split("Exponent: ")[1].splitlines()[0].strip() == "10001"
modulus = key.split("Modulus: ")[1].strip()
scenario = Path("examples/osrs/crafting-test.agel").read_text()
if options.bank:
    scenario += "\n" + Path("examples/osrs/osrs-bank.agel").read_text()


def execute(action=None):
    source = f"""(begin
      (import agel/osrs-wire) (import agel/osrs-login)
      (import agel/osrs-crypto) (import agel/osrs-game)
      (osrs-js5-open 241 '(1 2 3 4))
      (def crcs (osrs-cache-crcs (osrs-js5-group 255 255))) (wire-close)
      (def context (rsa-context {form(modulus)}))
      (def seed (osrs-random-seed)) (def game (osrs-game-state seed))
      (osrs-login {form(ACCOUNT)} {form(password)} context crcs seed (entropy-read 24))
      {scenario}
      (import agel/osrs-crafting-test)
      {action or '#t'}
    )"""
    for _ in range(12):
        result = evaluate(source, 43594)
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
            snapshot = oracle_program("AccountState", ACCOUNT)
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


def xp(snapshot):
    return int(re.search(r"^stat=13 base=88 visible=88 fine_xp=(\d+)$", snapshot, re.M)[1])


try:
    oracle_program("AccountState", ACCOUNT)
except subprocess.CalledProcessError:
    execute()
    await_saved(lambda _: True)

notice("RuneScape crafting: staging ore and coal" if options.smelt or options.bank else
       "RuneScape crafting: staging one hammer and five bars on the dedicated fixture")
oracle_program("SetupAccount", "craft-grant", *fixture_args)
try:
    if options.bank:
        execute("""(begin
          (def stock (stage-adamant-ore game))
          (osrs-wait-ticks (osrs-command stock "tele 3185 3436") 4)
          #t)""")
        await_saved(lambda s: "x=3185 z=3436 level=0" in s and inventory(s, "adamantite_ore") == 1)
    elif options.smelt:
        execute(f"(begin (stage-adamant-ore-batch game {COUNT}) #t)")
        await_saved(lambda s: "x=3226 z=3255 level=0" in s
                    and inventory(s, "adamantite_ore") == COUNT and inventory(s, "coal") == 6 * COUNT)
    else:
        execute("(begin (stage-adamant-stock game) #t)")
        before = await_saved(lambda s: "x=3187 z=3425 level=0" in s
                         and "stat=13 base=88 visible=88" in s
                         and inventory(s, "hammer") == 1
                         and inventory(s, "adamantite_bar") == 5
                         and inventory(s, "adamant_platebody") == 0)
finally:
    oracle_program("SetupAccount", "craft-revoke", *fixture_args)

normal = oracle_program("AccountState", ACCOUNT)
assert "rights= " in normal and "xp_rate_in_hundreds=100" in normal, normal
if options.prepare_only:
    notice(f"RuneScape PASS: {ACCOUNT} has five bars, a hammer, level 88 Smithing, and normal rights")
    sys.exit(0)
if options.bank:
    before_stock = smelt_stock(normal, 1, at_furnace=False)
    assert before_stock is not None, normal
    report = execute("""(begin
      (import agel/osrs-bank)
      (def bank (osrs-bank-open game osrs-bank-layout-241 10583 3186 3438))
      (def stored (osrs-bank-deposit bank osrs-bank-layout-241 449 1))
      (def returned (osrs-bank-withdraw stored osrs-bank-layout-241 449 1))
      (def closed (osrs-bank-close returned osrs-bank-layout-241))
      (list (osrs-inventory-count stored 95 449) (osrs-inventory-count closed 93 449)))""")
    assert report == "(1 1)", report
    await_saved(lambda s: smelt_stock(s, 1, at_furnace=False) == before_stock)
    notice("RuneScape PASS: deposited and withdrew adamantite ore with normal rights")
    sys.exit(0)
if options.smelt:
    notice("RuneScape crafting: normal-account furnace and adamant bar")
    before_stock = smelt_stock(normal, COUNT)
    assert before_stock is not None, normal
    report = execute(f"(get (smelt-adamant-bars game {COUNT}) 'report)")
    print(report)
    after = await_saved(lambda s: smelt_stock(s, COUNT, finished=True) is not None)
    after_stock = smelt_stock(after, COUNT, finished=True)
    assert smelt_saved(report, COUNT, before_stock, after_stock), (report, before_stock, after_stock)
    print(f"Saved Smithing fine XP: {before_stock[1]} -> {after_stock[1]}")
    notice(f"RuneScape PASS: {COUNT} ore and {6 * COUNT} coal consumed, {COUNT} adamant bars saved at normal rights and 1x XP")
    sys.exit(0)
notice("RuneScape crafting: normal-account anvil and platebody button")
if options.native:
    command = ["sh", "scripts/test-native-network.sh", "--craft", "--craft-fixture", ACCOUNT]
    with subprocess.Popen(command, stdout=subprocess.PIPE, text=True) as child:
        lines = []
        for line in child.stdout:
            print(line, end="", flush=True)
            lines.append(line)
        status = child.wait()
        if status:
            raise subprocess.CalledProcessError(status, command)
    report = "".join(lines)
else:
    report = execute("(forge-adamant-platebody game)")
    print(report)
assert "test adamant-platebody passed #t" in report, report
after = await_saved(lambda s: inventory(s, "adamant_platebody") == 1
                    and inventory(s, "adamantite_bar") == 0
                    and inventory(s, "hammer") == 1)
assert "rights= " in after and "xp_rate_in_hundreds=100" in after, after
saved_before, saved_after = xp(before), xp(after)
assert saved_after - saved_before == 3125, (saved_before, saved_after)
observed = re.search(r"smithing-xp-before (\d+) smithing-xp-after (\d+)", report)
assert observed and tuple(map(int, observed.groups())) == (saved_before // 10, saved_after // 10), report
print(f"Saved Smithing fine XP: {saved_before} -> {saved_after}")
print("Saved inventory: five bars consumed, one hammer retained, one adamant platebody produced")
notice("RuneScape PASS: adamant platebody forged through the anvil interface, saved at normal rights and 1x XP")

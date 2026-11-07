# RuneScape tests and live viewing

The hosted Agel client can log in to the private revision-241 world, decode
its game frames, observe skills and inventories, walk, and issue administrator
commands. The gameplay assertions are in `examples/osrs/game-tests.agel`.
NPC discovery, tracking and attack are also Agel. The combat scenario is in
`examples/osrs/combat-test.agel`.
Rust supplies bounded loopback streams, entropy bytes and the existing seed
interpreter. Packet formats, RSA/XTEA/ISAAC, snapshots, action history and
the smoke-test logic are Agel. These optional modules are loaded by the
`osrs` example rather than installed into every default Agel world.

## Native client

`sh scripts/test-native-network.sh` runs cache negotiation, password login,
live skills/inventory observations and a walk inside Agel OS. A separate
server oracle checks the saved coordinates, normal rights and 1× XP.
`--challenge` runs only cache and login challenge negotiation. The TCP relay
and buffer/service APIs are [general byte-stream mechanisms](byte-stream.md).
The client is headless; a native visual RuneScape renderer is not included.
See [the native transcript and limits](release-v0.3.16.md).

Reusable standard algorithms now live in `agel/crypto`; load `crypto.agel`
before the older `osrs-crypto.agel` compatibility module. Recompute opaque RSA
contexts when upgrading from 16-bit to 31-bit limbs. For slower transports,
the game state can supply a `maintenance` map with `start` and `stop` service
functions. `osrs-await` and `osrs-wait-ticks` then maintain the encrypted
no-timeout sequence and restore its outgoing cipher position before returning.
Low-level `osrs-next-packet` remains a direct read operation. The supplied native
harness connects these functions to `buffered-stream`'s maintenance service.

## Start or update the private world

The supplied environment targets Linux ARM64 containers, tested from macOS
ARM64. It needs Python 3.12+, Docker, Git, a Java launcher for Gradle and the
Rust toolchain. Gradle uses its managed Java 21 toolchain; this machine uses
Homebrew Java 17 to launch it.

```sh
python3 scripts/osrs/prepare.py
sh scripts/osrs/world.sh start
sh scripts/osrs/world.sh logs
```

Wait for `Server ready`. Before creating experimental accounts, apply the
1× baseline and restart:

```sh
sh scripts/osrs/baseline.sh
sh scripts/osrs/world.sh stop
sh scripts/osrs/world.sh resume
python3 scripts/osrs/world_manifest.py
```

For later code-only changes, reuse the installed cache and replace the
container. The named PostgreSQL volume survives replacement:

```sh
python3 scripts/osrs/prepare.py --reuse-cache
sh scripts/osrs/world.sh replace
```

`start` refuses an existing container name. `stop`/`resume` keep the same
container. None of these commands removes a database volume. Source and
patches are pinned; the manifest records the running image and cache hash.
Generated cache data, RSA keys, private probe credentials and the container
build context stay in `target/` and are not public release assets.

Only `127.0.0.1:43594` is published. The bundled account service's wildcard
listeners remain inside the container. The private profile disables login
proof-of-work; password verification, RSA, XTEA and ISAAC remain active.
This setting must be identical for both future benchmark arms and declared
in their manifests. Public-server proof-of-work challenges are not supported
by this client yet.

The baseline sets realm 255 to 1× base/global XP, disables automatic admin
grants and uses saved quest progress. Other modifiers and individual
mechanics still require parity checks; successful smoke tests are not proof
of complete OSRS parity.

## Run the working tests

```sh
sh scripts/test-osrs-game.sh
sh scripts/test-osrs-combat.sh
```

This runs the Agel scenarios and checks authoritative persisted server state:

- The dedicated `agelsetup` account clears its inventory, changes Smithing
  from 1 to 70 and receives 100 coins. Assertions wait for actual stat and
  inventory packets, rather than assuming a command succeeded.
- A fresh normal account attempts those admin commands, retains its original
  stats/items, and walks from the Lumbridge spawn to `(3223,3218)`.
- The read-only server oracle verifies its saved position, normal rights
  and 1× XP, and the setup account's saved skill and coins.

Only `agelsetup` receives admin rights through the explicit setup helper.
Experimental accounts retain normal rights. This is a permission-isolation
and mechanics smoke test, not the fixed-policy Exo comparison. The Java
helpers operate on the external server for setup and independent verification;
the game client and test assertions do not use them for gameplay decisions.

Probe credentials are retained in private files under
`target/osrs-login-probes/`. Do not publish that directory. The client prints
only the final Agel form's result, avoiding intermediate credential bindings.
Connection closure currently uses the server's disconnect grace period;
the harness reports and bounds duplicate-session retries.

Other checks:

```sh
sh scripts/test-osrs-wire.sh --live
sh scripts/test-osrs-login.sh
sh scripts/test-osrs-login.sh --live
```

These cover fragmented streams, revision refusal, EOF, JS5 blocks, RSA and
XTEA oracles, 600-output ISAAC sequences including refill, encrypted login
layout, wrong passwords, reconnect and persistence.

## Watch in an Agel OS window

Boot the existing graphical OS with its console, then pass the printed local
URL to the watcher:

```sh
python3 scripts/graphical_console.py target/boot/agel-v1.img --snapshot
# In a second terminal, substitute the URL printed above:
python3 scripts/osrs/watch.py VIEWER_URL
```

The viewer shows the real QEMU framebuffer. The watcher copies the gameplay
runner's status text into a `runescape-status` binding evaluated by the guest.
It does not render RuneScape and does not move the hosted client into the OS.
Native client tests run in a separate headless VM. The full game scene renderer remains unfinished. The current
OS image can be built with `scripts/build-boot.sh --features native-graphics`
if no graphical image exists yet.

## Current boundary

There is a rat-combat smoke test, but no maximum-stats, crafting, quest,
money-making or transfer score yet. The rat test requires exactly 8 Attack
XP, increased Hitpoints XP and removal of the selected observed NPC index,
then checks saved XP on the server. It does not read a separate kill counter.
v0.3.17 connects a retained repair actor to polling inference and the native
client. It repairs a seeded planner and executes five queued game actions on
the same connection. Full gameplay policy, durable episode history and paired
cost/time task evaluation remain open. The DOOM demonstration still exists until its replacement is ready.

The client implements password login against the explicit private profile,
not proof-of-work, MFA, token login or reconnect packets. It starts a fresh
login after a socket disconnect. RSA uses the server's exponent 65537 and
moduli up to 1024 bits. Login strings currently require ASCII. Cache groups
remain bounded and are not decompressed or CRC-verified. Game packet
framing covers the pinned revision's descriptors; semantic observations
cover skills, inventories and NPC spawn identities/tracked indices. NPC
movement bitcodes are retained, but coordinates are not continuously
reconstructed. Extended masks, health/death animations, transformations and
player movement still need semantic decoding. Combat currently uses the
root world only. The client retains its observed packets
and actions as Agel state; durable native OS history remains a later stage.


The current game adapter keeps packet byte fields in its retained `history`
as the lossless `agel/bytes` tagged hex representation. Use
`bytes-unpack-field(event, 'bytes)` to obtain the raw list; it also reads old
raw history entries. The packet returned directly by `osrs-next-packet` still
has its raw byte list. No observations are pruned by this packing step.
Outgoing and incoming opcode ciphers use the general ISAAC byte buffers.
Maintenance owns the updated outgoing context, and stopping it transfers that
context back before the next game action. It no longer prepares a long
keepalive schedule while the connection is idle.


Use `sh scripts/test-native-network.sh --repair` for the delayed-provider
fixture, or `--repair --live` for real Codex/Jev providers. The native runner
uses `agel/services` and `agel/service-stream` for serialized I/O state.
The repair actor still uses the bootstrap actor mechanism. Real providers
require the existing Codex login and `TYPESAFEAI_API_KEY`. The server-saved
position, normal rights and 1× XP remain mandatory completion checks.

Use `sh scripts/test-native-network.sh --optimize` to profile byte encoding
during a live native game session, or add `--live` for real model providers.
The private run directory retains the recorded input and judge ledger. The
generic profiler and repair gate take Agel callbacks; game code provides
recorded bytes and independently checked actions. See
[the native optimization evidence](release-v0.3.18.md).

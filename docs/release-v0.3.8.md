# v0.3.8 — Working RuneScape tests in Agel

`sh scripts/test-osrs-game.sh` now runs real gameplay smoke tests against
the private world. Agel logs in, reads encrypted server packets, walks,
observes skills/inventory and verifies administrator setup commands.

The dedicated setup account changes Smithing from 1 to 70 and inventory
coins from 0 to 100. A fresh normal account cannot use those commands and
walks to `(3223,3218)`. Assertions inspect actual game packets. A separate
read-only server oracle confirms the saved position, rights, skill, coins
and 1× XP rate. See the [game results](evidence/v0.3.8-game.txt) and
[world manifest](evidence/v0.3.8-world.json).

## Agel client and test code

RSA modular arithmetic, XTEA, ISAAC, login layout, cache CRC fields, game
packet framing, skill/inventory decoding, action history and test assertions
are Agel. The client handles the pinned revision's fixed, byte-length and
short-length packet frames, including extended opcodes and encrypted URL
payloads. Observations are currently skills and inventories; other game
packets remain recorded bytes.

The Rust example provides bounded loopback sockets and entropy bytes around
the existing interpreter. Agel chooses when to open/close a stream. It prints
only the final form's result, avoiding intermediate credential bindings.
The optional client modules do not change the default seed or kernel ABI.

The server patch adds administrator-only `setlevel skill level`. Existing
item/clear commands remain the inventory authority. Only `agelsetup` is
granted staff rights; experimental accounts remain normal players. This is
an external Kotlin server, not newly introduced Agel OS policy.

## Validation and fixes found through gameplay

- RSA tests compare 1024-bit boundary cases with Python modular exponentiation.
- XTEA tests cover a known vector, random blocks and unencrypted tails.
- ISAAC matches two 600-output sequences from the pinned server reference,
  including unsigned words and buffer refill.
- An independent login fixture decrypts the Agel packet with a private test
  key, checks XTEA content/CRC order and rejects truncated replies.
- Live tests cover new-account login, wrong-password rejection, fresh login
  after disconnect and persisted character state.
- Live gameplay verifies movement, setup commands, normal-account authority
  and persistence. Empty inventory handling is exercised by fresh accounts.
- Wire regressions, carrier Clippy, formatting and Python syntax checks pass.

The first game frames exposed a protocol detail absent from the pre-login
test: this RSProt version advertises 37 login bytes but writes 34 before
starting the game header. The client now preserves that boundary. Gameplay
also exposed an empty-key-list edge in inventory counting; the Agel client
handles empty inventory maps explicitly. Setup tests now wait for observed
state changes instead of assuming a fixed number of ticks is sufficient.

## Private profile and watching progress

The private container disables login proof-of-work for both future experiment
arms. Password authentication, RSA/XTEA/ISAAC and game mechanics remain
active. The profile is explicit in the manifest; it is not a public OSRS
login reproduction. Only game port `127.0.0.1:43594` is published.

`prepare.py --reuse-cache` rebuilds server code without regenerating an
installed cache. Container replacement preserves its PostgreSQL volume.
The manifest command is now `scripts/osrs/world_manifest.py`; the earlier
name collided with Python's standard `inspect` module in test helpers.

A live Agel OS console can display test status through `scripts/osrs/watch.py`.
It is a real QEMU framebuffer with a guest-evaluated status binding. The
game client still runs hosted; this is not a RuneScape renderer or native
OS networking. See [setup and live viewing](osrs-client.md).

## Remaining work

These are smoke tests, not Exo benchmarks. Combat, crafting, quests,
money-making, transfer, model-driven policies and paired fixed-policy cost/
time comparisons remain unfinished. The normal-account test is an authority
check, not that benchmark control.

Public login proof-of-work, MFA, token/reconnect protocols, native transport,
full semantic world observations and a game renderer remain open. Packet
and action history currently lives in Agel state; durable OS game history
and the agent loop still need integration. Guardian-only logging, interrupted
judge billing reconciliation and actual OS service version/revert activation
remain in the [release programme](exo-programme.md). DOOM has not yet been
replaced, and this release makes no claim of beating Exo.

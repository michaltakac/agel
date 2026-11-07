# v0.3.16: The Agel client plays through its own OS

The headless Agel client now negotiates the private world's cache, computes
RSA/XTEA/ISAAC, logs in, observes skills and inventory, and walks while running
inside Agel OS. The independent server oracle confirms that the movement was
saved. This is native gameplay over a general external TCP byte carrier; the
server remains the pinned private OpenRune world.

The [native transcript](evidence/v0.3.16-native-client.txt) records Smithing
level 1, zero coins, 31 encrypted keepalives, a successful process exit, and
saved coordinates `(3223, 3218, 0)` with normal rights and 1× XP. The verified
VM/broker/client/oracle segment took 139.251 seconds, excluding builds and
preparation. The VM remains 64 MiB and the evaluation limit 50 million steps.
The [world manifest](evidence/v0.3.16-world.json) records image, server/cache
identity, normal rates, quest behavior and the private login profile.

Run `sh scripts/test-native-network.sh`, or add `--challenge` for the smaller
cache/challenge test. Each run retains its private disk, generated account,
source, transport log, world manifest and saved-account oracle output under
`target/native-network-TIMESTAMP/`. Credentials are private test fixtures and
are checked for accidental serial output. Public evidence contains no password,
private key, account file or disk image.

## General mechanisms

`agel/byte-stream` owns byte encoding, framing, bounds and chunking.
`agel/buffered-stream` owns a real Agel agent's buffer, refill policy, EOF/failure
state and optional before-read service. Its compact buffer avoids retaining
large byte vectors in every actor-history copy. Small reads reuse fetched data.
See [the byte-stream API and transport contract](byte-stream.md).

The game adapter installs a standard encrypted no-timeout service during long
observations. Agel precomputes an opaque byte schedule, keeps a small service
counter, and restores the outgoing cipher position before the next action.
Rust does not know which bytes are heartbeats. The pinned server's game idle
supplier is 15 seconds; no server timeout or XP rule was relaxed. A previous
trial decoded buffered observations after losing its connection, and its walk
was not saved. The independent oracle caught that failure; completion now
requires the saved position, not just a transmitted packet.

Reusable RSA, XTEA and ISAAC routines live in `agel/crypto`; the older
`agel/osrs-crypto` module is a compatibility adapter. RSA uses 31-bit limbs
instead of 16-bit limbs, reducing work while keeping multiply/accumulate
values within checked signed 64-bit arithmetic. Results match independent
Python modular exponentiation at 1024 bits and across 31-bit limb boundaries.
Existing XTEA vectors/tails and two 600-draw ISAAC sequences also pass.
RSA public exponent 65537 and the existing 1–128-byte modulus bound remain.
Opaque RSA contexts must be recomputed after upgrading; they now identify
their radix and byte width. Optional-module loaders must load `crypto.agel`
before its compatibility adapter.

Rust adds bounded console byte forwarding, a capability for it, and the
TCP/entropy carrier. Bootstrap heap bookkeeping now coalesces free page runs,
stores run metadata in the free pages instead of a fixed table that could
lose runs, and reclaims page-sized class blocks for larger requests. It also
accounts for pages mapped before a partial `brk` failure. Unit probes cover
more than 64 free runs, alignment, splitting, live gaps and tail reclamation.
These changes are bootstrap mechanics, not game/learning policy. The Rust
seed evaluator, scheduler and substantial OS implementations still exist.

The [development trials](evidence/v0.3.16-development-trials.txt) retain EOF,
allocation and independent-oracle failures. One slow unbuffered trial was
deliberately canceled while live and is labeled incomplete. These fixes were
authored during repository development, not autonomously by the inner repair
agent. This smoke test invokes no model provider and makes no all-in cost claim.

## Compatibility and validation

The checkpoint codec's explicit empty-list case is corrected. `list` with no
arguments produces nil, so the old reconstruction did not preserve the distinct
empty list returned by `keys` of an empty map. New writes encode that value
with an `e` tag; a frozen v0.3.15 decoder refuses that new tag. The new decoder
also reads old empty `l` syntax faithfully. Previously committed v0.3.15
checkpoints remain readable; their write-time round-trip guard had refused
values affected by this bug before append. Keep the updated codec when
recovering newly encoded empty-list values.

Stream tests cover bounds, write prevalidation, malformed replies, buffering,
EOF, reconnect reset, maintenance state, and failed-read behavior. The TCP
carrier checks arbitrary bytes and EOF against an independent echo socket.
Crypto oracles, the standard library suite, bootstrap heap probes, clippy and
formatting checks pass. Native executable activation/revert/reboot and protected
judge-ledger recovery/reservation regressions also pass.

## Remaining scope

This is a scripted native smoke test, not a model-controlled Exo experiment.
The native inference and TCP companions are not yet one integrated runner.
The synchronous buffer facade has one owner; its service only runs during
receive refills. Durable game episode journals, native visual game rendering,
general service/domain/kernel replacement and full self-hosting remain open.
Combat training, crafting, coins per dollar, fresh-account transfer and quests
still need the specified paired fixed-policy comparisons. No superiority
claim follows from this release.

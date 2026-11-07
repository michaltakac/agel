# v0.3.17: Live repair with Agel services

An Agel repair actor now requests a real model-written tool change, waits for
an independent text judge, replays predictions and held-out cases, activates
the new source, and resumes game actions during the same native OS session.
The private server independently confirms the saved result. No client process,
repair actor or game connection is recreated for activation.

The infrastructure is general: polling inference, serialized services,
buffering, byte-field packing and batched action execution are Agel modules.
RuneScape supplies the protocol adapter and an external correctness oracle.

## Executed evidence

The [real-provider transcript](evidence/v0.3.17-native-live.txt) records a
seeded planner defect: integer division plans one batch for five actions of
size three, leaving two actions unplanned. Codex supplied the diagnosis and
replacement source. Jev approved the text at 820/1000 against a fixed 800
threshold. Predictions and recorded/held-out replay passed before activation.
The retained planner closure reads the same repair actor's updated version
store: frozen source scores 3/6 and repaired source 6/6 on this probe.

The repaired planner drives `agel/batches`, which executes all five queued
walking actions in two batches. The client then consumes observations, closes
the connection and exits successfully. The saved server position is
`(3223, 3218, 0)`, with normal rights and 1× XP. The transport trace contains
exactly two opens/closes: the initial cache connection and the game connection.
There is no reconnect during repair. The client also observes Smithing level 1
and zero coins from real packets.

The real run used 27,519,932 evaluation steps in a 64 MiB VM, within the
unchanged 50-million-step limit. The measured VM/broker/client/oracle interval
was 203.168 seconds, excluding build/preparation. These are smoke-test
measurements, not a paired task-efficiency result. Other regression work ran
during development, so these wall times are not an isolated performance study.

A [delayed-provider fixture](evidence/v0.3.17-native-fixture.txt) also passes
with a writer job spanning 21.030 seconds, exceeding the pinned server's
15-second idle window. Agel services the stream between polls and during long
byte encodes. It completes the same five actions and saved-state checks at
27,520,103 steps. The [world manifest](evidence/v0.3.17-world.json) pins the
private world, cache, normal rates, quest-progress profile and login profile.

## Costs and retained evidence

The real Jev request and result are retained in the OS's protected append-only
`/judge-ledger` and published as [decoded records](evidence/v0.3.17-real-judge-records.agel).
The receipt identifies requested `jev-latest`, returned `jev-1.14.0`, 623 input
tokens and 68 output tokens. Its recorded published-rate cost is 26,166
nanodollars, or **$0.000026166**, with the price source and check date included.
There is one real judge call in this release's live session. Codex billing is
unknown; no total-model-cost or coins-per-dollar claim follows.

Agel exchange records and the planner's source/version history are appended to
the separate protected `/judge-log`. Old source remains available. Failed
runtime trials are retained in private directories and summarized in
[the development evidence](evidence/v0.3.17-development-trials.txt). Those
trials used scripted providers and incurred no paid model inference. A guest
watchdog failure is recorded as a failure; its broker was terminated after
the guest had already stopped. Experimental slab allocation was not shipped.

## General runtime changes

`agel/services` implements a serialized service registry, state updates,
reentrancy checks, nested service calls, fault inspection and explicit restart
in Agel. `agel/service-stream` uses it for stream ownership.
`agel/stream-buffer` holds the buffer state machine shared with the older
bootstrap-actor adapter. The native path no longer accumulates a full opaque
bootstrap buffer heap after every small read. It keeps current service state
and counters; application observations, actions and version histories remain
retained. Both buffer backends pass the same behavior tests, and the Agel
service tests produce no bootstrap actor events.

These services execute under their controller's authority and share its domain.
They are top-level services, with no new security-isolation claim. The repair
actor still uses the bootstrap actor mechanism. Full self-hosting remains open.

`agel/inference` supplies start/poll operations and a servicing loop with an
explicit poll budget. The combined external companion owns at most one
provider job and the configured TCP carrier. It returns pending tickets while
Agel runs its own service callbacks. A matching poll consumes a result once;
wrong IDs and additional starts are refused. Synchronous inference remains
available. Both waiting bootstrap words relay journal acknowledgments.
Rust contains no game heartbeat timer or repair/activation policy.

`agel/crypto` adds low-byte ISAAC buffers with shared refill closures.
Full-word ISAAC remains available. The byte API matches independent pinned
vectors across capacities 1, 7, 256 and 512 and repeated refills. The adapter
keeps the actual outgoing context in its Agel maintenance service and transfers
it back before the next game action. It no longer prepares a long schedule
while leaving the live connection idle.

`agel/bytes` packs retained byte fields losslessly as tagged hex, preserving
nil versus an explicit empty list. Chunked encoding and pairwise joins bound
allocation work; optional service callbacks run between chunks. A 4096-byte
pack/unpack probe, including 64 callbacks, fits two million evaluation steps.
The game history uses this representation without pruning packets. Direct
`osrs-next-packet` results still expose raw byte lists. Use
`bytes-unpack-field(event, 'bytes)` for either new packed or legacy raw history.
Native scripts use a single-result `begin` entry so intermediate definition
results do not retain extra copies of application state.

The bootstrap allocator also lets an exhausted small allocation class reclaim
unused whole-page class blocks through the existing coalescing page pool.
This mechanical fix alone did not solve the retained-heap problem; moving the
stream state machine into Agel services did. VM and watchdog limits remain.

## Run and validation

```sh
sh scripts/test-native-network.sh --repair
sh scripts/test-native-network.sh --repair --live
```

Real mode uses the existing Codex login and `TYPESAFEAI_API_KEY`. Each run
retains a private disk, generated account, source, logs, ledger extraction,
world manifest and server oracle output under `target/native-network-TIMESTAMP/`.
Do not publish these private directories. The public evidence excludes
credentials, private keys, account files and disk images.

The service/stream, polling, bounded encoding, repair/retry and carrier tests
pass. The standard library, model/effect suites, clippy and formatting checks
pass. Native source-checkpoint recovery across two reboots, retained executable
activation/revert/reboot, and synchronous native repair regressions pass.
Hosted skill/item setup, normal-account isolation and exact rat-combat XP
regressions also pass. See [the stream/service API](byte-stream.md) and
[the polling inference API](model-agents.md#polling-inference-during-a-live-byte-session).

## Remaining scope

This is a seeded repair probe with scripted task selection. The byte/cipher,
service and memory fixes were authored during repository development, not by
the inner repair actor. It does not establish autonomous OS optimization.
Durable episode journals, automatic service recovery, full language parity,
running-domain/kernel replacement and native game rendering remain open.
The required maximum-combat, adamant-crafting, money, transfer and quest tasks
still need paired fixed-policy runs with complete cost accounting. No Exo
superiority claim follows from this release.

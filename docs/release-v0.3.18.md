# v0.3.18 — Replay a live service policy before replacing it

A running Agel process measures an existing byte encoder, asks an agent to
write a replacement service policy, and tests that policy before the live
client can use it. The process, retained client closure, and stream owner stay
in place. The old source stays in the version store.

RuneScape supplies recorded bytes and an independent game-state check. The
profiler, encoder, probe, judge exchange and version dispatch are general Agel
code. The bootstrap exposes a finer clock reading under the existing clock
capability; it does not choose the cadence or run the probe.

## What the gate checks

The controller times the fixed 64-byte cadence on a recorded field, then
estimates a larger bound from the measured peak work interval with a fourfold
margin and a 512-byte cap. The agent sees the measurements and writes a pure
Agel function of the bound. The text judge reviews the proposed edit before
the controller tests it.

The probe runs the same recorded bytes on the existing stream while version 1
is still active. It checks byte equality, fewer acknowledged callbacks, lower
elapsed time, maximum work of three seconds and maximum service gap of six
seconds. The recorded bytes are encoded locally; only normal maintenance
traffic reaches the server during the probe. Failure goes back to the agent
with its reason. A successful probe is followed by the prediction and
held-out replay gate before commit. Subsequent encodes validate the policy
result and use the original 64-byte bound for fields larger than the sample.

## What happened in the native run

The [real-provider run](evidence/v0.3.18-native-live.txt) used a 4,614-byte
field from the private [pinned world](evidence/v0.3.18-world.json). The
measured upper bound was 307 bytes. Codex wrote
`(fn (limit) (* (/ limit 64) 64))`, which selected 256 bytes.

| Same recorded bytes | Fixed control | Candidate before commit |
| --- | ---: | ---: |
| Bytes per service interval | 64 | 256 |
| Acknowledged service calls | 73 | 19 |
| Elapsed replay time | 52.530 s | 18.196 s |
| Maximum work interval | 0.156 s | 0.597 s |
| Maximum acknowledged service gap | 0.825 s | 1.230 s |

The retained client used version 2 twice on later packets. The independent
server oracle confirmed position `(3223, 3218, 0)`, normal account rights and
1× XP. The trace shows only the initial cache connection and one game
connection. The entire run took 307.384 seconds, excluding build and world
preparation, and used 30,812,652 steps within the unchanged 64 MiB and
50-million-step limits.

There was one [real Jev call](evidence/v0.3.18-real-judge-records.agel).
The guest logged 755 input tokens, 68 output tokens and a published-rate
cost of **$0.000031710**. Codex dollar cost is unknown. An earlier
[delayed-provider fixture](evidence/v0.3.18-native-fixture.txt) used the same
type of live workload and passed with no paid inference; it preceded the
tape read-back and explicit gap check.

Standard library, model and effect suites, lint and format checks, native
checkpoint recovery and synchronous native repair passed. The probe tests
exercise stale receipts, attempts to advance without a receipt, and retry
after a measured refusal. The real run passed on its first proposal.

## Run it

```sh
sh scripts/test-native-network.sh --optimize
sh scripts/test-native-network.sh --optimize --live
```

Real mode uses the existing Codex login and `TYPESAFEAI_API_KEY`. Runs retain a
private guest disk, generated account, packet corpus, logs, world manifest
and oracle evidence under `target/native-network-TIMESTAMP/`. The raw recorded
field is in the guest's protected append-only `/judge-log`; the run reads
it back after closing the stream and checks it byte for byte. Judge requests
and priced results are in the separate protected append-only `/judge-ledger`.
The public evidence omits the raw corpus, account files, keys and guest disk.

This is a scoped, ordered comparison against the fixed byte cadence. It
does not include inference time in the replay timing or compare entire
game tasks. The controller selects the optimization target and supplies the
objective; the model writes the policy. Native game rendering, autonomous
target selection, full self-hosting and paired Exo task benchmarks remain open.

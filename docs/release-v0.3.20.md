# v0.3.20 — Test live encoder adaptation during combat

The native combat runner can now send an attack, profile a recorded packet,
and install an agent-written encoder policy before it resumes reading the
outcome. Both arms start with the same fixed 64-byte policy and use the
same Agel combat code, guest, byte relay and read-only server oracle. The
adaptive arm submits one real proposal and a separate text judgment. The
fixed arm keeps its original policy and makes zero external model calls.

```sh
sh scripts/test-native-network.sh --combat
sh scripts/test-native-network.sh --combat --optimize --live
```

The game connection stays open while the agent works. Agel services it from
the same cipher context, logs the recorded input, and replays the candidate
while the fixed policy remains active. It verifies the bytes and checks the
elapsed time, callback count, peak work interval and service gap before
committing the candidate. The client reads the active version when encoding
later packet history. RuneScape supplies the recorded input and saved-state
oracle. The optimization applies to the general Agel byte encoder.

Both runs passed the same check: observed NPC removal, 8 Attack XP gained,
and observed Hitpoints XP rising from 1,154 to 1,156. The oracle independently
confirmed 80 and 11,566 tenths of XP. The paired sample favored the fixed arm
on elapsed time.

| Measure | Fixed | Adaptive |
| --- | ---: | ---: |
| Task time from initial observations | 146.765 s | 273.138 s |
| Boot through persisted-state verification | 319.576 s | 454.141 s |
| Guest evaluation steps | 44,164,967 | 49,213,451 |
| External writer and judge calls | 0 | 2 |
| Recorded judge cost | $0 | $0.000032130 |
| All-in model cost | $0 | Unknown |

See the [fixed trace](evidence/v0.3.20-fixed-combat.txt),
[adaptive trace](evidence/v0.3.20-adaptive-combat.txt),
[guest judge records](evidence/v0.3.20-adaptive-judge.agel)
and [shared world and policy manifest](evidence/v0.3.20-world.json).
Both runs stayed within 64 MiB and 50 million evaluation steps. The adaptive
arm finished within roughly 0.8 million steps of the limit.

The adaptive arm's exact-input replay fell from 73 callbacks and 52.255
seconds to 15 callbacks and 15.423 seconds. It chose 320-byte intervals from
the measured bound. The new version handled two later live packets. The
durable guest ledger records one Jev call at a published-rate cost of
**$0.000032130**. The writer's dollar cost is unknown.

The buffer now retains validated byte lists of at most 256 entries per refill.
Earlier versions decoded incoming bytes, re-encoded them as hex in the
buffer, and decoded them again at each read. An identical 512-read Agel
regression went from 540,281 steps to 264,433. The tests also verify that a
malformed byte later in a refill rejects the whole refill before any bytes
are returned. Both native arms use this buffer.

The task clock starts after initial game observations and includes the full
optimization in the adaptive arm. Wall time includes boot, login, play,
and server verification. Each arm uses a fresh normal account at 1× XP.
The comparison covers one encounter in each arm with live combat randomness.
A hosted combat regression briefly overlapped the adaptive run on the same
world after the adaptive attack was sent. Maximum stats and repeated seeded
trials remain open.

The standard library, effects and model suites passed, along with the
hosted combat regression, lint and formatting checks.

# Agel OS and the Exo benchmark

Status through v0.3.223, 7 November 2026.

Agel can run a RuneScape client in a native guest, revise a byte-service policy on the live connection, and preserve a rejected edit and its costs. The strongest result is a verified edit lifecycle. The matched runs so far do not establish a task-level advantage over Agel's fixed control or a head-to-head win against Exo. [Experiment programme](exo-programme.md) · [Exo evaluation](https://arxiv.org/html/2611.00463v1#S5)

## Implementation and evidence

- The private OpenRune world runs revision 241 with normal accounts at 1× XP. Separate admin fixtures stage skills and items. The Agel example owns the game protocol, observations, task scripts and history. The guest uses a host byte relay, and the benchmark uses an independent server-save oracle. RuneScape and DOOM remain examples outside the platform crates. [Client scope](osrs-client.md) · [Example separation](release-v0.3.196.md)
- The Agel repair actor sends candidate text for independent judgment, runs recorded and held-out replay, then activates or retains the current policy. A retry receives the refusal reason. A continuation predicate can stop between proposal, judgment and activation. [Repair](release-v0.3.11.md) · [Guarded transitions](release-v0.3.199.md) · [Measured retry evidence](release-v0.3.204.md)
- Native judge receipts record attempts and known charges in a protected append-only journal. Writer token usage is available in recent runs, but writer prices remain unknown for the results below. Unknown cost stays unknown in the scorer. [Journal checks](release-v0.3.6.md) · [Writer accounting](release-v0.3.182.md)
- Separate native demonstrations cover live Lisp form redefinition with an existing closure, guarded program activation and revert, checkpoint recovery, macro-built tools, domain history and kernel-slot rollback. The Agel evaluator is a subset, and the boot/runtime path still uses Rust. The demonstrations establish behavior on their tested paths. [Native program evolution](release-v0.3.12.md) · [Live evaluator](release-v0.3.14.md) · [Checkpoints](release-v0.3.15.md) · [Domain history](release-v0.3.61.md) · [Kernel history](release-v0.3.62.md) · [Macro tools](release-v0.3.64.md)

## Matched private-world results

Times are guest task times from individual pairs with uncontrolled game randomness. The fixed controls below made zero model calls. Known judge costs exclude unpriced writer calls.

| Workload | Fixed | Adaptive or transferred | Evidence |
| --- | --- | --- | --- |
| One rat, fixed-first | Passed in 69.306 s | Passed in 143.683 s. Replay rejected the first edit and the judge rejected the retry. The guest retained the original policy. Two judge calls cost $0.000083664. | [Score](evidence/v0.3.204-native-one-rat-score.txt) |
| Three melee skills, 16 XP each | Failed in 364.001 s. Saved 32 Attack and 32 Strength XP. | Failed in 316.115 s. Saved 16 Attack XP. The edit passed judgment and replay and saw one live use. One judge call cost $0.000041370. | [Score](evidence/v0.3.212-native-melee-score.txt) |
| One-rat transfer to a fresh account | Passed in 93.187 s | Passed in 110.427 s for combat and 167.469 s including recovery. The retained policy served six live packets. The recipient made zero model calls. Donor learning was billed separately. | [Score](evidence/v0.3.185-native-transfer-score.txt) |
| Adamant platebody from staged bars | Passed in 27.3 s | Passed in 127.6 s after falling back to the fixed policy. Two writer calls were unpriced. | [Score](evidence/v0.3.180-native-pair-scores.txt) |
| First bronze sale | Passed in 292.0 s with three saved coins | Failed in 222.3 s with zero saved coins. Coins per dollar is undefined. | [Score](evidence/v0.3.180-native-pair-scores.txt) |

Two other one-rat pairs passed in both arms. Each adaptive edit cut replay service calls from 73 to 25, then served zero live packets. The measured replay savings implied 212 and 95 comparable replays to recover optimization time in those runs. The fixed arm was faster in both. [First score](evidence/v0.3.197-native-one-rat-score.txt) · [Second score](evidence/v0.3.200-native-one-rat-score.txt)

Cook's Assistant has a fresh-account hosted completion with earned ingredients. Native quest evidence covers narrower phases, and a matched multi-quest campaign is still open. The smelting and bank examples have component fixtures, with live three-bar smelting and bank round-trip runs queued. The newer 16 XP melee pair failed in both guests. The fixed save met the full goal, including 32 Defence XP and the expected stance. The adaptive save reached Attack and Strength only. Its accepted edit reduced replay callbacks from 73 to 19 and served zero live packets. [Quest evidence](release-v0.3.29.md) · [Smelting](release-v0.3.215.md) · [Banking](release-v0.3.218.md) · [Newer melee pair](evidence/v0.3.223-native-melee-score.txt)

## What would establish a win

Exo evaluates maximum combat stats, adamant platebody production with resource gathering and training, currency accumulation over roughly eight active hours, fresh-account reuse, and quest throughput. Agel's current one-rat, bounded melee, staged crafting and first-sale results cover component workloads. A matched evaluation needs the same objectives, comparable game access and model settings, repeated trials, complete cost accounting, and separate reporting of learning and evaluation costs. Agel also needs an editing-disabled control with the same starting model, tools and budgets for the adaptive-policy experiment. [Exo evaluation](https://arxiv.org/html/2611.00463v1#S5) · [Roadmap](roadmap.md#exo-evaluation-and-os-evolution)

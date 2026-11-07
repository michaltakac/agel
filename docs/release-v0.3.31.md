# v0.3.31 — Compare fresh-account combat runs

The comparison command reads the two native run directories, checks the
guest reports against independently saved server XP, and emits Agel
input for the paired scorer. It requires different normal accounts and
matching private world, boot image, Agel executable, game adapter, and
combat source. The fixed and transferred encoder policies differ by design.

```sh
{
  cat crates/agel-stdlib/benchmark.agel
  python3 scripts/compare-native-combat.py FIXED_EVIDENCE TRANSFER_EVIDENCE
} | cargo run -q -p agel-cli
```

The comparison covers fresh-account execution after policy learning. It
requires no inference in either evaluation arm. The donor's historical
writer cost remains unknown, so this does not score the total cost of
learning and transfer together. Both trials use uncontrolled combat
randomness.

| Measure | Fixed policy | Transferred policy |
| --- | ---: | ---: |
| Server-verified combat result | Pass | Pass |
| Task time | 163.168 s | 112.747 s |
| Boot through saved-state verification | 338.475 s | 257.838 s |
| New model and judge cost | $0 | $0 |

The transferred policy took 50.421 seconds less on the task clock. The
scorer ran inside Agel OS and returned a zero cost difference for the
evaluation phase. The evidence covers one successful run per arm. The extractor currently
requires a verified result from each run. Aggregating failed trials remains open.

The Cook's Assistant route now tries an alternate tin rock when its first
attempt yields no ore, and retries speaking to the cook if no first page
opens. A fresh normal account gathered all three ingredients and completed
the quest in one session without admin setup. The server verified all
three hand-ins, one quest point, and 300 Cooking XP after 567.373 seconds.

See the [combat, scorer, and quest traces](evidence/v0.3.31-combat-and-quest.txt).
Repeated trials and the full Exo workloads remain open.

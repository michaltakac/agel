# v0.3.180

Agel OS evaluated two retained native benchmark pairs with the current
scorer. These are single pairs from the private world with uncontrolled
game randomness.

| Task | Fixed | Adaptive |
| --- | --- | --- |
| Adamant platebody | Passed in 27.3 s | Passed in 127.6 s after falling back to the fixed policy |
| First bronze sale | Passed in 292.0 s with three saved coins | Failed in 222.3 s with zero saved coins |

Both controls dispatched zero model calls. The crafting adaptive arm
made two writer calls with unknown dollar costs. The bronze-sale adaptive
arm had one priced judge call and one unpriced writer call. The records
leave coins per dollar undefined and preserve the observed coin totals.

[Agel OS score outputs](evidence/v0.3.180-native-pair-scores.txt)

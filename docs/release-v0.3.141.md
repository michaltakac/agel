# v0.3.141

Agel’s record chain seals an active chunk after eight records. Old
snapshots with larger chunks remain readable and can still accept
new records in order.

One hosted run appended and replayed 600 records with large payloads.
Elapsed time was 57 ms instead of 104 ms, with about 4% more evaluation
steps. The record-chain, benchmark, and interface suites passed. Native
timing remains unmeasured.

The live hosted mining test with the revised tutor dialogue also passed.
The server saved a bronze pickaxe, one copper ore, and 17.5 Mining XP
on a fresh normal account.

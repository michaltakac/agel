# v0.3.30 — Transfer a retained native combat policy

The native runner boots a copy of the verified adaptive combat disk and
opens a fresh account on the same pinned private world. Agel reads the
accepted encoder version, the frozen predecessor, and the recorded
workload from the guest's append-only audit log. It parses those records
as inert data, checks the version store, and replays the accepted policy
against its bounds before connecting.

The run exercises one rat encounter. Agel checks fresh combat observations,
uses the retained policy on the new connection, and verifies that the
donor audit log is identical afterward. A separate read-only server query
checks the saved XP at normal rights and 1× XP.

```sh
sh scripts/test-native-network.sh --combat --transfer-from DONOR_EVIDENCE_DIRECTORY
```

The donor directory comes from a verified adaptive combat run and contains
its original disk, world manifest, and saved-state evidence. The transfer
adds no writer or judge charges. The original adaptation cost remains
unknown because its writer lacked a dollar receipt.

The fresh account finished the task in 112.747 seconds and the runner
verified saved state in 257.838 seconds including boot and login. The
transferred policy served six live packets. The earlier fixed sample
took 226.847 seconds on the task clock and 441.543 seconds end to end.
Both samples used the same pinned world, boot image, Agel executable,
and combat source. They ran on separate occasions with uncontrolled
combat randomness, so the timing difference is a historical comparison.

See the [live traces](evidence/v0.3.30-native-transfer.txt) and
[transfer world manifest](evidence/v0.3.30-world.json). Repeated paired
trials, full combat progression, and transfer across other tasks remain
open.

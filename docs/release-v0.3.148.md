# v0.3.148

The melee trainer checks already available packets before retrying a
stance selection. A delayed stance update can therefore confirm the
selection without the trainer toggling away and back.

The interface fixture exercised both paths. An unconfirmed selection
still used the existing retry. A queued confirmation needed one click
per stance. Benchmark and interface tests passed. The running live melee
pair started before this change.

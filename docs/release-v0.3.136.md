# v0.3.136

Agel's combat transfer recovers the retained policy, workload, and profile
in one pass over the donor audit log. It selects the last complete record
of each kind and ignores an unterminated trailing record.

A hosted fixture with a 4,614-byte retained workload returned the same
records in 216,673 evaluation steps instead of 573,155. All benchmark
tests passed. Native recovery timing remains unmeasured.

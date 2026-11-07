# v0.3.150

The native melee trace reports the start of each skill phase, stance
requests, and retries. It includes the observed stance and the previous
target where relevant. Encounter and final score formats are unchanged.

The interface fixture verified the progress events across all three
skills. Benchmark and interface tests passed. The live melee run
started before this logging change.

# v0.3.41 · Reuse the active encoder callable

The native byte encoder now evaluates the active Agel policy into a
callable once and reuses it while the policy version stays active.
The controller refreshes the callable after the judgment and replay
sequence resolves. Retained-policy startup uses the same refresh path.

The benchmark regression checks that version 1 returns 64, version 2
returns 320 at a measured bound of 327, and inputs outside the measured
workload still fall back to 64. The matched live timing comparison
remains pending.

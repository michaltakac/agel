# v0.3.85 · Let the optimizer tune cadence below the safe limit

The Agel optimizer can now propose a capped service cadence and try
a different cadence after replay refusal. The candidate still has to
preserve bytes, meet work and gap budgets, reduce service calls, and
beat both adjacent fixed controls on elapsed time.

Fresh-account transfer checks the retained cadence against the measured
bound and replays the recorded bytes. The fixture verifies a 192-byte
cap under a measured limit of 327. A live optimizer run is underway.

The latest fixed and adaptive economy runs both stopped before the
first sale. Their Agel executables differed, so the paired scorer
rejected the comparison.

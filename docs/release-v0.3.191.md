# v0.3.191

After committing an edit, the native optimizer logs a replay break-even
estimate in the protected journal. It compares the candidate's measured
duration with the faster adjacent control, then divides the optimizer
overhead by that saving. The result counts equivalent replay workloads.

For the retained v0.3.189 one-rat pair, those measurements imply 124
equivalent replays to recover the optimization time. That is a replay
estimate. The live task still took longer in the adaptive arm.

# v0.3.144

The native encoder dispatcher uses the fixed cadence when a packet is too
short for a cadence change to matter or its trusted limit allows only 64.
Eligible packets still run the active policy and validate its result.

A hosted fixture made 400 short-packet decisions. Both versions returned
64 for every decision. Evaluation steps fell from 153,212 to 9,212.
Benchmark and interface tests passed. Native elapsed time remains unmeasured.

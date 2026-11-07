# v0.3.131

Agel’s hex encoder checks the input length once and uses the empty tail
to stop its per-byte loop. Empty lists still encode as empty text, and
the encoder preserves the service callback boundaries.

A 4,614-byte hosted comparison returned identical hex and made the same
19 service calls. Evaluation steps fell from 680,266 to 670,900. The
byte-stream, benchmark, and RuneScape interface suites passed. Native
elapsed time remains unmeasured.

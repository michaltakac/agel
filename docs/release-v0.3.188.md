# v0.3.188

Adaptive one-rat combat uses the shared packet observation hooks
during preparation and inference. It resumes the encounter with the
updated observations and retains the count of packets processed while
waiting for inference.

The preceding reverse journal scan also passed native donor replay with
identical lineage checksums. On that donor it used about 2% more evaluator
steps than the forward scan after the CRC change.

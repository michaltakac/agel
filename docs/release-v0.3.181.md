# v0.3.181

`benchmark-writer-estimate` computes a nanodollar estimate from writer
usage receipts and caller-supplied input, cached-input, and output rates
in nanodollars per token. It reports a range when cache usage is unknown
and leaves both bounds unknown if a call has no usage receipt.

The estimate is separate from the benchmark's recorded charges and
coins-per-dollar result. Hosted tests and a synthetic calculation inside
Agel OS verified the range.

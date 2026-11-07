# v0.3.127

The Agel RuneScape client retains fragmented reads as chunks and joins
them when the requested bytes have arrived. It avoids reversing the
assembled packet. Exact reads still return bytes in wire order and
reject early EOF and oversized replies.

One hosted comparison assembled the same 4,614 bytes from fragments of
up to 256 bytes. Elapsed time fell from 3.65 to 1.24 seconds, with about
213,500 evaluator steps in either run. The fragmented wire and interface
suites passed. Native timing remains unmeasured.

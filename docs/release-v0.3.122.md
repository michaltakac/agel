# v0.3.122

The shared Agel stream buffer takes one byte directly from the buffered list
when the caller requests a single byte. Larger reads use the existing split.
Refill validation, EOF, and service behavior are preserved.

Reading a 256-byte buffer one byte at a time produced the same bytes and
used 73,894 hosted evaluation steps, down from 89,453. The byte-stream,
benchmark, and RuneScape interface suites passed. Native elapsed time
remains unmeasured.

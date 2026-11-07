# v0.3.116

The RuneScape wire client returns a complete read directly and counts
partial reads without traversing their byte lists. Fragmented payloads
still preserve byte order. The client rejects oversized transport
replies.

The fragmented wire fixtures and all four interface tests passed. A
hosted evaluation of zero-length, 256-byte, and 4,096-byte reads
returned identical values and used 189,382 steps instead of 301,207.
Native wall-clock impact remains unmeasured.

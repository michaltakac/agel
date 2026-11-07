# v0.3.113

The Agel byte encoder uses a two-character lookup for each validated
byte. It preserves output and service call boundaries.

A 4,614-byte hosted evaluation at quantum 256 produced identical
output and used 680,179 steps, down from 860,125. The byte-stream,
benchmark, and RuneScape interface tests passed. Native wall-clock
impact has yet to be measured.

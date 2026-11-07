# v0.3.120

Agel’s hex decoder reads adjacent pairs in one pass and uses the original
parser for whitespace and a trailing unpaired character. It preserves the
decoded values and errors checked by the comparison.

A 256-byte hosted input used 34,592 evaluation steps, down from 47,909.
The short whitespace-separated input used 744 steps, up from 704. The
byte-stream, benchmark, and RuneScape interface tests passed. Native
wall-clock impact has yet to be measured.

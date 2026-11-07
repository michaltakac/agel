# v0.3.142

The native RuneScape runner opts into 1 KiB carrier reads. The Agel
byte-stream constructor still defaults to 256 bytes. Write and entropy
limits are unchanged.

A native Agel OS fixture read and verified all 1,024 bytes over TCP and
the serial carrier in one request. Carrier, byte-stream, and interface
tests passed. The effect on live benchmark time remains unmeasured.

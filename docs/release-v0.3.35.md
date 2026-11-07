# v0.3.35 · Keep byte carrier frames off the desktop terminal

The desktop forwards byte carrier requests to serial without repainting
them in the process terminal. Ordinary Agel output still appears in the
terminal. Multi-block request frames follow the same path.

In a QEMU loopback test, the median interval between 18 carrier requests
fell from 644 ms to 12 ms. Each boot verified eight 256-byte echo exchanges
and exited successfully. This measures the carrier path in that fixture.
The private-world melee benchmark runs separately.

[Recorded byte carrier test](evidence/v0.3.35-byte-carrier.txt)

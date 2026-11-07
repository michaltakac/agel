# v0.3.128

The RuneScape client can service the connection between chunks while it
assembles a fragmented game packet. The exact-read interface used for
login and cache traffic keeps its existing behavior.

The wire fixture verified two service callbacks for a three-fragment
read and no callback for a complete read. Wire and interface suites
passed. The previous native quest hand-in disconnected after a
16-second gap between a read and the next write. The effect of the new
service points on native disconnects is pending.

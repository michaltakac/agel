# v0.3.69 · Retry the mining tutor from a second tile

In a native quest run the server saved the fresh account at the mining
waypoint after the client timed out waiting for the tutor. The RuneScape
example now walks to a second tile if the first approach misses. It keeps
the packet and cipher state and sends keepalives during both waits.
If both attempts miss, the error lists the observed NPC types and active
world. The quest and economy scenarios share this route.

An Agel fixture exercised the second approach, preserved wait state,
keepalive cadence, and the diagnostic when both attempts miss.

The native v0.3.68 replay gate also passed with fixture providers. The
candidate replayed 4,614 bytes in 9.18 seconds with 25 service calls.
The adjacent controls took 11.30 and 10.16 seconds with 73 calls each.
Agel committed the policy and the guest exited successfully.

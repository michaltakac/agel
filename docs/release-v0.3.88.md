# v0.3.88 · Use fresh route observations at the Cook

The RuneScape example shares a route predicate between the mining
tutor and Cook approaches. It checks that the server advertised and
cleared a new route for the requested tile. The Cook approach also
requires the NPC observation before ending its wait early. The bounded
fallback checks the latest NPC state if the matching flag never arrives.

The interface fixture covers stale and mismatched route flags, and the
quest fixture verifies the ordering before talk. The hosted mining test
with the preceding route logic saved one pickaxe, one copper ore, and
17.5 Mining XP.

Agel also services the connection while assembling recorded replay
chunks. The preceding adaptive economy attempt lost its connection
before reaching the sale task.

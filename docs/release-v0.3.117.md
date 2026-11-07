# v0.3.117

The RuneScape bit reader and NPC decoder use direct collection counts
for packet length and the tracked NPC limit. The bit reader still
requires a byte list.

A 96-NPC packet produced identical decoded state in 206,748 Agel
steps, down from 279,361. The NPC protocol fixtures and all four
interface tests passed. Native wall-clock impact remains unmeasured.

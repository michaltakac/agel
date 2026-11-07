# v0.3.206

Melee training stops retrying the same target when the NPC disappears.
It allows one short observation window for combat XP to arrive. If that
window confirms the original encounter, training credits it. Otherwise
training can select another visible target without walking back toward
the missing NPC.

The OSRS interface fixture covered both delayed XP and retargeting.
The existing stalled-but-visible target retry remains covered.

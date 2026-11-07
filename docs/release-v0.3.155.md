# v0.3.155

The OSRS melee example can finish a skill phase as soon as it observes
the requested skill XP and an increase in Hitpoints XP. Encounter reports
record whether the target had disappeared when those observations arrived.
The first rat encounter still requires observed removal.

In the prior native fixed run, the account saved 36 Defence XP against
an 8 XP target, but the run failed while waiting for encounter completion.
A simulated stream verified that the updated policy accepts the XP outcome
while reporting the rat as still visible.

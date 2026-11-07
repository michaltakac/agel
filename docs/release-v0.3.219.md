# v0.3.219

Before retargeting a stalled melee encounter, Agel checks up to 512
already available packets for delayed combat XP. The same confirmation
predicate applies to the queued observations. The check keeps the
current target when those packets confirm the encounter.

The interface fixture verified delayed XP arriving after the encounter's
bounded wait. In the earlier live fixed arm, the server saved 32
Defence XP while the guest reported a stalled encounter. The revised
check still needs live validation.

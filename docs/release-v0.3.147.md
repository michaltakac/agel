# v0.3.147

The adaptive melee and crafting runners can observe game packets while
preparing replay input and awaiting inference. If the optimizer stops
before gameplay, the runner records the guest clock and checks the
saved server state. The scorer distinguishes an interruption from a
policy refusal or commit.

The interruption fixture passed for all four task types. Re-scoring
the retained crafting pair produced identical input for the Agel scorer.
The live melee pair started before this change.

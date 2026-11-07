# v0.3.212

The melee trainer distinguishes an old cached stance from a fresh
update after its final click. If no new stance arrives, it can use
XP in the requested skill as evidence even when an earlier stance
is cached. A fresh contradictory update still stops the phase.

Six interface tests passed. They covered cached stance, a fresh
contradiction, and update counts across a variable reset.

Both arms of the earlier 16 XP melee pair failed. The fixed arm
saved 32 Attack and 32 Strength XP, then stopped before earning
Defence XP. The adaptive arm saved 16 Attack XP and stopped before
earning Strength XP, after committing and using one policy edit.

[Agel OS score output](evidence/v0.3.212-native-melee-score.txt)

# v0.3.167

The RuneScape client skips unrelated state observers for `PLAYER_INFO`
packets. It still advances the wire cipher state, retains the packet
bytes, and counts the packet toward observation tick budgets.

The hosted private-world game suite verified movement, administrative
skill and item setup, normal-account permission checks, and saved state.

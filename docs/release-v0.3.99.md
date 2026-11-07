# v0.3.99

The RuneScape mining example splits the walk to the tutor at a waypoint
after the server returned a partial route on the direct walk. It traces
the tutor's advice choice, site icon, and pickaxe award. If the initial
copper interaction leaves both ore and XP at zero, it retries once. A
partial or inconsistent outcome stops the scenario.

The prior native fixed-policy run mined both ores, smelted a bronze bar,
and sold it for three coins. The server verified the coins and XP. The
adaptive arm and the separate waypoint pair are pending.

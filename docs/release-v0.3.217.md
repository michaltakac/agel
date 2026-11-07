# v0.3.217

The banking example now has a revision-241 layout for OpenRune.
The layout comes from the pinned game-value cache, and the Varrock
West booth coordinates come from the pinned map cache.

The private-server crafting harness adds a banking check. It stages
ore on a dedicated account, removes setup rights, then asks Agel to
deposit and withdraw the ore. Both transfers must appear in the
client's inventory observations. The saved inventory and XP must
match the starting snapshot. Live validation is queued.

# v0.3.28 — Complete Cook's Assistant with earned ingredients

A fresh normal account completed Cook's Assistant using ingredients
obtained through ordinary gameplay. Agel picked up a pot, earned coins by
making and selling a bronze bar, and bought a bucket. It milked a dairy cow,
picked up a farm egg, gathered wheat through the field gate, and milled it
into flour. It then returned to the cook and handed in all three ingredients.

The verified account reconnected after its first route stalled following
the milk pickup. It kept its earned equipment and milk on that same account.
The server verified quest stage 2, all three hand-ins, one quest point,
and 300 Cooking XP at normal rights and 1× XP. No admin setup was used.

The Agel adapter now encodes the third location option used to descend the
mill's middle ladder. The revision 241 packet fixture checks the wire bytes.
The route, map coordinates, and quest assertions are in the RuneScape fixture.

```sh
sh scripts/test-osrs-quest.sh --gather
```

This command exercises the combined hosted route on a new account. The
published completion trace is from the recovered account. Native completion,
an uninterrupted run of the final route, and the paired adaptive benchmark
remain open.

See the [live traces](evidence/v0.3.28-cooks-assistant.txt) and
[pinned world manifest](evidence/v0.3.28-world.json).

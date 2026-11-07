# v0.3.24 — Earn the first coins from a fresh account

Agel gets a pickaxe from the Lumbridge tutor, mines copper and tin, and
smelts a bronze bar. It then approaches the Lumbridge General Store,
trades with a shopkeeper whose NPC index it has observed, and waits for
both store interfaces. It checks that it holds one bronze bar and zero
coins before sending Sell 1.

The client adds `osrs-trade` for the third NPC action and
`osrs-click-inventory-item` for item actions on an open interface. The latter
finds the item in the observed inventory and encodes its current slot in
the interface action. The packet fixture verifies the fields and rejects
an absent item, an unobserved NPC, and a closed interface. The game server
still enforces its own shop rules and matches the submitted item to the
slot.

```sh
sh scripts/test-osrs-mining.sh --sell
```

The runner uses the hosted Agel evaluator and a fresh normal account. It
checks that the bar disappears and coins arrive, then independently
reads saved inventory and skill XP from the private server. The path
uses ordinary actions throughout and dispatches zero model calls.

Two clean runs saved **3 coins** apiece, removed the bronze bar and both
ores, and retained the tutor pickaxe. They also saved 35 Mining XP and
6.2 Smithing XP. Wall times through verification were 159.836 and 157.352
seconds. An intervening run saved the same result but exposed a stale
verifier assertion that still expected a bar. The final verifier waits for
the mode's actual outcome inventory and checks that the sold bar is absent.
See the [live traces](evidence/v0.3.24-sales.txt) and
[pinned world manifest](evidence/v0.3.24-world.json).

The standard library suite, packet fixture, lint, and formatting checks passed.

This establishes an economy outcome for a fixed route. The coins-per-dollar
benchmark needs priced model usage, a fixed observation window, and paired
adaptive and fixed policies.

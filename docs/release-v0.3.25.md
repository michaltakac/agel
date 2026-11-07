# v0.3.25 — Complete Cook's Assistant through ordinary dialogue

Agel takes a fresh character through the Cook's Assistant acceptance and
hand-in. It waits for the next dialogue panel before answering, including
when consecutive pages reopen the same interface. The completion check
requires the quest scroll, one quest point, 300 Cooking XP, and removal of
the milk, egg, and flour from inventory.

The client records explicit subinterface opens and reads signed small,
large, and long player variables from the revision 241 protocol. The
packet fixture checks repeated opens, the signed decoding boundaries, and
variable reset. The fixed Agel route selects the first option at each of
the cook's two menus.

```sh
sh scripts/test-osrs-quest.sh
```

The hosted runner stages one of each ingredient on a fresh fixture account,
checks that the quest is unstarted, and removes admin rights before the
gameplay session. The read-only server verifier checks stage 2, all three
ingredient flags, one quest point, and 300 Cooking XP. It also checks that
the player still has normal rights and a 1× XP rate.

Two live hand-ins on different fresh accounts passed, taking 31.534 and
30.381 seconds from normal-rights login through server verification.
The final trace records the saved values directly. See the
[live traces](evidence/v0.3.25-cooks-assistant.txt) and the
[pinned world manifest](evidence/v0.3.25-world.json).

The standard library suite, packet fixture, lint, and formatting checks passed.

This is a quest completion fixture with staged ingredients. Ingredient
acquisition and the multi-quest adaptive benchmark remain open.

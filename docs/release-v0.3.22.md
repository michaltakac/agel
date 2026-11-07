# v0.3.22 — Earn the first pickaxe and copper on a fresh account

Agel walks from the private world's starting area to the Lumbridge mining
tutor, discovers the tutor's live NPC index, and obtains a bronze pickaxe
through the tutorial dialogue. It then interacts with a copper rock and
observes the ore and Mining XP. The test starts with zero Mining XP and no
pickaxe or ore. The account has normal rights throughout, and the scenario
uses no admin commands.

The revision 241 game module adds `osrs-talk` for an observed NPC and
`osrs-resume-dialogue` for an observed interface. A resume can select a
child slot or continue a dialogue without a child. The first live attempt
stalled at the item preview, which enables slot 0. Ordinary text
continuations use the no-child value. The packet fixture checks action
encoding and refusal when the target NPC or interface is absent.

The pinned cache identifies the tutor as type 3222 and the tested copper
rock as type 10943 at `(3230, 3147, 0)`. The independent read-only server
query verifies one bronze pickaxe, one copper ore and exactly 17.5 Mining XP
saved at 1× rates. The client observes 17 whole XP on the wire. Two clean runs on different
fresh accounts verified the same outcome, each taking about a minute through
server verification. See the [live traces](evidence/v0.3.22-mining.txt)
and [world and policy manifest](evidence/v0.3.22-world.json).

```sh
sh scripts/test-osrs-mining.sh
```

The test uses the hosted Agel evaluator and the pinned private world. It
exercises fresh-account material acquisition. The previous release verifies
adamant platebody manufacture inside Agel OS with staged stock. Training and
gathering the actual adamant materials from a fresh account remain part of
the full crafting benchmark.

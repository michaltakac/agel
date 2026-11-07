# v0.3.27 — Observe ground items before acting

The Agel RuneScape adapter now decodes ground-item additions, quantity
changes, and removals from revision 241 enclosed zone updates. It keeps
last-observed counts by tile and plane. A missing observation is distinct
from an observed count of zero. Zone resets clear the corresponding
observations. Scene rebuilds retain overlapping zones and discard those
outside the new area. Instance rebuilds clear the static-scene observations.

The existing fresh-account container fixture now waits for the server to
advertise the castle pot. It verifies the ground count changes from one
to zero and the pot enters inventory. It then earns three coins and
spends two on the bucket as before. The saved inventory is verified
independently.

```sh
sh scripts/test-osrs-quest.sh --containers
```

The observer covers static scenes and enclosed zone updates from the pinned
private world. A live survey saw all six candidate eggs at the Lumbridge
farms. One attempted pickup left both the ground and inventory counts
unchanged. That narrows the route failure to the interaction or approach.
Native login and movement passed. A separate native pickup probe timed
out before it could verify the ground and inventory changes.
Completing the full quest through ordinary gameplay and measuring an
adaptive policy against a fixed control remain open.

See the [live traces](evidence/v0.3.27-ground-observations.txt) and
[pinned world manifest](evidence/v0.3.27-world.json).

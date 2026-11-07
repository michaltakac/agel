# v0.3.149

When a rat disappears after an encounter, the melee trainer selects an
already visible alternative before waiting for the old target to return.
If there is no visible alternative, it keeps its existing bounded wait.

All six interface tests and three benchmark tests passed. The target
fixture covers both choices. The live melee pair started before this
change.

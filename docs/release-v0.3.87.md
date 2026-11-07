# v0.3.87 · Observe server route flags

The RuneScape client tracks the destination and clear events in revision
241 map-flag packets. The mining tutor approach can stop waiting when it
observes both a fresh cleared flag for the requested destination and the
tutor. A cleared flag alone can also mean cancellation. The bounded wait
still checks the latest observations when no matching flag arrives.

The packet fixture exercises an initial clear, a destination, a clear
for that destination, and a new route. Both native economy arms with the
new route observation are underway.

A fresh hosted Cook's Assistant run with the preceding route gathered
all ingredients through gameplay and saved all three hand-ins, one
quest point, and 300 Cooking XP at normal rights.

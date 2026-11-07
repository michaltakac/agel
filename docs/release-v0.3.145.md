# v0.3.145

The native quest traces pickup commands and inventory packets separately
from dialogue. If an item is still missing at the end of a bounded wait,
Agel checks already available packets before reporting the item count
and last route it observed.

The inventory trace fixture and all six interface tests passed, including
an item update that becomes visible only in the final packet drain. Live
verification remains pending.

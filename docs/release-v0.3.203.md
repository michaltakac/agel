# v0.3.203

The native service optimizer selects the largest recorded byte field
within its supported replay size of 1,024 to 8,192 bytes. Oversized
records no longer hide eligible workloads. Equal sizes favor the newer
record, and the complete history remains available for inspection.

Hosted and 1 GiB guest fixtures exercised selection from both list
and chunked histories. The optimizer source digest changed, so transfer
from an older melee donor requires its matching source tree.

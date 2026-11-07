# v0.3.163

Both native byte bridges report counts for successful reads and writes.
An empty read records EOF, and readiness probes record idle, ready, or
EOF. The trace records metadata without printing the transferred bytes.
This helps distinguish a slow guest from a socket that stopped supplying
data during a benchmark.

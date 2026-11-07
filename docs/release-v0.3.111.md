# v0.3.111

The RuneScape mining example now requires a fresh tutor menu open and
a fresh pause-input event after talking. If a retry walk leaves an old
menu visible in the client, the example still sends a new talk and
waits for its own actionable menu.

A fresh hosted account earned the tutor pickaxe and mined its first
copper ore. The regression fixture distinguishes a stale menu from
a newly opened menu whose input has been enabled. The matched native
rerun is queued.

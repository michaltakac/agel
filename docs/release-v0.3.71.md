# v0.3.71 · Retry tutor dialogue and record the live melee comparison

The RuneScape example now checks for the tutor's dialogue after each
packet and keeps sending keepalives. If the first talk produces no menu,
it moves to a second tile, observes the tutor again, and retries with
the current NPC index. An Agel fixture exercises that retry.

The live three-skill melee pair completed on separate fresh accounts
with matching verified workload and guest binaries. Randomness was
uncontrolled. The fixed run passed in 1,087.387 seconds. Its independent
server readback verified the saved XP. The adaptive run failed after
1,025.571 seconds and retained the fixed policy after both proposals
were refused. Two judge receipts totaled 80,850 nanodollars. Two writer
charges remain unknown, so the scorer leaves the adaptive total unknown.
The elapsed time on the failed arm measures time to failure.

See the [scorer input](evidence/v0.3.71-melee-pair.agel).

# v0.3.19 — Run combat inside Agel OS

`sh scripts/test-native-network.sh --combat` runs the fixed combat policy
on a fresh normal account in the private world. Agel selects an observed rat,
sends the attack, waits for increased Attack and Hitpoints XP and checks that
the same NPC index disappears. The runner then uses a read-only server query
to verify that saved XP matches the packet observations at whole-XP precision.
The account has normal rights and 1× XP. It uses zero model and judge calls.

The [verified run](evidence/v0.3.19-native-combat.txt) gained 8 Attack XP
and increased observed Hitpoints XP from 1,154 to 1,156. The server saved
80 and 11,566 tenths of XP respectively. Agel retained 2,440 packet and
action records and used one cache connection and one game connection.
The guest measured 136.478 seconds from initial observations to the combat
outcome. Boot through the saved-state oracle took 302.391 seconds. The
64 MiB guest used 49,413,585 of its 50 million steps, leaving little room
for longer or slower encounters. The [world manifest](evidence/v0.3.19-world.json)
records the server, cache and fixed combat policy hash.

The first [native trial](evidence/v0.3.19-native-trials.txt) exhausted the default 32-tick observation window.
A subsequent trial with a larger window reached the 64 MiB guest heap limit
while consuming queued updates, even though the server had saved the XP.
The hosted check also exposed variability in XP per fight. The Agel score now
requires gains in both stats and NPC removal, and the independent oracle
checks the exact observed whole XP rather than a hard-coded eight Attack XP.

The combat runner uses `agel/record-chain` to retain packet and action
history. Appends copy a current chunk of at most 32 records. Earlier chunks
are shared through Agel closures. The runner folds the retained history
without copying it into one list. The same module works with arbitrary Agel
records. The game adapter accepts it through the optional `history-push`
callback, preserving list histories for existing callers.

The standard library suite, hosted combat, live tool replacement, and
normal-account permission checks passed.

This is a fixed-policy correctness baseline for a single encounter. The full
maximum-stats workload and a matched adaptive combat arm remain to be run.

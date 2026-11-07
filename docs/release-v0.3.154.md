# v0.3.154

The RuneScape melee example now requires the requested combat stance
to be observed before it starts the next skill phase. If the bounded
retry ends without confirmation, the failure records the expected and
observed modes in the native benchmark's existing failure log.

A simulated game stream verified that the example refuses an
unconfirmed stance after retry and accepts a confirmed stance.

# v0.3.135

The RuneScape melee trainer uses the game client's maintained packet
observer for each five-tick window. A native connection can send a
paced keepalive during packet reads and assembly, including during
stance changes and retries.

The five interface tests and six benchmark tests passed. The interface
fixture still verifies stance recovery and retargeting after a stalled
attack. Live three-skill verification is pending.

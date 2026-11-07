# v0.3.196

The RuneScape host wire harness and game interface tests now belong to
the example package under `examples/osrs`. The legacy DOOM host bridge
lives under `examples/doom/host` and keeps the `agel-play` binary name.
Both example packages remain in the workspace so CI can exercise them.

The game examples consume the same Agel capabilities as other applications.
Game protocols, task policies, and scoring live in example code and its
test drivers. Use `sh scripts/test-osrs-wire.sh` for the offline wire
test or `sh scripts/test-osrs-login.sh` for the login and crypto fixtures.

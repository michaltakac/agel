# v0.3.125

The native optimizer can consume up to four available observations at a
time while it prepares recorded replay input. The example checks at most
once every three seconds during preparation. It continues servicing the
connection between checks.

The runner reports preparation and inference observations separately.
Measured replay still uses the original service callback. A hosted
fake-clock check verified the observation cadence and the existing pulse
calls. The live effect on disconnects remains to be verified.

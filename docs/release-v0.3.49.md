# v0.3.49 · Service the live connection during replay setup

Agel now reconstructs recorded replay input in bounded chunks. Between
chunks it services the existing game connection. The candidate still has
to pass text judgment and replay before activation.

The previous adaptive run disconnected before judgment. In the retry,
the guest kept the connection open through two real writer calls and two
real judge calls, with 21 keepalive writes during the optimization stage.
The text gate refused both proposals. The guest kept the fixed policy and
started the combat task.

[Native retry trace](evidence/v0.3.49-native-serviced-replay.txt)

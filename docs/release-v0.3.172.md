# v0.3.172

The RuneScape quest retries the castle pot pickup once if the inventory
is still empty and the ground observation still shows the pot. The
agent waits for inventory evidence after each request.

The interface fixture verified a successful retry and confirmed that
the agent stops after the first request when the pot is absent.

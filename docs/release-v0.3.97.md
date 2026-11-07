# v0.3.97

The RuneScape first-sale example records the observed NPC and route at each
tutor approach, before sending a talk action, and after observing the
response. The second approach now checks route observations before trying
again. Longer bounded observation windows give the client more time to
catch up, with the existing keepalive cadence.

The previous sequential crafting pair verified the platebody on both
accounts. Its adaptive arm retained the fixed policy after two refusals.
The native first-sale rerun is still pending.

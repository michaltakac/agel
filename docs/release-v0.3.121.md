# v0.3.121

The native Cook’s Assistant run now reports whether it saw and picked up the
castle pot. The same observer follows the mining and sale used to pay for the
bucket. Its tutor observations include the route, NPC, and counts of menu opens
and input advertisements. Before repeating a tutor interaction, the client
checks up to 128 already available packets for an actionable menu.

A regression test replays the fixed run’s exact menu-open, input, and close
packets. The menu becomes actionable after the input advertisement and stops
being actionable when it closes. The previous live pair failed in both arms.
The fixed arm reached the mining tutor. The adaptive arm committed a policy
change and then timed out during container acquisition. The native outcome of
the new observation path is pending.

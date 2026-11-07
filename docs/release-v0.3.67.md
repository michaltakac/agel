# v0.3.67 · Respond to the first observed quest dialogue

After talking to the Cook, the RuneScape example reads until it observes
a fresh dialogue page or reaches the tick limit. On a timeout it keeps
the latest packet and cipher state before trying a different approach.
The original strict wait API still reports a timeout as an error.

The fixture exercises an initial miss, a retry from a different tile,
and a page that becomes visible during the bounded read. Both native
quest modes now retain the dialogue packet and action trace.

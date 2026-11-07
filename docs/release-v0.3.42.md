# v0.3.42 · Record guest boot output and bound the bridge deadline

The native inference bridge keeps guest boot output in the run log.
The melee scorer compares the x86 frame pool reported by each guest when
that evidence is present.

`AGEL_GUEST_TIMEOUT_SECONDS` sets the bridge deadline from 1 to 3,600
seconds. The default is 900 seconds. The bridge reports a closed console
separately from an expired deadline.

A serial fixture confirmed that the bridge retained the frame-pool
report, stopped at a one-second deadline, and rejected an invalid
deadline before connecting.

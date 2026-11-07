# v0.3.207

The native optimizer distinguishes policy checks from live encoder
uses. On each live use, the guest counts the encoder service calls that
the active cadence avoids relative to cadence 64. This counts callbacks
and is separate from transport writes and elapsed time.

A 1 GiB guest fixture compared the counter with actual encoder callback
counts at packet sizes around cadence boundaries. It also verified that
checking the selected cadence did not increment either live counter.

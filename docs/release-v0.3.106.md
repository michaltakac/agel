# v0.3.106

The byte carrier can report whether a read has data ready without
consuming bytes. The Agel stream checks its buffered bytes before asking
the carrier.

The native adaptive economy client uses that check between inference
polls. It decodes up to four available packets per poll into the retained
game state while the existing keepalive service owns the outgoing cipher.
This addresses stale observations after an optimizer wait. The matched
native run with this change is pending.

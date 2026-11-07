# v0.3.108

The Cook's Assistant example waits through up to three bounded
observation windows for its first actionable dialogue page before
reissuing the talk action. Each window sends a keepalive and checks
against the original interface and pause-event counters.

The interface test replays the captured cook page and its separate
pause event. A scenario fixture verifies that a delayed page is used
without sending another talk. The native hand-in rerun is pending.

# v0.3.199

Agel's external repair driver accepts a continuation predicate. It
checks the predicate between transitions, so a caller can stop after
a proposal without sending a judge request, or after judgment without
replaying and activating the candidate. Stopping preserves the active
policy and leaves the pending repair available to resume.

The native combat example checks the task's observed state through this
driver. The scorer distinguishes a stop after inference started from a
zero-call preparation skip and counts any calls already made.

A 1 GiB native guest fixture stopped after one writer response, with
the judge still pending and policy version 1 still active.

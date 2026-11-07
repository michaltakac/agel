# v0.3.157

Agel agents can include `reply-to` in a live tool edit or revert request.
The tool actor replies with the gate's outcome, the tool name, and the
version currently active. Refusal, conflict, and rollback replies
include the recorded reason so the caller can revise its proposal.

The live-tool fixture covers a text-judge refusal, a successful retry,
and a failed revert whose replay leaves the active version in place.

# v0.3.118

Before retrying a cook interaction, the native client inspects up to
128 already available game packets against the original dialogue
counters. The same check follows a dialogue continuation. The stream
stops reading when the next actionable page arrives or the carrier has
no bytes ready.

Tests cover the delayed page without a second walk, a decoded ready
packet, and an idle carrier. The previous hand-in reached the first
page and sent the continuation before timing out. A native rerun of
the revised dialogue is queued.

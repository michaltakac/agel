# v0.3.57 · Change the approach when the cook does not respond

The Cook's Assistant example now tries distinct adjacent tiles before
retrying a talk request. The checkpoint trace includes the NPC index
and type for each request. The approach uses the cook spawn at
`3208, 3215` from the pinned private world's map.

The regression fixture withholds dialogue on the first request and
requires a different approach before accepting the retry. The live
native hand-in still needs to complete and pass the server-state check.

# v0.3.178

The offline kernel stager clears the previous candidate from the
boot selector before overwriting its slot. An interruption between
these writes leaves the trusted slot selected. The replacement still
requires admission, and the earlier archived version can be restaged.

An injected interruption verified the selector state and archived
restore at the write boundary.
A 1 GiB native guest then booted the trusted slot and readmitted the
archived candidate after it was restaged.

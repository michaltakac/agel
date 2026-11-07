# v0.3.184

The offline kernel stager distinguishes an empty selector from a malformed
one. It refuses unknown formats, truncated records, invalid slot numbers,
and candidates that name the trusted slot. This prevents an ambiguous
selector from being used to choose a kernel slot to overwrite.

Disk fixtures verified that malformed selectors leave the image unchanged
and an empty selector still permits first-time staging.
A native reboot fixture also verified interrupted replacement and
readmission of the archived candidate.

# v0.3.218

Agel can close the revision-241 bank modal and wait for both bank
interfaces to close before handing the connection back to crafting.
The private-world banking scenario now includes that step.

The wire fixture verified the encrypted close opcode and checked that
sending it leaves the observed interface open until the server replies.
A hosted fixture rejected an observation that closed only one bank
interface and accepted the observation that closed both.

# v0.3.82 · Verify activated artifacts before native dispatch

The native program and domain examples now dispatch through an adapter
that checks the installed program's start, length, and CRC against the
recorded activation artifact. The kernel checks the image CRC during
load. The existing callback-only interface still delegates validation to
the caller. The adapter check detects table mismatches before launch.

The fixture changed the selected row's CRC and confirmed that the
dispatcher refused it without calling the launcher.

The hosted bronze test also verified the furnace approach from v0.3.81.
A fresh normal account saved one bar, 35 Mining XP, and 6.2 Smithing XP.

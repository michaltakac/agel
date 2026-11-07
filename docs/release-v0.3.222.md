# v0.3.222

The native melee score now reports whether the server save meets the
XP and stance goal independently of the guest's correctness verdict.
A guest failure stays a failure even if the save shows that the
account reached the target.

A 1 GiB guest evaluated two retained saves. The fixed save met the
16 XP goal in all three skills and recorded Defence stance. The
other save reached Attack only. The local workspace suite passed.

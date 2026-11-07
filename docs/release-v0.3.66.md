# v0.3.66 · Verify checkpoint gate history

Agel reconstructs active versions from the checkpoint history. Each commit
must follow an allowed text judgment and a successful replay with at least
one recorded step. If the commit records activation, its passing result must
match the stored version. Recovery checks that each change starts from the
version active at the time and that the rebuilt pointers match the snapshot.
The QEMU checkpoint test recovered from a torn write, rejected an unsafe
revert, and saved a passing revert as a new version across two OS reboots.

RuneScape stays an example workload. Its three-skill melee harness now
checks that the final server snapshot preserves at least the XP observed
by the client. The fixed run saw 88 Defence XP and the server saved 128.
The stricter single-rat check still requires exact XP.

The fresh-account transfer finished with normal rights and 1× XP. It
replayed 4,614 bytes exactly, used the retained policy eight times and
made zero new model or judge calls. The fresh disk preserved the donor's
judge journal byte for byte.

# v0.3.86 · Pin the runtime for both economy arms

`python3 scripts/test-native-economy-pair.py` snapshots the boot image,
Agel executable, inference bridge, and Agel sources before launching
the fixed and adaptive arms. It keeps the artifact digests and logs,
and writes an Agel score input if the evidence passes the paired scorer.

The first run using pinned runtime binaries is underway. Its fixed
guest's boot regions and installed Agel executable match the snapshot.
The previous live cadence experiment ended after the text judge refused
its proposal, with no policy activation.

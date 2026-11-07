# v0.3.190

The native optimizer decodes its recorded tape with Agel's compact hex
decoder and reuses the recorded lengths for preparation and journal
writes. A fixture verified identical bytes and the same service
callback count in hosted Agel and a 1 GiB native guest.

The preceding donor scan change passed native recovery with identical
lineage checksums. Checking completion only at line boundaries reduced
the fixture from 7,218,334 to 7,085,467 evaluator steps.

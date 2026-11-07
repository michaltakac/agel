# v0.3.83 · Bind native spawn to the activated image

The Agel program and domain adapters now send the activated image's
start sector, length, and CRC with the spawn request. The kernel compares
them with the resolved table entry before loading the image. This covers
both activation canaries and subsequent version dispatch. The native
evolution scenario exercises a spawn with a mismatched CRC.

Use the matching kernel and Agel executable for checked dispatch. The
identity uses the existing CRC for accidental corruption detection.
Ordinary name-only spawn remains available.

# v0.3.168

Agel's byte stream encodes a single-byte write directly. Keepalives
and other one-byte messages avoid the chunk assembly path while
still validating the byte and requiring a one-byte acknowledgement.

Hosted byte-stream tests verify the emitted request and reject a
short acknowledgement.

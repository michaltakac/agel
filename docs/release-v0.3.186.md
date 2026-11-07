# v0.3.186

Agel's CRC-32 loop reads numeric nibble values directly from its
lookup table and calculates text length once per checksum. The
checksum and journal formats are unchanged.

The hosted journal framing suite verified that Agel and the native
journal implementation still agree.

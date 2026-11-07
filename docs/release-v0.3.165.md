# v0.3.165

The Agel byte stream checks read and entropy reply lengths before
decoding carrier payloads. Its compact hex decoder consumes pairs
directly and rejects malformed digits. The general hex decoder still
accepts whitespace for its existing callers.

Tests cover valid carrier responses, oversized replies, and malformed
compact hex.

The private-server cache and login challenge also passed inside Agel OS.

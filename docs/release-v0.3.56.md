# v0.3.56 · Trace native quest dialogue packets

The RuneScape hand-in checkpoint now records interface opens, closes,
resyncs, text updates, input permissions, and the continuation actions
Agel sends. Packet payloads in the private serial trace are bounded to
256 bytes. The scenario still runs on the normal account and the existing
native client.

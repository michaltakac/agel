# v0.3.103

A repair candidate approved by the text judge now runs its predicted,
displayed, and held-out cases before any external activation probe.
Agel writes that replay receipt to the audit. If replay fails, the
actor retains the current version and gives the writer the refusal
reason. A successful external probe still goes through the ordinary
version commit gate.

Tests verify that a bad candidate never calls the external probe and
that the audit orders recorded replay before the probe on success.

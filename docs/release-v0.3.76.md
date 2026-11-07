# v0.3.76 · Replay Agel code before kernel promotion

The x86 isolated workshop now replays a fixed Agel program on the
running, signature-admitted candidate before changing the trusted slot.
The replay runs in its own reset evaluator. It checks closure capture,
evaluation of quoted code, and arithmetic. A failed replay exhausts the
candidate's boot budget so the next boot returns to the trusted slot.

This connects semantic replay to the actual boot selector. The kernel
trial program ships with the signed kernel. Text judgment and the Agel
version history are still separate from this promotion command.

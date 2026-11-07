# v0.3.75 · Require trial activation for kernel-slot versions

Agel's version store now requires a passing activation receipt before
committing a kernel-slot edit or revert. A refusal preserves the active
version and records the reason for retry. Recovery also rejects a
kernel-slot commit without the corresponding activation event.

The Agel tests exercise successful activation, refusal, retry, revert,
and recovery of the resulting history with fixture trial receipts.
The BIOS slot selector still uses its existing admission and promotion
path. Connecting its trial boot to this Agel gate is pending.

A live native optimizer run produced two writer receipts totaling
37,520 input tokens, 24,576 cached, and 540 output tokens. Two judge
receipts totaled 83,034 nanodollars. The run ended after the proposed
policy failed the gate, and the writer dollar cost remains unknown.

# v0.3.158

When recorded replay finds a mismatch, the repair agent distinguishes a
failed prediction, a visible contract failure, and a held-out contract
failure in its next proposal prompt. The prompt includes the failure
category and reason without the held-out arguments or expected values.

Hosted tests cover the inline gate and the deferred replay path. A failed
deferred replay still prevents the external activation probe from running.

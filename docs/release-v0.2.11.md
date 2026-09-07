# Agel v0.2.11 — Native Agent Workbench

The native QEMU desktop now has a small interactive agent workbench. On a fresh
empty world, `:workbench` loads an Agel library with two clickable dock items,
keyboard focus and agent actions. Tab selects; Enter activates. Source inspection
opens in the native window with `:source 1`.

Preview a bounded behavior replacement and test message in a candidate world,
then promote or discard it. Failed test turns leave the live component intact.
Promotion preserves an explicit rollback point and does not rerun the candidate.
Edited source cells can be saved and reconstructed after reboot.

Four language primitives support scene identity/ownership/hit testing and
operator-boundary behavior replacement. Interaction policy remains ordinary Agel.
The pointer driver and fixed source panel remain native bootstrap scaffolding.

See [the workbench guide](native-workbench.md) for executable examples and limits.
The QEMU test generates `target/native-workbench.png` and checks actual mouse
input, keyboard focus, preview pixels, rejection, promotion, recovery and reboot.

This remains pre-production: scalar native actor state, shared evaluator globals,
bounded rounded-rectangle scenes, a seed ASCII font, and source rather than heap
persistence. No voice/model integration, general widget toolkit, filesystem
sandbox, formal correctness proof, or hardware-atomic display update is claimed.

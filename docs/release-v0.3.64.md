# v0.3.64 · Exercise macro-built tools inside Agel OS

The live-tool regression now calls the macro-built implementation
before reverting it. It checks that the existing actor dispatches
version 3, returns the recorded result, activates the guarded revert
as version 4, and retains call history and the original source.

The native evolution suite runs the same scenario in an Agel guest.
Its judge is a deterministic fixture.

[Native evaluation trace](evidence/v0.3.64-native-macro-tools.txt)

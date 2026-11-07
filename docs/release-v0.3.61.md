# v0.3.61 · Version isolated native domain launches

Agel can now edit, launch, and revert a named native domain generation
through the same text judgment and replay gates used for native programs.
Domain and program names occupy separate version namespaces. A generation
activates only after its compiled ELF canaries match the recorded results.

The native evaluation uses the OS launcher that gives each child a fresh
protection domain and no file authority. Switching generations changes
subsequent launches from the same controller. Migration of already running
processes remains open.

[Native evaluation trace](evidence/v0.3.61-native-domain.txt)

# v0.3.84 · Fail closed on unsupported checked spawn

Checked native dispatch now uses a dedicated kernel operation. An older
kernel refuses the operation, preventing a new Agel executable from
silently launching by name when it requested an image identity check.
Ordinary name-only spawn retains its existing operation. The new kernel
also accepts checked requests from the v0.3.83 executable.

QEMU confirmed that an older kernel refused both checked requests while
still launching by name. The current kernel accepted the matching
identity and refused the changed checksum.

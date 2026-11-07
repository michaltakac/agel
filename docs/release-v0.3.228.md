# v0.3.228

The native serial REPL conformance test now expects the kernel's
1,792-byte effect-text limit. The kernel uses a 2,048-byte effect
buffer and reserves 256 bytes for the payload header.

The stale expectation stopped main CI after the workspace tests
passed. The complete serial REPL test now passes locally under QEMU
on x86-64, AArch64, and RISC-V.

# v0.3.39 · Admit more guest memory on x86 QEMU

Agel now reads QEMU's x86 physical memory map through the firmware
configuration device. The frame allocator uses a contiguous RAM range
with a 256 MiB ceiling. The x86 process address window is 192 MiB.
Boots without QEMU firmware configuration use the legacy pool limit.

The same kernel booted with 64 MiB and 1 GiB of VM RAM. The frame pool
ended at 62 MiB and 256 MiB respectively. The 1 GiB guest wrote and
checked 80 MiB in a process. The 64 MiB guest refused that allocation
cleanly.

[Guest allocation evidence](evidence/v0.3.39-native-memory.txt)

[QEMU firmware interface](https://www.qemu.org/docs/master/specs/fw_cfg.html)

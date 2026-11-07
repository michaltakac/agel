# v0.3.77 · Pin the kernel replay at admission

During candidate admission, the trusted x86 kernel records the SHA-512
digest of its Agel trial. Promotion compares that digest to the running
candidate's trial before executing it. A mismatch exhausts the candidate's
boot budget, leaving the trusted slot selected for the next boot.

QEMU verified the digest on disk and rejected a signed, interactive
candidate with different trial bytes. A matching candidate still passed
replay and promoted.

Selectors admitted by an earlier release have no trial digest. Use a
v0.3.77 boot image as the trusted base before staging candidates with this
promotion gate. The selector's trial digest lives on the boot disk, within
the existing signed-kernel trust model.

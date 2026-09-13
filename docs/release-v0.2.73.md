# Agel v0.2.73 — Built where CI builds

A fix release. v0.2.71 and v0.2.72 passed every suite on the machine
that made them and failed the DOOM suite on CI, where the engine faulted
before its first frame. The cause was the toolchain, not the engine, and
the fix is proved on the runner's toolchain.

## What changed

- **The fault.** CI's clang 18 references data through the GOT
  (`add S_music@GOTPCREL(%rip), %r15`), and its lld 18, linking a static
  program that is not `-pie`, relaxes that to `add $S_music, %r15`: an
  absolute 32-bit immediate that cannot hold an address at 512 GiB and
  is silently truncated. The engine then touched the low 32 bits of the
  music table in `S_ChangeMusic`, right after `ST_Init`, and the kernel
  stopped its domain with the fault reported. Homebrew's newer lld keeps
  the load, which is why nothing failed here.
- **The fix.** `scripts/build-c-program.sh` compiles x86-64 C with
  `-fdirect-access-external-data`, so data is reached by a PC-relative
  `lea` with no GOT entry at all, and links with `--no-relax`, so the
  GOT entries that remain for function addresses stay loads on any lld.
- **The proof.** The runner's binary was reproduced in an Ubuntu 24.04
  container (clang 18.1.3, lld 18.1.3, linked through gcc as Ubuntu's
  clang does for this target), faulted on QEMU here at the same
  instruction, and with the flags plays the whole timed demo at 171
  frames per second. The same run showed the engine's last line printing
  a literal `%f`.
- **`printf` formats floating point:** `%f`, `%e`, `%g` and their
  capitals, with the flags, width and precision the other conversions
  take, rounding half up on the digit after the precision, `inf` and
  `nan` spelled, at most forty fraction digits. `float.c` has three more
  checks (23).
- **CI prints its clang, lld and QEMU versions** at the start of a run,
  so the next difference between the runner and a desk is in the log.

## Proof

`scripts/test-doom.sh` and `scripts/test-play.sh` on this machine, the
container-built binary by hand, `scripts/test-libc.sh` on all three
machines with the new checks, and the full regression. CI's own run on
this tag is the claim this release exists to make; the notes say so
before it has run, and the roadmap line cites it only once it has passed.

## Not claimed

The flags are proved by DOOM and the float program on the two toolchains
that build them, not by every clang. The C library is still not tested
against a conformance suite. Everything v0.2.72 did not claim still
holds: the loop that plays is Rust on the host, not an Agel agent in the
OS; that is the next rung.

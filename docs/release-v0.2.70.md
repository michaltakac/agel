# Agel v0.2.70 — Floating point

The fourth rung of *Does it run DOOM?*: the floating-point unit belongs
to processes, and the C library has what the port asks for.

## What changed

- **The unit is the process's.** The x86-64 kernel turns SSE on at
  bring-up and saves and restores each domain's x87 and SSE state around
  every entry; the AArch64 kernel enables the unit for EL0 and saves and
  restores the thirty-two SIMD registers and the two control words the
  same way. The supervisor is built without floating point and keeps no
  state. The RISC-V machine here has no unit and its programs stay
  soft-float.
- **C programs compile with the compiler's defaults** on x86-64 and
  AArch64; the x86-64 entry stub aligns the stack before calling the
  library, since vector stores assume a called function's alignment.
- **The library:** `math.h` (`fabs`, `sqrt`, `floor`, `ceil`, `fmod`,
  `atan`, `atan2`, `sin`, `cos`, integer `pow`), `strtod` and `atof` in
  C (the Rust side is soft-float and would return a double where a
  hard-float caller does not read it: the first run showed exactly
  that), `fseek`, `ftell`, `rewind`, `remove`, `strcasecmp`,
  `strncasecmp`, `strings.h`, `inttypes.h`, and `system`, which answers
  `ENOSYS`.

## Proof

`scripts/test-libc.sh` runs `float.c` on x86-64 and AArch64: twenty
checks of the unit, the math functions, `strtod`, seeking and telling in
a stream, `remove` and the case-insensitive comparisons. The full
regression passes, the desktop's C programs now built with SSE among
them.

## Not claimed

No floating point in `printf` or `scanf`; no `exp`, `log` or fractional
`pow`; RISC-V stays without a unit; DOOM does not run yet, its port is
the next rung.

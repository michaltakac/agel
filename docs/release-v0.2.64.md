# Agel v0.2.64 — A roadmap and a smaller tree

A roadmap that says what is done, partial and open, line by line, with the
test that proves each; five bugs fixed; the two MMU kernels' domain layer
made one; the test scripts sharing what they repeated.

![The native desktop at v0.2.63](images/native-desktop-v0.2.63.png)

## What changed

- **[`docs/roadmap.md`](roadmap.md)**, which the README points at: every
  line of the system marked done only when the code exists on every
  surface the line names and a test in the repository proves it, partial
  with exactly what is missing, open with no code; no dates, no ordering
  beyond dependence. Three documents that still said the POSIX layer did
  not exist are corrected.
- **Bugs fixed.** In the C library, `malloc` refused sizes whose rounding
  wrapped rather than handing out a small block; `realloc` trusts no
  header it has not found by walking the heap; `strtoul` parses unsigned
  values of its own and `strtol` sets `ERANGE`. In the native evaluator,
  a list or map whose chain ends in the empty cell is rendered and counted
  without indexing past the heap. The desktop's largest window is the
  maximized box (1920×920), so a resize event never announces a size the
  protocol calls impossible. The x86-64 clock says so when the PIT never
  signals during calibration instead of keeping a silent guess. The
  vector renderer places text with saturating arithmetic, an image
  reserves entries for what its bytes could hold rather than what a
  header claims, a provider's non-UTF-8 stderr no longer discards its
  answer, and the C formatter's widths and the scanner's numbers clamp
  instead of overflowing.
- **`errno.h`** names the ten error numbers the kernel already returned
  and C programs could not spell: `ESRCH`, `ECHILD`, `EAGAIN`, `EBUSY`,
  `EEXIST`, `ENODEV`, `ENFILE`, `ESPIPE`, `EPIPE`, `ENOTEMPTY`, and
  `ERANGE`.
- **One paged domain.** The AArch64 and RISC-V domain layers were 275
  identical lines each; `boot/kernel/src/arch/paged.rs` now holds the
  domain both build, run and reclaim, and each architecture keeps only its
  trap decode and its translation root. The user-text and read-only
  ranges, the six `create_*_world` bodies per machine, the form reader's
  continuation test, the file-descriptor prologue, the compositor's
  record emission and the asset header probe each exist once.
- **Scripts.** `scripts/lib.sh` prepares a machine and boots the
  headless x86-64 image for the ten suites that each spelled it out;
  `graphical_console.py` (renamed so it imports) carries the image
  preparation, pixel-region and reply-checking helpers six suites had
  copied; the serial harness dispatches its nine modes from a table. An
  unreferenced 437 KiB font is gone. CI lints the four configurations it
  boots but never linted (AArch64 `isolated-repl` and `native-graphics`
  with each board, RISC-V `isolated-repl`) and builds the desktop image
  once, explicitly, before the Python suites that take it.
- Dead public items removed from the supervisor and the standard
  library; one hex formatter in `agel-integrity`; the shared-page
  constants and error numbers declared once each in the kernel.

## Proof

Every kernel configuration CI lints is clean, the workspace tests,
clippy and docs pass, and the full regression of 74 suites on three
machines passes: the domain consolidation runs under every isolation,
process, file, C-library, spawn, breadth, persistence and power-cut
suite on AArch64 and RISC-V. The tree is 850 lines smaller.

## Not claimed

The roadmap records the state; it does not shorten the list. The world
digest that binds evidence to state is over a debug rendering, not a
canonical encoding, and the roadmap now says so.

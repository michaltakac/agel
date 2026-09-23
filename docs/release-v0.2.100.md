# Agel v0.2.100: room

A person asked why the cells and the kernel image were so small, and
whether the OS could be more robust for real work with more of both. The
answer was that neither limit was a principle: the image budget was the
real-mode megabyte the BIOS stage loaded into, and the cell, body,
request and text sizes were the native evaluator's first static tables.
This release is the room: a new disk layout with a 2 MiB kernel slot, the
kernel above the real-mode megabyte, and the evaluator's limits raised to
what a program needs. Every limit that moved is in the table below, with
what it was.

## What changed

- **Disk layout v3.** The kernel slots are 4096 sectors (2 MiB) each,
  past the data region: slot A at sector 65536, slot B at 69760; the
  image is 73,984 sectors (36 MiB). The workspace slots are 32 sectors
  each: A at 1024 as before, B at 1058; the recovery record (1056), the
  selector (1057), the filesystem, program, asset and data regions are
  where they were. Sectors 1–1023 are free. `scripts/build-boot.sh`
  writes the kernel at 65536 and rejects one over 2,097,152 bytes;
  `stage-kernel.py`, `run-graphics.sh` (the seed is sector 0 and slot A
  now) and the rollback test know the new sectors.
- **The BIOS stage loads above the megabyte.** It enters and leaves
  protected mode with the data segments' 4 GiB limits kept (unreal mode)
  and loads the slot in 33 conservative 127-sector transfers into a
  bounce buffer at `0x10000`, copying each chunk up to `0x100000` with
  32-bit addressing. The kernel links at 1 MiB, its `.bss` may reach
  4 MiB, the supervisor stack is 4–5 MiB, the frame pool starts at 6 MiB,
  and the kernel maps the first 6 MiB page by page (three tables) so the
  ring-3 text hole stays expressible. The stage is 507 of its 510 bytes:
  there was no room left for the disk-error message, so a failed read
  halts silently. One bug on the way: after the first chunk the copy
  left `esi` past the buffer and the next transfer read its packet from
  there, so every later chunk was chunk 0 again; a QEMU memory dump
  compared with the image found it in one look.
- **The evaluator's limits.** The shared page's payload moved past the
  handshake words (offset 640, 896 bytes; the block area at 1536); the
  evaluator's private stack is 16 MiB (a world is 636 KiB now, a session's
  three banks 1.9 MiB, and a preview holds copies beside them; a unit
  test guards that they fit with half the stack to spare). The fuel per
  form is 40,000.

| limit | before | now |
|---|---|---|
| kernel image | 260,096 bytes | 2,097,152 bytes |
| cell source | 256 bytes | 896 bytes |
| function body | 224 bytes | 1,024 bytes |
| bindings in a world | 96 | 160 |
| nodes in a form | 512 | 2,048 |
| text heap | 16 KiB | 64 KiB |
| result text | 256 bytes | 1,024 bytes |
| model request | 200 bytes | 1,000 bytes |
| effect reply | 2 KiB | 4 KiB |
| effect text | 1 KiB | 3 KiB |
| exec line | 200 bytes | 256 bytes |
| look line | 192 bytes | 255 bytes |
| parameters, locals, arguments | 6, 12, 12 | 8, 16, 16 |
| cell name | 24 bytes | 32 bytes |
| cells in the world | 40 | 64 |
| fuel per form | 10,000 | 40,000 |
| evaluator stack | 4 MiB | 8 MiB |
| x86-64 frame pool | 6–48 MiB | 6–62 MiB |
| AArch64 and RISC-V supervisor stack | 512 KiB | 4 MiB |
| workspace slot | 7.5 KiB | 15.5 KiB |

- **What the room broke, and the fixes.** The POSIX personality keeps
  its own copy of the shared page's offsets (`boot/posix/abi`), so a
  process hung at its first request until they moved too. The AArch64
  and RISC-V supervisor stacks were 512 KiB and the workshop keeps
  several workspace copies on them, 60 KiB each now, so the storage
  driver died at boot from a supervisor stack that had run into it;
  those stacks are 4 MiB now, like x86-64's megabyte was made for the
  same reason at v0.2.97. The workshop's `driver_text_error` prints on
  the supervisor's own console the error a stopped driver could not,
  which is how that one was read. The x86-64 frame pool ran out with a
  16 MiB evaluator stack beside the display's back buffer and DOOM's
  7 MiB zone (`the process window is full after 0 free pages`), so the
  evaluator's stack is 8 MiB (the guard still holds with half to spare)
  and the pool ends at 62 MiB of the 64 MiB machine instead of 48. And
  the x86 emitter in `agel/native-x86` writes a compiled program's output
  through the block area at a literal offset, which moved with the
  layout; a compiled `fib` printed NULs until it did.
- **Tests that named the old limits** moved with them: the `let` with
  too many bindings has seventeen now, the fuel-exhaustion case does four
  times the work, and the cell counts in the desktop tests are what the
  programs hold.

## What it costs

- A world is copied per form (three banks): 636 KiB instead of about
  80 KiB. Under TCG a step of the DOOM agent was not measurably slower
  in the live run; the number to watch is in the notes of the next
  release that measures it.
- A 36 MiB image instead of 32; a boot that reads 2 MiB instead of
  254 KiB, not noticeable under QEMU.
- The BIOS stage has three bytes to spare.

## Validation

- `scripts/test-boot.sh`, `test-native-repl.sh` (all three machines),
  `test-kernel-rollback.sh` (slot B at its new sector), `test-power-cut.sh`
  and `test-native-persistence.sh` (workspace slot A where it was, twice
  as large), the desktop suites, and the full local regression and CI.
- The evaluator's unit tests, with the guard on the session's size.

## Not claimed

- The desktop out of the supervisor: the third part of the "room"
  answer, a structural change with its own milestone. The image budget
  no longer decides what the OS can do, which is what this release is
  for.
- Any change to what the language or the desktop does; this is room,
  not features. The next release uses it.

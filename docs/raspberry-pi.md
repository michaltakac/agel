# Agel on the Raspberry Pi

The AArch64 research kernel is built for one board at a time, and a board
is a compile-time fact: `boot/kernel/src/arch/aarch64/board.rs` is the one
place the kernel names a physical address. QEMU's `virt` machine is the
default; the `board-raspi4` feature lays the kernel out for the Raspberry
Pi 4 (BCM2711), which QEMU models as `raspi4b`. The Pi 5 (BCM2712) shares
the Pi 4's boot shape and differs in its addresses, so the Pi 4 under QEMU
is the rehearsal for the Pi 5 on the bench.

## What a Pi presents, and what the kernel does about it (v0.2.55)

| | QEMU `virt` | Raspberry Pi 4 (QEMU `raspi4b`, and the board) |
|---|---|---|
| image | an ELF, loaded by its program headers at `0x4008_0000` | a flat `kernel8.img` at `0x80000`; the linker script takes the address from `AGEL_LOAD`, which the build script defines for the board |
| entry | EL1, core 0 | EL2, core 0, the other cores parked by the firmware; `agel_boot` drops to EL1 first |
| RAM | from `0x4000_0000` | from zero; the frame pool is 16 MiB to 64 MiB |
| console | PL011 at `0x0900_0000` | PL011 UART0 at `0xfe20_1000`, on GPIO 14 and 15 of the header |
| interrupts | GICv2 at `0x0800_0000` | GIC-400 at `0xff84_1000`; the timer's PPI 30 as before |
| devices | the first gibibyte | the last gibibyte below 4 GiB, holding the peripherals at `0xfc00_0000` and the GIC |
| disk | virtio-blk on MMIO | the card on an SD host controller, driven by programmed I/O (v0.2.56); without a card the workshop runs without a disk and says so |
| power off | PSCI | none without firmware at EL3: the kernel halts |

**The drop from EL2.** The entry reads `CurrentEL`; at EL2 it sets
`HCR_EL2.RW` so EL1 is AArch64, `CNTHCTL_EL2` so EL1 may use the physical
counter and timer, `CNTVOFF_EL2` to zero, `SCTLR_EL1` to its reset value
(MMU and caches off), `SPSR_EL2` to EL1h with interrupts masked, `ELR_EL2`
to the next instruction, and `eret`s. EL2 is left with no vectors; nothing
returns to it. The bring-up then insists on EL1 as before.

**What is proved.** `scripts/test-raspi4.sh` builds the flat image and
boots it on `raspi4b`: the kernel reports itself as `aarch64-raspi4`, the console driver
domain speaks through the PL011, the evaluator domain answers
`(+ 20 22)`, a function is defined and called, and `:exec` is refused with
`denied: no storage device`. Preemption runs through the GIC-400 the
whole time. The test skips itself where QEMU has no `raspi4b` (Ubuntu
24.04's QEMU 8.2 does not; QEMU 9 and later do), so CI records a skip,
not a pass.

## The card (v0.2.56)

The storage driver domain on the board is an SD Host Controller driver:
the same protocol every storage driver speaks (a sector in or out of the
block area, a status word, a yield) over the controller's registers,
which the domain sees as one granted page and nothing else. Every
register access is a whole 32-bit word, as the Pi's Arasan controller
requires; the 8- and 16-bit registers are reached through the word that
holds them.

Bring-up is the standard sequence: reset all, bus power at 3.3 V, the
internal clock at the identification divider, every interrupt status
recorded and none signalled (the driver polls, as the others do), then
`GO_IDLE_STATE`, `SEND_IF_COND` (a version-2 card answers `0x1aa`),
`APP_CMD` and `SD_SEND_OP_COND` until the card is ready (with the
high-capacity request when the card is version 2), `ALL_SEND_CID`,
`SEND_RELATIVE_ADDR`, `SEND_CSD` for the capacity (version 1 and version
2 CSDs both decoded), `SELECT_CARD`, then the transfer clock at base over
eight and 512-byte blocks. A sector is `READ_SINGLE_BLOCK` or
`WRITE_BLOCK`, addressed by sector on a high-capacity card and by byte on
a standard one, moved a word at a time through the buffer data port
between the buffer-ready and transfer-complete interrupts. A flush is
nothing to do: a write has completed before it is answered. A request
past the card's capacity is refused, and a controller that stops
answering is a timed-out request, not a lost driver.

The board names two controllers: EMMC2 at `0xfe34_0000`, where the Pi
4's slot is wired, and the first controller at `0xfe30_0000`, where
QEMU's model puts the card. The supervisor reads each one's present-state
register once at bring-up and grants the storage driver the page of the
one reporting a card; that is the only probe the kernel makes on a
board.

`scripts/test-raspi4.sh` now also makes a 64 MiB card (a power of two,
as QEMU's model requires; small enough to be a standard-capacity card,
so the byte-addressed path is the one tested), installs the `hello`
program on it, boots the board with the card, runs the program from the
card, stages and saves a cell, boots the board again on the same card and
requires the cell restored and the workspace clean.

## Toward the Pi 5

What the Pi 5 changes, from its datasheet and the Linux device tree:

| | Raspberry Pi 5 (BCM2712) |
|---|---|
| image | `kernel_2712.img`, the same flat shape; `config.txt` names it |
| console | the PL011 on the 3-pin debug connector at `0x10_7d00_1000`; UART0 on the header is on RP1 |
| interrupts | GIC-400 at `0x10_7fff_9000` (distributor), `0x10_7fff_a000` (CPU interface) |
| devices | the `0x10_0000_0000` window: the SoC's peripherals are above 4 GiB, so `TCR_EL1.IPS` (40 bits today) suffices but the device window moves to a top-level entry of its own |
| disk | SDHCI at `0x10_00ff_f000`, the same register set the driver already speaks |
| display | the firmware's framebuffer through the mailbox at `0x10_7c01_3880`, HDMI |
| device tree | passed in `x0` at entry; not read yet |

The order of work, each a release with a test: a `board-raspi5` layout
with the addresses above, which needs the board to verify; the mailbox
framebuffer, so the graphical workshop paints on HDMI; the device tree,
so one image serves both boards. The SD driver is done (v0.2.56) and
needs the board to confirm it drives the real controller, whose clock and
pins the firmware sets up before the kernel runs.

## Running on a Pi 4 today

```sh
./scripts/build-kernel.sh raspi4 --features isolated-repl
```

prints `boot/kernel/target/raspi4/kernel8.img`. Copy it to a FAT-formatted
card beside the firmware files (`start4.elf`, `fixup4.dat`, `bcm2711-rpi-4-b.dtb`)
with a `config.txt` of

```text
arm_64bit=1
enable_uart=1
kernel=kernel8.img
```

and read the console at 115200 baud on GPIO 14 and 15. The workspace and
the program region live on the same card, at the sectors
[`native-boot.md`](native-boot.md) gives, past the firmware's FAT
partition only if the card is laid out so; the simplest card for a first
try is one with the firmware files and nothing else, since the kernel
writes sectors 1024 and up, which a small FAT partition does not reach.
This has been run under QEMU only; the board is the next thing to try.

//! Where things are on the board: the one place the AArch64 kernel names a
//! physical address. QEMU's `virt` machine is the default; the
//! `board-raspi4` feature selects the Raspberry Pi 4's layout (BCM2711),
//! which QEMU also models and which shares its boot shape with the Pi 5:
//! a flat image at `0x80000`, RAM from zero, entry at EL2, no virtio.
//!
//! Nothing here is probed. A board is a compile-time fact, and a kernel
//! built for one board says so in its name rather than guessing on
//! another.

#[cfg(not(feature = "board-raspi4"))]
mod layout {
    pub const NAME: &str = "aarch64";
    /// Physical base of the device window: one gibibyte holding the
    /// UART, the interrupt controller and the virtio transports.
    pub const DEVICE_BASE: u64 = 0x0000_0000;
    /// Physical base of RAM.
    pub const RAM_BASE: u64 = 0x4000_0000;
    /// PL011 UART.
    pub const UART_BASE: u64 = 0x0900_0000;
    /// GICv2 distributor and CPU interface.
    pub const GIC_DISTRIBUTOR: u64 = 0x0800_0000;
    pub const GIC_CPU: u64 = 0x0801_0000;
    /// The frame pool: a fixed range above the image and its stack.
    pub const POOL_START: u64 = 0x4100_0000;
    pub const POOL_END: u64 = 0x4400_0000;
    /// The kernel's own text, for worlds that try to write it.
    pub const KERNEL_PROBE_ADDRESS: u64 = 0x4008_0000;
    /// The `virt` machine's virtio-mmio transports: 32 slots of 0x200 bytes.
    pub const VIRTIO_MMIO: Option<(u64, u64, u64)> = Some((0x0a00_0000, 32, 0x200));
    /// PSCI is how this machine is switched off.
    pub const PSCI: bool = true;
}

#[cfg(feature = "board-raspi4")]
mod layout {
    pub const NAME: &str = "aarch64-raspi4";
    /// The last gibibyte of the low 4 GiB: the peripherals at
    /// `0xfc00_0000` and the GIC-400 at `0xff84_0000` are in it.
    pub const DEVICE_BASE: u64 = 0xc000_0000;
    /// RAM starts at zero; the firmware places the image at `0x80000`.
    pub const RAM_BASE: u64 = 0x0000_0000;
    /// PL011 UART0, the debug console on the 40-pin header (GPIO 14/15).
    pub const UART_BASE: u64 = 0xfe20_1000;
    /// GIC-400 distributor and CPU interface.
    pub const GIC_DISTRIBUTOR: u64 = 0xff84_1000;
    pub const GIC_CPU: u64 = 0xff84_2000;
    /// The frame pool: 16 MiB to 64 MiB, above the image and its stack.
    pub const POOL_START: u64 = 0x0100_0000;
    pub const POOL_END: u64 = 0x0400_0000;
    pub const KERNEL_PROBE_ADDRESS: u64 = 0x0008_0000;
    /// No virtio: the SD card is an SDHCI controller, not yet driven.
    pub const VIRTIO_MMIO: Option<(u64, u64, u64)> = None;
    /// No PSCI without firmware at EL3: the machine is halted instead.
    pub const PSCI: bool = false;
}

pub use layout::*;

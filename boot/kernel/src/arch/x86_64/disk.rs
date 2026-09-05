//! Minimal primary-ATA PIO storage for the native source workspace.
//!
//! The BIOS seed already boots from QEMU's first IDE disk. v0.1.7 deliberately
//! uses only single-sector LBA28 operations and a bounded poll loop; there is
//! no probing, DMA, partition parser, filesystem, or ambient block API.

use super::hal;

const DATA: u16 = 0x1f0;
const SECTOR_COUNT: u16 = 0x1f2;
const LBA_LOW: u16 = 0x1f3;
const LBA_MID: u16 = 0x1f4;
const LBA_HIGH: u16 = 0x1f5;
const DRIVE: u16 = 0x1f6;
const STATUS_COMMAND: u16 = 0x1f7;
const ALTERNATE_STATUS: u16 = 0x3f6;

const STATUS_ERROR: u8 = 1 << 0;
const STATUS_DATA_REQUEST: u8 = 1 << 3;
const STATUS_DEVICE_FAULT: u8 = 1 << 5;
const STATUS_BUSY: u8 = 1 << 7;
const COMMAND_READ: u8 = 0x20;
const COMMAND_WRITE: u8 = 0x30;
const COMMAND_FLUSH: u8 = 0xe7;
const POLL_LIMIT: usize = 10_000_000;

pub fn read_disk_sector(lba: u32, sector: &mut [u8; 512]) -> Result<(), &'static str> {
    select(lba, COMMAND_READ)?;
    wait_for_data()?;
    for index in 0..256 {
        let word = unsafe { hal::in16(DATA) };
        let offset = index * 2;
        sector[offset..offset + 2].copy_from_slice(&word.to_le_bytes());
    }
    finish()
}

pub fn write_disk_sector(lba: u32, sector: &[u8; 512]) -> Result<(), &'static str> {
    select(lba, COMMAND_WRITE)?;
    wait_for_data()?;
    for index in 0..256 {
        let offset = index * 2;
        unsafe {
            hal::out16(
                DATA,
                u16::from_le_bytes([sector[offset], sector[offset + 1]]),
            )
        };
    }
    finish()
}

pub fn flush_disk() -> Result<(), &'static str> {
    wait_ready_for_command()?;
    unsafe { hal::out8(STATUS_COMMAND, COMMAND_FLUSH) };
    wait_not_busy()
}

fn select(lba: u32, command: u8) -> Result<(), &'static str> {
    if lba >= 1 << 28 {
        return Err("workspace sector is outside LBA28");
    }
    wait_ready_for_command()?;
    unsafe {
        hal::out8(DRIVE, 0xe0 | ((lba >> 24) as u8 & 0x0f));
        delay_400ns();
        hal::out8(SECTOR_COUNT, 1);
        hal::out8(LBA_LOW, lba as u8);
        hal::out8(LBA_MID, (lba >> 8) as u8);
        hal::out8(LBA_HIGH, (lba >> 16) as u8);
        hal::out8(STATUS_COMMAND, command);
    }
    Ok(())
}

/// Wait until a new command may be issued. A completed command can leave ERR
/// latched; only the next command clears it, so pre-command polling must not
/// turn that recoverable state into a permanent lockout.
fn wait_ready_for_command() -> Result<(), &'static str> {
    for _ in 0..POLL_LIMIT {
        let status = unsafe { hal::in8(STATUS_COMMAND) };
        if status == 0 || status == 0xff {
            return Err("workspace disk is absent");
        }
        if status & STATUS_BUSY == 0 {
            return Ok(());
        }
    }
    Err("workspace disk remained busy")
}

/// ATA requires at least 400 ns after selecting a drive. Four alternate-status
/// reads provide that delay without acknowledging a pending interrupt.
unsafe fn delay_400ns() {
    for _ in 0..4 {
        let _ = unsafe { hal::in8(ALTERNATE_STATUS) };
    }
}

fn wait_for_data() -> Result<(), &'static str> {
    for _ in 0..POLL_LIMIT {
        let status = unsafe { hal::in8(STATUS_COMMAND) };
        if status == 0 || status == 0xff {
            return Err("workspace disk is absent");
        }
        if status & STATUS_BUSY != 0 {
            continue;
        }
        if status & (STATUS_ERROR | STATUS_DEVICE_FAULT) != 0 {
            return Err("workspace disk reported an error");
        }
        if status & STATUS_DATA_REQUEST != 0 {
            return Ok(());
        }
    }
    Err("workspace disk data request timed out")
}

fn wait_not_busy() -> Result<(), &'static str> {
    for _ in 0..POLL_LIMIT {
        let status = unsafe { hal::in8(STATUS_COMMAND) };
        if status == 0 || status == 0xff {
            return Err("workspace disk is absent");
        }
        if status & STATUS_BUSY != 0 {
            continue;
        }
        if status & (STATUS_ERROR | STATUS_DEVICE_FAULT) != 0 {
            return Err("workspace disk reported an error");
        }
        return Ok(());
    }
    Err("workspace disk remained busy")
}

fn finish() -> Result<(), &'static str> {
    wait_not_busy()
}

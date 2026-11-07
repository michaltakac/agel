use super::{hal, POOL_END, POOL_START};

const SELECT: u16 = 0x510;
const DATA: u16 = 0x511;
const LEGACY_END: u64 = 62 * 1024 * 1024;
const HUGE_PAGE: u64 = 2 * 1024 * 1024;

fn select(item: u16) {
    unsafe { hal::out16(SELECT, item) };
}

fn read<const N: usize>() -> [u8; N] {
    core::array::from_fn(|_| unsafe { hal::in8(DATA) })
}

fn e820() -> Option<(u16, usize)> {
    select(0);
    if &read::<4>() != b"QEMU" {
        return None;
    }
    select(0x19);
    let entries = u32::from_be_bytes(read());
    for _ in 0..entries.min(1024) {
        let bytes = u32::from_be_bytes(read()) as usize;
        let item = u16::from_be_bytes(read());
        let _reserved = read::<2>();
        let name = read::<56>();
        if name.starts_with(b"etc/e820\0") {
            return (bytes > 0 && bytes <= 20 * 128 && bytes.is_multiple_of(20))
                .then_some((item, bytes / 20));
        }
    }
    None
}

fn region() -> (u64, u64, u32) {
    let start = u64::from_le_bytes(read());
    let size = u64::from_le_bytes(read());
    (
        start,
        start.saturating_add(size),
        u32::from_le_bytes(read()),
    )
}

pub fn pool_end() -> u64 {
    let Some((item, count)) = e820() else {
        return LEGACY_END;
    };
    select(item);
    let mut end = POOL_START;
    for _ in 0..count {
        let (start, stop, kind) = region();
        if kind == 1 && start <= POOL_START && stop > POOL_START {
            end = end.max(stop);
        }
    }
    select(item);
    for _ in 0..count {
        let (start, stop, kind) = region();
        if kind != 1 && stop > POOL_START && start < end {
            end = start.max(POOL_START);
        }
    }
    // Leave room at the reported RAM ceiling for firmware allocations.
    end.saturating_sub(HUGE_PAGE).min(POOL_END).max(POOL_START) & !(HUGE_PAGE - 1)
}

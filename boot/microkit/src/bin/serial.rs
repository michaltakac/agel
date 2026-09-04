//! The serial protection domain: the only holder of a device capability.
//!
//! It owns the PL011 and nothing else. It has no contract capability, no
//! children, and no way to reach either of the other domains except by being
//! called. If it faults, nothing else in the system is affected; the system
//! simply goes quiet, which is the correct failure for a console.

#![no_std]
#![no_main]

use agel_microkit::microkit::{Channel, MessageInfo};
use agel_microkit::serial::{Uart, RECOVERY_BUFFER_VADDR, UART_VADDR, WORLD_BUFFER_VADDR};

/// Channel the world prints on.
const WORLD: Channel = 0;
/// Channel the recovery domain prints on.
const RECOVERY: Channel = 1;

/// Safety: the device region is mapped for this domain by `agel.system`.
static UART: Uart = unsafe { Uart::new(UART_VADDR) };

#[no_mangle]
pub extern "C" fn init() {}

#[no_mangle]
pub extern "C" fn notified(_channel: Channel) {}

/// Print on behalf of a domain that has no device capability.
///
/// The label is the byte count the caller wants printed. It is untrusted input
/// from another protection domain, and is clamped rather than believed.
#[no_mangle]
pub extern "C" fn protected(channel: Channel, info: MessageInfo) -> MessageInfo {
    let buffer = match channel {
        WORLD => WORLD_BUFFER_VADDR,
        RECOVERY => RECOVERY_BUFFER_VADDR,
        // A channel this domain does not serve. Saying nothing is the whole
        // response: there is no error to report to a caller that should not
        // have been able to reach us.
        _ => return MessageInfo::new(0, 0),
    };
    // Safety: both pages are mapped readable for this domain.
    unsafe { UART.write_shared(buffer, info.label() as usize) };
    MessageInfo::new(0, 0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
}

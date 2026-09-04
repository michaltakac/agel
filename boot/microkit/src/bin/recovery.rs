//! The recovery protection domain: the plane the world cannot reach.
//!
//! It is the world's parent, so seL4 delivers the world's faults here rather
//! than letting them end the system. It holds no contract capability and runs
//! no Agel code; its whole job is to still be working after the world is not.
//!
//! This is the same recovery boundary the research kernel has, expressed in
//! seL4's own terms: the containment is a property of the system description
//! and the kernel, not of a supervisor loop this project wrote.

#![no_std]
#![no_main]

use core::fmt::Write as _;

use agel_microkit::microkit::{Channel, Child, MessageInfo};
use agel_microkit::serial::{Writer, RECOVERY_BUFFER_VADDR};

/// Channel to the serial domain.
const SERIAL: Channel = 0;

/// The address the world writes to when it has finished its work. A fault at
/// any other address means it went wrong somewhere else.
const COMPLETION_MARKER: u64 = 0xdead_0000;

/// `seL4_Fault_VMFault`. It is 6 rather than 5 because this is an MCS kernel,
/// where 5 is `seL4_Fault_Timeout`.
const VM_FAULT_LABEL: u64 = 6;
/// `seL4_VMFault_Addr`: the message register holding the faulting address.
const VM_FAULT_ADDRESS: usize = 1;

#[no_mangle]
pub extern "C" fn init() {}

#[no_mangle]
pub extern "C" fn notified(_channel: Channel) {}

/// Contain a child protection domain's fault.
///
/// Returning `false` means "do not reply", which leaves the faulting domain
/// stopped. That is the whole containment: the world is not resumed, the
/// kernel is untouched, and this domain keeps running.
#[no_mangle]
pub extern "C" fn fault(child: Child, info: MessageInfo, _reply: *mut MessageInfo) -> i8 {
    // Safety: the page and channel are declared in `agel.system`.
    let mut out = unsafe { Writer::new(RECOVERY_BUFFER_VADDR, SERIAL) };
    let label = info.label();
    // Safety: inside a fault entry point, the fault message is in the IPC
    // buffer.
    let address = unsafe { agel_microkit::microkit::message_register(VM_FAULT_ADDRESS) };

    if label == VM_FAULT_LABEL && address == COMPLETION_MARKER {
        let _ = writeln!(
            out,
            "recovery: world {child} finished and stopped at its completion marker"
        );
        let _ = writeln!(
            out,
            "recovery: contained it without replying; the world is not resumed"
        );
        let _ = writeln!(out, "AGEL_SEL4_OK");
    } else {
        let _ = writeln!(
            out,
            "recovery: contained world {child} fault label {label:#x} at {address:#x}"
        );
        let _ = writeln!(out, "AGEL_SEL4_FAILED: the world faulted unexpectedly");
    }
    out.flush();
    // Do not reply: the child stays stopped.
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
}

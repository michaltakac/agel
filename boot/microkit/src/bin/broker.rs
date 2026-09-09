//! The broker protection domain: the Agel kernel contract, in user space.
//!
//! This is the domain that exists because seL4 must not be modified. Every
//! capability, endpoint, notification and clock the contract defines lives
//! here, in an ordinary thread with an ordinary address space, and is reached
//! by an ordinary protected procedure call.
//!
//! Since v0.2.25 it answers with [`IndependentKernel`], the second
//! implementation of the contract, written from the contract document and the
//! corpus rather than from the reference model. The research kernels keep the
//! reference model behind their trap gates; this domain runs the other one
//! behind a boundary enforced by a kernel Agel did not write. When CI requires
//! the seL4 transcript to match the frozen one, it is now comparing two
//! implementations, not one implementation behind two boundaries.

#![no_std]
#![no_main]

use agel_kernel_abi::independent::IndependentKernel;
use agel_microkit::microkit::{Channel, MessageInfo};
use agel_microkit::protocol;

/// The broker's object table.
///
/// A protection domain is one seL4 thread and Microkit dispatches its entry
/// points one at a time, so this is only ever reached from one place at a time.
/// It is reached through a raw pointer rather than a reference so that no
/// `&mut` to a mutable static is ever formed.
static mut OBJECTS: Option<IndependentKernel> = None;

#[no_mangle]
pub extern "C" fn init() {
    // Safety: `init` runs once, before any entry point that reads this.
    unsafe { OBJECTS = Some(IndependentKernel::new()) };
}

#[no_mangle]
pub extern "C" fn notified(_channel: Channel) {}

/// Answer one contract invocation for whichever domain called.
#[no_mangle]
pub extern "C" fn protected(_channel: Channel, info: MessageInfo) -> MessageInfo {
    let table = &raw mut OBJECTS;
    // Safety: Microkit runs one entry point at a time in a protection domain
    // that has one thread, and `init` has already run.
    let Some(objects) = (unsafe { (*table).as_mut() }) else {
        // `init` has not run, which cannot happen. Refuse rather than invent
        // an answer.
        return MessageInfo::new(
            u64::from(agel_kernel_abi::Status::ResourceExhausted.code()),
            0,
        );
    };
    // Safety: called from inside a `protected` entry point.
    unsafe { protocol::answer(objects, info) }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
}

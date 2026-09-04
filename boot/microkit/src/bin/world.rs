//! The Agel world protection domain.
//!
//! It holds no device capability, no children, and no authority over anything
//! except two channels: one to ask the broker a contract question, one to ask
//! the serial domain to print. It cannot reach the broker's object table, the
//! UART, the recovery domain, or the kernel.
//!
//! What it runs is the *shared* conformance driver over the *shared* corpus,
//! through a [`BrokerKernel`] whose `invoke` is an seL4 protected procedure.
//! Every other Agel backend runs the same two functions over the same data, so
//! the transcript this domain prints is comparable to theirs byte for byte.
//!
//! It finishes by faulting on purpose, which is how the recovery domain gets
//! the last word.

#![no_std]
#![no_main]

use core::fmt::Write as _;

use agel_kernel_abi::{conformance, write_step, Kernel};
use agel_microkit::microkit::{self, Channel};
use agel_microkit::protocol::BrokerKernel;
use agel_microkit::serial::{Writer, WORLD_BUFFER_VADDR};

/// Channel to the serial domain.
const SERIAL: Channel = 0;
/// Channel to the broker.
const BROKER: Channel = 1;

/// Address the world writes to when it has finished, so the recovery domain's
/// fault report can distinguish "done" from "went wrong". It is inside the
/// canonical range and outside every region `agel.system` maps into this
/// domain, so the write is a plain unmapped-address fault.
const COMPLETION_MARKER: u64 = 0xdead_0000;

#[no_mangle]
pub extern "C" fn init() {
    // Safety: the page and channel are declared in `agel.system`.
    let mut out = unsafe { Writer::new(WORLD_BUFFER_VADDR, SERIAL) };
    let mut broker = BrokerKernel::new(BROKER);

    let _ = writeln!(
        out,
        "world: asking the broker for the contract it implements"
    );

    let mut agreed = 0_usize;
    let _ = writeln!(out, "---BEGIN AGEL CONTRACT TRANSCRIPT---");
    let _ = writeln!(
        out,
        "agel-kernel-contract v{}.{}.{} corpus={} steps",
        agel_kernel_abi::VERSION_MAJOR,
        agel_kernel_abi::VERSION_MINOR,
        agel_kernel_abi::VERSION_PATCH,
        conformance::CORPUS.len()
    );
    broker.reset_to_conformance_domain();
    for step in conformance::CORPUS {
        let observed = broker.invoke(&step.request);
        let _ = write_step(&mut out, step.label, &step.request, &observed);
        agreed += 1;
    }
    let _ = writeln!(out, "---END AGEL CONTRACT TRANSCRIPT---");
    let _ = writeln!(out, "world: {agreed} invocations answered by the broker");

    // The corpus checks properties the transcript alone would not: that
    // authority is never widened, that revoked handles fail closed, that a
    // bounded queue pushes back. Running them here means the seL4 backend is
    // held to the same invariants as every other, not merely to the same bytes.
    match conformance::check_invariants(&mut broker) {
        Ok(()) => {
            let _ = writeln!(out, "world: contract invariants hold across the boundary");
        }
        Err(failure) => {
            let _ = writeln!(out, "AGEL_SEL4_FAILED: {failure}");
        }
    }
    out.flush();

    // Microkit has no exit. Faulting at a known address is how this domain
    // hands control to its parent, and the address says it got here on purpose.
    // Safety: does not return.
    unsafe { microkit::fault_deliberately(COMPLETION_MARKER) };
}

#[no_mangle]
pub extern "C" fn notified(_channel: Channel) {}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
}

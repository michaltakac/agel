//! The per-architecture half of the research kernel.
//!
//! Exactly one of these modules compiles, so the shared code above calls the
//! selected architecture's inherent items directly and pays nothing for the
//! choice. The interface every architecture must provide is small on purpose:
//!
//! ```text
//! NAME                     a short label for serial reports
//! POOL_START / POOL_END    the fixed physical frame range
//! KERNEL_PROBE_ADDRESS     a supervisor-only address a world may try to write
//! console_initialize       prepare the serial device
//! console_write_byte       emit one byte
//! exit(success)            leave the emulator, if it can be left
//! user_text_range          bounds of the user-executable section
//! fault_name(cause)        the shared name for a trap cause
//! PROVOCATIONS             the misbehaviours this architecture can produce
//! Machine::bring_up        address spaces, traps, preemption
//! Machine::create_world    a protection domain entered unprivileged
//! Domain::{invoke_in_world, provoke, run, stopped}
//! ```
//!
//! Anything larger than that would mean the shared driver had started to depend
//! on a particular machine, and the contract would no longer be the boundary.

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;
#[cfg(target_arch = "riscv64")]
pub use riscv64::*;
#[cfg(target_arch = "x86_64")]
pub use x86_64::*;

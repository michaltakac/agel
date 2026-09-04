//! The Agel kernel contract as a Microkit system on an unmodified seL4 kernel.
//!
//! This is the assurance backend the research kernel exists to be compared
//! against. The difference is the whole point: on x86-64, AArch64 and RISC-V the
//! research kernel *is* the privileged code, and Agel is trusting code Agel
//! wrote. Here the privileged code is a kernel Agel did not write, did not
//! modify, and could not modify without giving up the reason for choosing it.
//!
//! Nothing in this crate runs privileged. Every protection domain below is an
//! ordinary seL4 thread in its own address space, holding exactly the
//! capabilities the system description in `agel.system` gave it, and reaching
//! everything else through kernel-mediated IPC.
//!
//! What is deliberately *not* here: any Agel concept inside the kernel. The
//! contract is answered by an ordinary server protection domain, exactly as
//! `docs/microkernel-research.md` requires — "do not fork seL4 to add Lisp
//! objects, mailboxes, policy, or dynamic agent semantics. Those belong in
//! isolated servers."

#![no_std]
#![deny(missing_docs)]

pub mod microkit;
pub mod protocol;
pub mod serial;

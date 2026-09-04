//! The Agel kernel contract carried over an seL4 protected procedure.
//!
//! On the research kernel a contract invocation is a trap into the kernel,
//! because there the kernel answers it. On seL4 the kernel knows nothing about
//! Agel and must not be taught: the contract is answered by an ordinary server
//! protection domain, and an invocation is a call to that server.
//!
//! The encoding is chosen so the whole invocation fits in the four message
//! registers seL4 passes in hardware registers on AArch64. The message label is
//! 52 bits, so the operation code and the capability slot travel there, leaving
//! all four registers for the contract's four bounded argument words.
//!
//! ```text
//! request   label = operation | (capability << 16)     words = arguments
//! reply     label = status                             words = values
//! ```

use agel_kernel_abi::{Operation, Request, Response, Status, WORDS};

use crate::microkit::{self, Channel, MessageInfo, REGISTER_MESSAGE_WORDS};

const _: () = assert!(WORDS == REGISTER_MESSAGE_WORDS);

/// Label meaning "rebuild the conformance capability space".
///
/// The contract has no reset operation, and should not: rebuilding a capability
/// space is a system-construction act, not something a domain asks for. This is
/// a broker convention for the conformance harness, placed above every value a
/// packed `operation | capability << 16` can produce so the two can never be
/// confused.
pub const RESET_LABEL: u64 = 1 << 48;

/// Pack an invocation into a message label.
pub const fn request_label(operation: u16, capability: u32) -> u64 {
    (operation as u64) | ((capability as u64) << 16)
}

/// Unpack an invocation from a message label.
pub const fn unpack_request_label(label: u64) -> (u16, u32) {
    ((label & 0xffff) as u16, (label >> 16) as u32)
}

/// The client half: a [`Kernel`](agel_kernel_abi::Kernel) whose every
/// invocation is a protected procedure call to the broker.
///
/// This is what makes the seL4 backend comparable to the others rather than
/// merely similar. The world runs the *same* `conformance::transcribe` over the
/// *same* corpus; only the implementation of one method differs, and that
/// method is a system call into a kernel Agel did not write.
pub struct BrokerKernel {
    channel: Channel,
}

impl BrokerKernel {
    /// Address the broker on `channel`.
    pub const fn new(channel: Channel) -> Self {
        Self { channel }
    }
}

impl agel_kernel_abi::Kernel for BrokerKernel {
    fn invoke(&mut self, request: &Request) -> Response {
        let info = MessageInfo::new(
            request_label(request.operation.code(), request.capability),
            WORDS as u16,
        );
        // Safety: the world's protected-procedure end of this channel is
        // declared in `agel.system`.
        let (reply, values) =
            unsafe { microkit::protected_call(self.channel, info, request.arguments) };
        // The reply is whatever the broker sent. It is another protection
        // domain's output, so it is validated rather than trusted: an
        // unrecognised status becomes a recognised failure.
        match Status::from_code(reply.label() as u16) {
            Some(Status::Ok) => Response::ok(values),
            Some(status) => Response::fail(status),
            None => Response::fail(Status::InvalidOperation),
        }
    }

    fn reset_to_conformance_domain(&mut self) {
        let info = MessageInfo::new(RESET_LABEL, 0);
        // Safety: as above.
        unsafe { microkit::protected_call(self.channel, info, [0; WORDS]) };
    }
}

/// The server half: answer one protected procedure call against `kernel`.
///
/// Returns the reply message info; the message registers are written in place.
///
/// # Safety
/// Only correct inside a `protected` entry point.
pub unsafe fn answer<K: agel_kernel_abi::Kernel>(kernel: &mut K, info: MessageInfo) -> MessageInfo {
    if info.label() == RESET_LABEL {
        kernel.reset_to_conformance_domain();
        return MessageInfo::new(u64::from(Status::Ok.code()), 0);
    }

    let (operation, capability) = unpack_request_label(info.label());
    let Some(operation) = Operation::from_code(operation) else {
        return MessageInfo::new(u64::from(Status::InvalidOperation.code()), 0);
    };
    if info.count() as usize != WORDS {
        // A caller that sends the wrong number of words is not asking a
        // question the contract defines.
        return MessageInfo::new(u64::from(Status::InvalidArgument.code()), 0);
    }
    let mut arguments = [0_u64; WORDS];
    for (index, word) in arguments.iter_mut().enumerate() {
        *word = unsafe { microkit::message_register(index) };
    }

    let response = kernel.invoke(&Request::with(operation, capability, arguments));
    for (index, value) in response.values.iter().enumerate() {
        unsafe { microkit::set_message_register(index, *value) };
    }
    MessageInfo::new(u64::from(response.status.code()), WORDS as u16)
}

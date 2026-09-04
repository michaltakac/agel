//! The seL4 and Microkit binding, written directly rather than bound from C.
//!
//! The SDK's `microkit.h` and `libsel4` express `microkit_ppcall`,
//! `microkit_notify` and the message-register accessors as `static inline` C, so
//! there is nothing in `libmicrokit.a` to call for them: a Rust protection
//! domain has to issue the system call itself. That is a small amount of code,
//! and writing it out makes the whole privileged interface of these domains
//! visible in one file.
//!
//! `libmicrokit.a` still supplies the parts that are real code: `_start`, the
//! `main` event loop that dispatches to [`init`](crate) / `notified` /
//! `protected` / `fault`, and the IPC buffer pointer this module reads.
//!
//! Everything here is specific to AArch64 and to the kernel configuration the
//! SDK ships for `qemu_virt_aarch64`, which is an **MCS** configuration. The
//! system call convention below passes the reply capability in `x6` because of
//! that.

use core::arch::asm;

/// A Microkit channel identifier.
pub type Channel = u32;
/// A Microkit child protection-domain identifier.
pub type Child = u32;

/// First capability slot of the outgoing notification range.
const BASE_OUTPUT_NOTIFICATION_CAP: u64 = 10;
/// First capability slot of the outgoing endpoint range, used by protected
/// procedure calls.
const BASE_ENDPOINT_CAP: u64 = 74;

/// `seL4_SysCall`, sign-extended as the kernel expects it in `x7`.
const SYS_CALL: u64 = -1_i64 as u64;
/// `seL4_SysSend`.
const SYS_SEND: u64 = -5_i64 as u64;

/// Message registers carried in registers rather than through the IPC buffer.
///
/// AArch64 seL4 passes the first four in `x2`–`x5`. The Agel contract is
/// designed to fit inside them: four bounded words, with the operation and the
/// capability packed into the message label. A control path that never touches
/// the IPC buffer is a control path with no shared-memory step to get wrong.
pub const REGISTER_MESSAGE_WORDS: usize = 4;

/// A packed `seL4_MessageInfo`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct MessageInfo(u64);

impl MessageInfo {
    /// Build a message info from a label and a word count.
    ///
    /// The label is 52 bits, which is the whole reason the contract's operation
    /// code and capability slot can travel outside the message registers.
    pub const fn new(label: u64, count: u16) -> Self {
        Self(((label & 0x000f_ffff_ffff_ffff) << 12) | ((count as u64) & 0x7f))
    }

    /// The 52-bit label.
    pub const fn label(self) -> u64 {
        self.0 >> 12
    }

    /// The number of message registers.
    pub const fn count(self) -> u16 {
        (self.0 & 0x7f) as u16
    }

    /// The raw word, as the kernel and `libmicrokit` exchange it.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Rebuild from a raw word received from the kernel.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

#[repr(C)]
struct IpcBuffer {
    tag: u64,
    msg: [u64; 120],
    // The remaining fields are not used by these domains.
}

extern "C" {
    /// Set up by `libmicrokit`'s `main` before it calls any entry point.
    static __sel4_ipc_buffer: *mut IpcBuffer;
}

/// Read one message register a caller left in the IPC buffer.
///
/// A server reads its arguments here because `libmicrokit`'s receive path
/// spills the register message into the IPC buffer before dispatching.
///
/// # Safety
/// Only correct inside a `protected` or `notified` entry point, with `index`
/// below the count the message info reported.
pub unsafe fn message_register(index: usize) -> u64 {
    unsafe { (*__sel4_ipc_buffer).msg[index & 0x7f] }
}

/// Write one message register for the reply.
///
/// # Safety
/// Only correct inside a `protected` entry point, with `index` below
/// [`REGISTER_MESSAGE_WORDS`].
pub unsafe fn set_message_register(index: usize, value: u64) {
    unsafe { (*__sel4_ipc_buffer).msg[index & 0x7f] = value };
}

/// Make a protected procedure call on `channel`.
///
/// This is a bounded synchronous call in the contract's sense: the caller
/// blocks, the callee runs on the caller's donated budget, and the message is a
/// fixed number of words. seL4 enforces that the callee's priority is at least
/// the caller's, so the dependency is explicit in the system description rather
/// than discovered at run time.
///
/// # Safety
/// `channel` must be one this protection domain was given a protected-procedure
/// end of in `agel.system`.
pub unsafe fn protected_call(
    channel: Channel,
    info: MessageInfo,
    arguments: [u64; REGISTER_MESSAGE_WORDS],
) -> (MessageInfo, [u64; REGISTER_MESSAGE_WORDS]) {
    let mut destination = BASE_ENDPOINT_CAP + u64::from(channel);
    let mut reply_info = info.raw();
    let mut word0 = arguments[0];
    let mut word1 = arguments[1];
    let mut word2 = arguments[2];
    let mut word3 = arguments[3];
    unsafe {
        asm!(
            "svc #0",
            inout("x0") destination,
            inout("x1") reply_info,
            inout("x2") word0,
            inout("x3") word1,
            inout("x4") word2,
            inout("x5") word3,
            // MCS: the reply capability. Zero means "use the caller's".
            in("x6") 0_u64,
            in("x7") SYS_CALL,
            options(nostack),
        )
    };
    let _ = destination;
    (
        MessageInfo::from_raw(reply_info),
        [word0, word1, word2, word3],
    )
}

/// Signal the notification bound to `channel`.
///
/// # Safety
/// `channel` must be one this protection domain has a notification end of.
pub unsafe fn notify(channel: Channel) {
    let destination = BASE_OUTPUT_NOTIFICATION_CAP + u64::from(channel);
    let info = MessageInfo::new(0, 0).raw();
    unsafe {
        asm!(
            "svc #0",
            in("x0") destination,
            in("x1") info,
            in("x2") 0_u64,
            in("x3") 0_u64,
            in("x4") 0_u64,
            in("x5") 0_u64,
            in("x6") 0_u64,
            in("x7") SYS_SEND,
            options(nostack),
        )
    };
}

/// Stop this protection domain by taking a fault its parent will see.
///
/// Microkit offers no "exit"; a domain that has finished either blocks forever
/// in the event loop or stops. Faulting deliberately at a known address is how
/// this system hands the last word to its recovery domain, and the address is
/// chosen so the report is unambiguous about which it was.
///
/// # Safety
/// Does not return.
pub unsafe fn fault_deliberately(marker: u64) -> ! {
    unsafe { (marker as *mut u64).write_volatile(0) };
    // The write above always faults; the loop is only here to satisfy `!`.
    loop {
        unsafe { asm!("wfe", options(nomem, nostack)) };
    }
}

/// Symbols `libmicrokit`'s event loop requires from every protection domain.
///
/// `protected` and `fault` are weak in the library, so a domain defines only
/// the entry points it actually uses. This macro exists so that the two a
/// domain never uses are still obviously *decided* rather than forgotten.
#[macro_export]
macro_rules! microkit_entry_points {
    (init = $init:expr, notified = $notified:expr $(,)?) => {
        /// Microkit calls this once, after the domain's memory is mapped.
        #[no_mangle]
        pub extern "C" fn init() {
            $init()
        }

        /// Microkit calls this when a notification this domain waits on fires.
        #[no_mangle]
        pub extern "C" fn notified(channel: $crate::microkit::Channel) {
            $notified(channel)
        }
    };
}

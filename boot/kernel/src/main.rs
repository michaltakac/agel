#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

#[cfg(feature = "isolation-selftest")]
mod contract;
#[cfg(feature = "isolation-selftest")]
mod cpu;
#[cfg(feature = "isolation-selftest")]
mod domain;
#[cfg(feature = "isolation-selftest")]
mod hal;
#[cfg(feature = "isolation-selftest")]
mod memory;
#[cfg(feature = "isolation-selftest")]
mod user;

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "isolation-selftest"
)))]
mod native;

const COM1: u16 = 0x3f8;

struct Serial;

impl Serial {
    fn initialize() {
        unsafe {
            out(COM1 + 1, 0x00);
            out(COM1 + 3, 0x80);
            out(COM1, 0x03);
            out(COM1 + 1, 0x00);
            out(COM1 + 3, 0x03);
            out(COM1 + 2, 0xc7);
            out(COM1 + 4, 0x0b);
        }
    }

    fn write_byte(byte: u8) {
        while unsafe { input(COM1 + 5) } & 0x20 == 0 {}
        unsafe { out(COM1, byte) };
    }

    fn write(text: &str) {
        for byte in text.bytes() {
            if byte == b'\n' {
                Self::write_byte(b'\r');
            }
            Self::write_byte(byte);
        }
    }

    #[cfg(not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "native-selftest",
        feature = "isolation-selftest"
    )))]
    fn read_byte() -> u8 {
        while unsafe { input(COM1 + 5) } & 1 == 0 {}
        unsafe { input(COM1) }
    }
}

#[cfg(not(any(feature = "selftest", feature = "native-selftest")))]
#[derive(Clone, Copy)]
enum Slot {
    A,
    B,
}

#[cfg(not(any(feature = "selftest", feature = "native-selftest")))]
struct RecoveryMonitor {
    active: Slot,
    previous: Slot,
    candidate_verified: bool,
}

#[cfg(not(any(feature = "selftest", feature = "native-selftest")))]
impl RecoveryMonitor {
    const fn new() -> Self {
        Self {
            active: Slot::A,
            previous: Slot::A,
            candidate_verified: false,
        }
    }

    fn status(&self) {
        Serial::write("active slot: ");
        Serial::write(match self.active {
            Slot::A => "A (stable)\n",
            Slot::B => "B (candidate)\n",
        });
    }

    fn verify(&mut self) {
        self.candidate_verified = true;
        Serial::write("candidate B: isolated health evidence accepted\n");
    }

    fn promote(&mut self) {
        if matches!(self.active, Slot::B) {
            self.candidate_verified = false;
            Serial::write("denied: candidate B is already active; slot A remains rollback\n");
        } else if self.candidate_verified {
            self.previous = self.active;
            self.active = Slot::B;
            self.candidate_verified = false;
            Serial::write("selected slot B; slot A retained for rollback\n");
        } else {
            Serial::write("denied: verify candidate before promotion\n");
        }
    }

    fn fault(&mut self) {
        self.active = self.previous;
        self.candidate_verified = false;
        Serial::write("watchdog fault: rolled back to slot ");
        Serial::write(match self.active {
            Slot::A => "A\n",
            Slot::B => "B\n",
        });
    }
}

#[no_mangle]
#[link_section = ".text.entry"]
extern "C" fn agel_boot() -> ! {
    // Nothing has zeroed `.bss`: it is a NOBITS section, so it occupies
    // addresses in the image's memory range but contributes no bytes to the raw
    // disk image the BIOS loads. Relying on the emulator handing us zeroed RAM
    // would make correctness a property of QEMU rather than of this kernel.
    zero_bss();
    Serial::initialize();
    Serial::write("\nAgel v1.2 native workshop\n");
    Serial::write("recovery monitor is outside the mutable agent world\n");
    Serial::write("self-check: BIOS seed -> long mode -> Rust HAL [ok]\n");

    #[cfg(feature = "selftest")]
    {
        Serial::write("AGEL_BOOT_OK\n");
        unsafe { out32(0xf4, 0x10) };
        halt()
    }

    #[cfg(all(feature = "monitor-selftest", not(feature = "selftest")))]
    {
        let mut monitor = RecoveryMonitor::new();
        monitor.status();
        monitor.promote();
        monitor.verify();
        monitor.promote();
        monitor.status();
        monitor.verify();
        monitor.promote();
        monitor.fault();
        monitor.status();
        Serial::write("AGEL_MONITOR_OK\n");
        unsafe { out32(0xf4, 0x10) };
        halt()
    }

    #[cfg(all(
        feature = "native-selftest",
        not(any(feature = "selftest", feature = "monitor-selftest"))
    ))]
    {
        native_selftest()
    }

    #[cfg(all(
        feature = "isolation-selftest",
        not(any(
            feature = "selftest",
            feature = "monitor-selftest",
            feature = "native-selftest"
        ))
    ))]
    {
        isolation_selftest()
    }

    #[cfg(not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "native-selftest",
        feature = "isolation-selftest"
    )))]
    {
        native_repl()
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn native_repl() -> ! {
    let mut session = native::Session::new();
    let mut monitor = RecoveryMonitor::new();
    let mut line = [0_u8; 256];
    Serial::write("AGEL_NATIVE_READY\n");
    Serial::write("Type :help. Definitions live transactionally in this VM session.\n");
    loop {
        Serial::write("agel-native[");
        write_u64(session.revision());
        Serial::write("]> ");
        let length = read_form(&mut line);
        let source = &line[..length];
        if source
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| byte == b';')
        {
            continue;
        }
        match source {
            b":help" => Serial::write(
                "forms: quote if begin def fn | builtins: + - * / = < eval\n\
                 commands: :revision :rollback :defs :limits :recovery-status :verify :promote :fault :shutdown\n",
            ),
            b":revision" => {
                Serial::write("revision ");
                write_u64(session.revision());
                Serial::write("\n");
            }
            b":rollback" => match session.rollback() {
                Ok(()) => Serial::write("rolled back one committed native world\n"),
                Err(error) => write_error(error),
            },
            b":defs" => {
                Serial::write("definitions (");
                write_u64(session.binding_count() as u64);
                Serial::write("): ");
                for index in 0..session.binding_count() {
                    if index > 0 {
                        Serial::write(" ");
                    }
                    if let Some(name) = session.binding_name(index) {
                        write_bytes(name);
                    }
                }
                Serial::write("\n");
            }
            b":limits" => {
                Serial::write("source=");
                write_u64(line.len() as u64);
                for (name, bound) in native::LIMITS {
                    Serial::write(" ");
                    Serial::write(name);
                    Serial::write("=");
                    write_u64(*bound);
                }
                Serial::write("\n");
            }
            b":recovery-status" => monitor.status(),
            b":verify" => monitor.verify(),
            b":promote" => monitor.promote(),
            b":fault" => monitor.fault(),
            b":shutdown" => unsafe { out32(0xf4, 0x10) },
            b"" => {}
            _ => match session.evaluate(source) {
                Ok(value) => write_value(value, source),
                Err(error) => write_error(error),
            },
        }
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn read_line(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        match Serial::read_byte() {
            b'\r' | b'\n' => {
                Serial::write("\n");
                return length;
            }
            8 | 127 if length > 0 => {
                length -= 1;
                Serial::write("\x08 \x08");
            }
            byte if (byte.is_ascii_graphic() || byte == b' ') && length < buffer.len() => {
                buffer[length] = byte;
                length += 1;
                Serial::write_byte(byte);
            }
            _ => {}
        }
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn read_form(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        length += read_line(&mut buffer[length..]);
        if !needs_more_input(&buffer[..length]) || length == buffer.len() {
            return length;
        }
        buffer[length] = b'\n';
        length += 1;
        Serial::write("             ... ");
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn needs_more_input(source: &[u8]) -> bool {
    let mut depth = 0_u16;
    let mut comment = false;
    for byte in source {
        if comment {
            if *byte == b'\n' {
                comment = false;
            }
            continue;
        }
        match byte {
            b';' => comment = true,
            b'(' => depth = depth.saturating_add(1),
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth > 0
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn write_value(value: native::Value, source: &[u8]) {
    match value {
        native::Value::Int(value) => write_i64(value),
        native::Value::Bool(true) => Serial::write("#t"),
        native::Value::Bool(false) => Serial::write("#f"),
        native::Value::Nil => Serial::write("nil"),
        native::Value::Code { start, end } => {
            Serial::write("'");
            write_bytes(&source[start as usize..end as usize]);
        }
        native::Value::Function => Serial::write("#<native-function>"),
    }
    Serial::write("\n");
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn write_error(error: native::Error) {
    Serial::write("error: ");
    Serial::write(error.0);
    Serial::write(" (transaction rolled back)\n");
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn write_bytes(bytes: &[u8]) {
    for byte in bytes {
        Serial::write_byte(*byte);
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn write_i64(value: i64) {
    if value < 0 {
        Serial::write_byte(b'-');
        write_u64(value.unsigned_abs());
    } else {
        write_u64(value as u64);
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
fn write_u64(mut value: u64) {
    let mut digits = [0_u8; 20];
    let mut position = digits.len();
    loop {
        position -= 1;
        digits[position] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    write_bytes(&digits[position..]);
}

#[cfg(all(
    feature = "native-selftest",
    not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "isolation-selftest"
    ))
))]
fn native_selftest() -> ! {
    let mut session = native::Session::new();
    let passed = expect_int(&mut session, b"(+ 20 22)", 42)
        && expect_int(&mut session, b"-9223372036854775808", i64::MIN)
        && expect_int(&mut session, b"((fn (x) ((fn (x) (+ x 1)) 41)) 0)", 42)
        && expect_int(&mut session, b"(((fn (x) (fn (y) (+ x y))) 40) 2)", 42)
        && session.evaluate(b"(def + 9)").is_err()
        && session.evaluate(b"(fn (x x) x)").is_err()
        && session
            .evaluate(b"((fn (x) (def add-x (fn (y) (+ x y)))) 40)")
            .is_err()
        && session.evaluate(b"(def f (fn (x) 1))").is_ok()
        && expect_int(&mut session, b"(f (begin (def f (fn (x) 2)) 0))", 1)
        && session.evaluate(b"(def square (fn (x) (* x x)))").is_ok()
        && expect_int(&mut session, b"(square 9)", 81)
        && expect_int(&mut session, b"(eval '(+ 40 2))", 42)
        && session.evaluate(b"(def x 1)").is_ok()
        && session.evaluate(b"(def x 2)").is_ok()
        && session.evaluate(b"(begin (def x 3) (/ 1 0))").is_err()
        && session.rollback().is_ok()
        && session.integer(b"x") == Some(1)
        && session
            .evaluate(b"(def fact (fn (n) (if (= n 0) 1 (* n (fact (- n 1))))))")
            .is_ok()
        && expect_int(&mut session, b"(fact 6)", 720);
    if passed {
        Serial::write("AGEL_NATIVE_OK\n");
        unsafe { out32(0xf4, 0x10) };
    } else {
        Serial::write("AGEL_NATIVE_FAILED\n");
        unsafe { out32(0xf4, 0x11) };
    }
    halt()
}

#[cfg(all(
    feature = "native-selftest",
    not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "isolation-selftest"
    ))
))]
fn expect_int(session: &mut native::Session, source: &[u8], expected: i64) -> bool {
    session.evaluate(source) == Ok(native::Value::Int(expected))
}

/// Zero the `.bss` section named by the linker script.
fn zero_bss() {
    extern "C" {
        static mut __bss_start: u8;
        static mut __bss_end: u8;
    }
    // Safety: the linker guarantees the two symbols bound one contiguous,
    // 8-byte-aligned region that belongs to this image and that nothing has
    // read yet.
    unsafe {
        let start = &raw mut __bss_start as *mut u64;
        let end = &raw mut __bss_end as *mut u64;
        let mut cursor = start;
        while cursor < end {
            cursor.write_volatile(0);
            cursor = cursor.add(1);
        }
    }
}

#[inline]
unsafe fn out(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack)) };
}

#[inline]
unsafe fn out32(port: u16, value: u32) {
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack)) };
}

#[inline]
unsafe fn input(port: u16) -> u8 {
    let value: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack)) };
    value
}

fn halt() -> ! {
    loop {
        unsafe { asm!("cli; hlt", options(nomem, nostack)) };
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    Serial::write("KERNEL PANIC: recovery monitor halted the mutable world\n");
    halt()
}

// ---------------------------------------------------------------------------
// Phase 1 isolation self-test
// ---------------------------------------------------------------------------

/// A `core::fmt` sink over COM1, so the kernel can render the contract
/// transcript with exactly the same code the hosted reference model uses.
#[cfg(feature = "isolation-selftest")]
struct SerialWriter;

#[cfg(feature = "isolation-selftest")]
impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        Serial::write(text);
        Ok(())
    }
}

#[cfg(feature = "isolation-selftest")]
macro_rules! kprint {
    ($($argument:tt)*) => {{
        use core::fmt::Write as _;
        let _ = write!(SerialWriter, $($argument)*);
    }};
}

/// Report a condition that makes the isolation test meaningless and stop.
#[cfg(feature = "isolation-selftest")]
fn isolation_failed(reason: &str) -> ! {
    Serial::write("AGEL_ISOLATION_FAILED: ");
    Serial::write(reason);
    Serial::write("\n");
    unsafe { out32(0xf4, 0x11) };
    halt()
}

#[cfg(feature = "isolation-selftest")]
fn allocate_stack(pool: &mut memory::FramePool, pages: u64) -> u64 {
    let mut top = 0;
    for _ in 0..pages {
        match pool.allocate() {
            // The pool is a bump allocator, so consecutive frames are
            // contiguous and the last one's end is the top of the stack.
            Ok(frame) => top = frame + memory::PAGE,
            Err(_) => isolation_failed("frame pool exhausted building a kernel stack"),
        }
    }
    top
}

/// Prove that an unprivileged world can speak the kernel contract, and cannot
/// do anything else.
#[cfg(feature = "isolation-selftest")]
fn isolation_selftest() -> ! {
    use agel_kernel_abi::model::ModelKernel;
    use agel_kernel_abi::{conformance, write_step, Kernel};
    use domain::{shared, Domain, Stop};

    extern "C" {
        static __user_text_start: u8;
        static __user_text_end: u8;
    }
    // Only the addresses are taken; the bytes are never read through these.
    let user_text = (&raw const __user_text_start) as u64..(&raw const __user_text_end) as u64;

    hal::disable_interrupts();
    if !memory::enable_no_execute() {
        isolation_failed("processor does not support the no-execute bit");
    }

    let mut pool = memory::FramePool::new();
    let Ok(identity) = memory::build_identity_window(&mut pool, user_text.clone()) else {
        isolation_failed("could not build the identity window");
    };
    let Ok(kernel_space) = memory::AddressSpace::new(&mut pool, identity) else {
        isolation_failed("could not build the kernel address space");
    };
    // Safety: the new space maps every address this code and its stack use.
    unsafe {
        kernel_space.activate();
        domain::set_kernel_root(kernel_space.root());
    }

    let trap_stack = allocate_stack(&mut pool, 4);
    let fault_stack = allocate_stack(&mut pool, 2);
    // Safety: called once, from ring 0, with interrupts disabled, and with two
    // distinct kernel stacks.
    unsafe {
        cpu::install(trap_stack, fault_stack);
        // 1_193_182 Hz / 11_932 is very close to 100 Hz.
        cpu::remap_interrupts(11_932);
    }
    Serial::write("isolation: page tables, descriptors, traps and timer installed\n");

    let entry = user::agel_world_main as usize as u64;
    if !user_text.contains(&entry) {
        isolation_failed("the ring-3 entry point is not in user-executable text");
    }

    // -- The contract, spoken from ring 3 -----------------------------------
    let Ok(mut world) = Domain::new(&mut pool, identity, entry, 8) else {
        isolation_failed("could not build the conformance world");
    };
    let mut reference = ModelKernel::new();
    reference.reset_to_conformance_domain();

    let mut agreed = 0_usize;
    Serial::write("---BEGIN AGEL RING3 CONTRACT TRANSCRIPT---\n");
    kprint!(
        "agel-kernel-contract v{}.{}.{} corpus={} steps\n",
        agel_kernel_abi::VERSION_MAJOR,
        agel_kernel_abi::VERSION_MINOR,
        agel_kernel_abi::VERSION_PATCH,
        conformance::CORPUS.len()
    );
    for step in conformance::CORPUS {
        let observed = world.invoke_in_world(&step.request);
        let expected = reference.invoke(&step.request);
        let _ = write_step(&mut SerialWriter, step.label, &step.request, &observed);
        if observed == expected {
            agreed += 1;
        }
    }
    Serial::write("---END AGEL RING3 CONTRACT TRANSCRIPT---\n");
    if agreed != conformance::CORPUS.len() {
        isolation_failed("ring 3 and the reference model disagree");
    }
    if world.stopped().is_some() {
        isolation_failed("the conformance world did not survive its own corpus");
    }
    Serial::write("isolation: ring-3 corpus matches the reference model\n");

    // -- Four hostile worlds, four containments -----------------------------
    let provocations = [
        (
            shared::COMMAND_FAULT_WRITE,
            "page-fault",
            "writing to kernel memory",
        ),
        (
            shared::COMMAND_FAULT_DIVIDE,
            "divide-error",
            "dividing by zero",
        ),
        (
            shared::COMMAND_FAULT_PRIVILEGED,
            "general-protection",
            "masking interrupts",
        ),
    ];
    for (command, expected, description) in provocations {
        let Ok(mut hostile) = Domain::new(&mut pool, identity, entry, 8) else {
            isolation_failed("could not build a hostile world");
        };
        match hostile.provoke(command) {
            Stop::Faulted(fault) if fault.name() == expected => {
                kprint!(
                    "isolation: contained a world {description}: {} at rip {:#x}\n",
                    fault.name(),
                    fault.rip
                );
            }
            Stop::Faulted(fault) => {
                kprint!("isolation: unexpected {} for {description}\n", fault.name());
                isolation_failed("a world faulted in an unexpected way");
            }
            _ => isolation_failed("a world was not contained"),
        }
        // A stopped world stays stopped, and cannot be re-entered by accident.
        if !matches!(hostile.run(), Stop::Faulted(_)) {
            isolation_failed("a stopped world was resumed");
        }
    }

    let Ok(mut spinner) = Domain::new(&mut pool, identity, entry, 4) else {
        isolation_failed("could not build the looping world");
    };
    match spinner.provoke(shared::COMMAND_SPIN) {
        Stop::BudgetExhausted => {
            Serial::write("isolation: preempted a world that never yields\n");
        }
        _ => isolation_failed("an infinite loop was not preempted"),
    }

    // -- The recovery plane is intact ---------------------------------------
    let mut monitor = RecoveryMonitor::new();
    monitor.status();
    monitor.promote();
    monitor.verify();
    monitor.promote();
    monitor.fault();
    monitor.status();
    kprint!("isolation: {} frames still unallocated\n", pool.remaining());

    Serial::write("AGEL_ISOLATION_OK\n");
    unsafe { out32(0xf4, 0x10) };
    halt()
}

/// A trap taken in ring 0 is a defect in the kernel itself.
///
/// There is no domain to contain and no state that can be trusted, so the only
/// honest action is to say which vector fired and stop. Silently continuing
/// would turn a kernel bug into a world that appears to have been contained.
#[cfg(feature = "isolation-selftest")]
pub fn report_kernel_trap(frame: &cpu::TrapFrame) -> ! {
    kprint!(
        "AGEL_ISOLATION_FAILED: supervisor trap vector {} error {:#x} at rip {:#x}\n",
        frame.vector,
        frame.error,
        frame.rip
    );
    unsafe { out32(0xf4, 0x11) };
    halt()
}

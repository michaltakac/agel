//! The architecture-neutral isolation and conformance driver.
//!
//! This is the whole Phase 1 claim in one function, and it runs unchanged on
//! every backend: bring the machine up, hand an unprivileged world the kernel
//! contract and require it to answer all of it correctly, then require the
//! supervisor to survive every way that world can misbehave.
//!
//! Nothing here knows what a page table entry looks like. That is deliberate:
//! if the shared driver needed architecture knowledge, the contract would not
//! actually be the boundary.

use crate::arch;
use crate::console;
use crate::kprint;
use crate::monitor::RecoveryMonitor;
use crate::service::{ServiceDomain, ServiceError, ServiceWriter};
use crate::world::{shared, Stop};
use agel_kernel_abi::model::ModelKernel;
use agel_kernel_abi::{conformance, write_step, Kernel};
use core::fmt::Write as _;

/// Report a condition that makes the isolation test meaningless and stop.
fn failed(reason: &str) -> ! {
    console::write("AGEL_ISOLATION_FAILED: ");
    console::write(reason);
    console::write("\n");
    arch::exit(false)
}

/// Prove that an unprivileged world can speak the kernel contract, and cannot
/// do anything else.
pub fn run() -> ! {
    let mut machine = match arch::Machine::bring_up() {
        Ok(machine) => machine,
        Err(reason) => failed(reason),
    };
    kprint!(
        "isolation[{}]: address spaces, traps and preemption installed\n",
        arch::NAME
    );

    let entry = crate::user::agel_world_main as *const () as usize as u64;
    if !arch::user_text_range().contains(&entry) {
        failed("the unprivileged entry point is not in user-executable text");
    }

    // Phase 3: the console driver leaves the supervisor before anything else
    // uses it, so that the conformance transcript below is printed by an
    // unprivileged domain. If the driver does not work, the transcript does not
    // appear, and the frozen-transcript diff fails. That is a stronger check
    // than any assertion about the driver could be.
    let mut console = match machine.create_console_world(entry, 8) {
        Ok(domain) => ServiceDomain::new(domain, entry, 8),
        Err(reason) => failed(reason),
    };
    kprint!(
        "isolation[{}]: console driver in an unprivileged domain, generation {}\n",
        arch::NAME,
        console.generation()
    );

    run_conformance(&mut machine, &mut console);
    run_native_evaluator(&mut machine, &mut console);
    run_containment(&mut machine);
    run_driver_restart(&mut machine, &mut console);

    // The recovery plane must still work after everything above. A supervisor
    // that survives a hostile world but loses its own recovery policy has not
    // survived in any sense that matters.
    let mut monitor = RecoveryMonitor::new();
    monitor.status();
    monitor.promote();
    monitor.verify();
    monitor.promote();
    monitor.fault();
    monitor.status();
    kprint!(
        "isolation[{}]: {} frames still unallocated\n",
        arch::NAME,
        machine.frames_remaining()
    );

    console::write("AGEL_ISOLATION_OK\n");
    arch::exit(true)
}

/// Run a persistent native Agel session entirely inside an unprivileged world.
fn run_native_evaluator(machine: &mut arch::Machine, driver: &mut ServiceDomain) {
    let entry = crate::user::agel_evaluator_main as *const () as usize as u64;
    if !arch::user_text_range().contains(&entry) {
        failed("the evaluator entry point is not in user-executable text");
    }
    let mut evaluator = match machine.create_evaluator_world(entry, 20) {
        Ok(world) => world,
        Err(reason) => failed(reason),
    };

    expect_evaluation(&mut evaluator, b"(+ 20 22)", b"42", 1, false);
    expect_evaluation(
        &mut evaluator,
        b"(def fact (fn (n) (if (= n 0) 1 (* n (fact (- n 1))))))",
        b"#<native-function>",
        2,
        false,
    );
    expect_evaluation(&mut evaluator, b"(fact 6)", b"720", 3, false);
    expect_evaluation(
        &mut evaluator,
        b"(begin (def answer 42) answer)",
        b"42",
        4,
        false,
    );
    expect_evaluation(
        &mut evaluator,
        b"(begin (def answer 99) (/ 1 0))",
        b"error: division by zero",
        4,
        true,
    );
    expect_evaluation(&mut evaluator, b"answer", b"42", 5, false);

    {
        let mut out = ServiceWriter::new(driver);
        let _ = writeln!(
            out,
            "isolation[{}]: native Agel evaluated factorial with transactional rollback in an unprivileged domain",
            arch::NAME
        );
        out.flush();
        if out.failure().is_some() {
            failed("the console driver could not report evaluator success");
        }
    }

    // Generic fault and non-yield containment run immediately below. The live
    // evaluator corpus above is kept cooperative so every backend reaches the
    // same subsequent adversarial sequence.
}

fn expect_evaluation(
    evaluator: &mut arch::Domain,
    source: &[u8],
    expected: &[u8],
    revision: u64,
    error: bool,
) {
    if source.len() > crate::world::PAYLOAD_BYTES {
        failed("native evaluator test source exceeds its shared buffer");
    }
    for (offset, byte) in source.iter().enumerate() {
        evaluator.core().write_payload(offset, *byte);
    }
    evaluator
        .core()
        .write_shared(shared::ARGUMENTS, source.len() as u64);
    evaluator.core().stage_command(shared::COMMAND_EVALUATE);
    match evaluator.run() {
        Stop::Replied => {}
        Stop::Faulted(fault) => {
            kprint!(
                "isolation[{}]: native evaluator faulted: {} (cause {:#x}, detail {:#x}) at {:#x} touching {:#x}\n",
                arch::NAME,
                fault.name(),
                fault.cause,
                fault.detail,
                fault.pc,
                fault.address
            );
            failed("native evaluator did not yield a response");
        }
        Stop::BudgetExhausted => failed("native evaluator exhausted its tick budget"),
    }
    let observed_error = evaluator.core().read_shared(shared::STATUS) != 0;
    let length = evaluator.core().read_shared(shared::VALUES) as usize;
    let observed_revision = evaluator.core().read_shared(shared::VALUES + 1);
    if observed_error != error || observed_revision != revision || length != expected.len() {
        kprint!(
            "isolation[{}]: evaluator metadata error={} length={} revision={}, expected error={} length={} revision={}\n",
            arch::NAME,
            observed_error,
            length,
            observed_revision,
            error,
            expected.len(),
            revision
        );
        console::write("isolation: evaluator answered: ");
        for offset in 0..length.min(crate::world::PAYLOAD_BYTES) {
            console::write_byte(evaluator.core().read_payload(offset));
        }
        console::write("\n");
        failed("native evaluator response metadata disagrees");
    }
    for (offset, byte) in expected.iter().enumerate() {
        if evaluator.core().read_payload(offset) != *byte {
            failed("native evaluator response bytes disagree");
        }
    }
}

/// Run the frozen corpus inside an unprivileged world and compare every answer
/// against the reference model running in the supervisor.
fn run_conformance(machine: &mut arch::Machine, driver: &mut ServiceDomain) {
    let mut world =
        match machine.create_world(crate::user::agel_world_main as *const () as usize as u64, 8) {
            Ok(world) => world,
            Err(reason) => failed(reason),
        };
    let mut reference = ModelKernel::new();
    reference.reset_to_conformance_domain();

    let mut agreed = 0_usize;
    {
        // Every byte below is written by a domain that holds the device, on
        // behalf of a supervisor that no longer reaches it directly.
        let mut out = ServiceWriter::new(driver);
        let _ = writeln!(out, "---BEGIN AGEL CONTRACT TRANSCRIPT---");
        let _ = writeln!(
            out,
            "agel-kernel-contract v{}.{}.{} corpus={} steps",
            agel_kernel_abi::VERSION_MAJOR,
            agel_kernel_abi::VERSION_MINOR,
            agel_kernel_abi::VERSION_PATCH,
            conformance::CORPUS.len()
        );
        for step in conformance::CORPUS {
            let observed = world.invoke_in_world(&step.request);
            let expected = reference.invoke(&step.request);
            let _ = write_step(&mut out, step.label, &step.request, &observed);
            if observed == expected {
                agreed += 1;
            }
        }
        let _ = writeln!(out, "---END AGEL CONTRACT TRANSCRIPT---");
        out.flush();
        if let Some(error) = out.failure() {
            kprint!(
                "isolation[{}]: the console driver failed: {}\n",
                arch::NAME,
                error.name()
            );
            failed("the console driver did not print the transcript");
        }
    }

    if agreed != conformance::CORPUS.len() {
        // A disagreement is almost always the world having been stopped rather
        // than the world having answered wrongly, so say which before failing.
        match world.stopped() {
            Some(Stop::Faulted(fault)) => kprint!(
                "isolation[{}]: the conformance world faulted: {} (cause {:#x}, detail {:#x}) at {:#x} touching {:#x}\n",
                arch::NAME,
                fault.name(),
                fault.cause,
                fault.detail,
                fault.pc,
                fault.address
            ),
            Some(Stop::BudgetExhausted) => kprint!(
                "isolation[{}]: the conformance world never yielded\n",
                arch::NAME
            ),
            _ => kprint!(
                "isolation[{}]: {} of {} answers matched\n",
                arch::NAME,
                agreed,
                conformance::CORPUS.len()
            ),
        }
        failed("the unprivileged world and the reference model disagree");
    }
    if world.stopped().is_some() {
        failed("the conformance world did not survive its own corpus");
    }
    kprint!(
        "isolation[{}]: unprivileged corpus matches the reference model\n",
        arch::NAME
    );
}

/// Make a fresh world misbehave in every way this architecture can, and require
/// each one to be contained.
fn run_containment(machine: &mut arch::Machine) {
    let entry = crate::user::agel_world_main as *const () as usize as u64;
    for provocation in arch::PROVOCATIONS {
        let mut hostile = match machine.create_world(entry, 4) {
            Ok(world) => world,
            Err(reason) => failed(reason),
        };
        match (hostile.provoke(provocation.command), provocation.expected) {
            (Stop::Faulted(fault), Some(expected)) if fault.name() == expected => {
                kprint!(
                    "isolation[{}]: contained a world {}: {} at {:#x}\n",
                    arch::NAME,
                    provocation.description,
                    fault.name(),
                    fault.pc
                );
            }
            (Stop::BudgetExhausted, None) => {
                kprint!(
                    "isolation[{}]: preempted a world {}\n",
                    arch::NAME,
                    provocation.description
                );
            }
            (Stop::Faulted(fault), _) => {
                kprint!(
                    "isolation[{}]: {} produced {} (cause {:#x}, detail {:#x}) at {:#x}\n",
                    arch::NAME,
                    provocation.description,
                    fault.name(),
                    fault.cause,
                    fault.detail,
                    fault.pc
                );
                failed("a world was contained in an unexpected way");
            }
            _ => failed("a world was not contained"),
        }
        // A stopped world stays stopped, and cannot be re-entered by accident.
        if hostile.stopped().is_none() {
            failed("a stopped world did not latch its stop reason");
        }
        if matches!(hostile.run(), Stop::Replied) {
            failed("a stopped world was resumed");
        }
    }
    let _ = shared::COMMAND_INVOKE;
}

/// Lose the console driver on purpose, replace it, and require the supervisor
/// to have noticed.
///
/// This is the half of "its own restartable domain" that a driver merely living
/// somewhere else does not give you. The old handle is kept deliberately: a
/// caller that has not noticed a restart must be refused, not quietly served by
/// a server that no longer remembers the conversation.
fn run_driver_restart(machine: &mut arch::Machine, driver: &mut ServiceDomain) {
    let stale = driver.handle();

    match driver.provoke(shared::COMMAND_FAULT_WRITE) {
        Stop::Faulted(fault) => kprint!(
            "isolation[{}]: the console driver faulted: {} at {:#x}\n",
            arch::NAME,
            fault.name(),
            fault.pc
        ),
        _ => failed("the console driver was not contained"),
    }

    // The supervisor is still here, and can still say so, because its own
    // last-resort path does not run through the component that just died.
    if driver.stopped().is_none() {
        failed("the console driver did not latch its stop reason");
    }

    if let Err(reason) = driver.restart(machine) {
        failed(reason);
    }
    kprint!(
        "isolation[{}]: replaced it; generation {} after {} restart\n",
        arch::NAME,
        driver.generation(),
        driver.restarts()
    );

    // A handle from before the restart must fail closed.
    let mut refused = ServiceWriter::with_handle(driver, stale);
    let _ = writeln!(refused, "this line must never appear");
    refused.flush();
    match refused.failure() {
        Some(ServiceError::Stale) => kprint!(
            "isolation[{}]: a handle from generation {} was refused: {}\n",
            arch::NAME,
            stale.generation(),
            ServiceError::Stale.status().name()
        ),
        Some(other) => {
            kprint!("isolation[{}]: unexpected {}\n", arch::NAME, other.name());
            failed("a stale handle failed in the wrong way");
        }
        None => failed("a stale handle was accepted after a restart"),
    }

    // The replacement works, and says so through the device it now holds.
    let mut out = ServiceWriter::new(driver);
    let _ = writeln!(
        out,
        "isolation[{}]: the replacement console driver is printing this line",
        arch::NAME
    );
    out.flush();
    if out.failure().is_some() {
        failed("the replacement console driver did not print");
    }
}

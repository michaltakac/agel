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
use crate::world::{shared, Stop};
use agel_kernel_abi::model::ModelKernel;
use agel_kernel_abi::{conformance, write_step, Kernel};

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

    let entry = crate::user::agel_world_main as usize as u64;
    if !arch::user_text_range().contains(&entry) {
        failed("the unprivileged entry point is not in user-executable text");
    }

    run_conformance(&mut machine);
    run_containment(&mut machine);

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

/// Run the frozen corpus inside an unprivileged world and compare every answer
/// against the reference model running in the supervisor.
fn run_conformance(machine: &mut arch::Machine) {
    let mut world = match machine.create_world(crate::user::agel_world_main as usize as u64, 8) {
        Ok(world) => world,
        Err(reason) => failed(reason),
    };
    let mut reference = ModelKernel::new();
    reference.reset_to_conformance_domain();

    let mut agreed = 0_usize;
    console::write("---BEGIN AGEL CONTRACT TRANSCRIPT---\n");
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
        let _ = write_step(&mut console::Writer, step.label, &step.request, &observed);
        if observed == expected {
            agreed += 1;
        }
    }
    console::write("---END AGEL CONTRACT TRANSCRIPT---\n");

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
    let entry = crate::user::agel_world_main as usize as u64;
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

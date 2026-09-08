use agel_core::{EvaluationOptions, Value, World};
use agel_jit::{
    managed::{Fault, Limits, Native},
    state::{CommitError, NativeState},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::default();
    let mut options = EvaluationOptions::default();
    options.budget.fuel = 10_000_000;
    agel_stdlib::install(&mut world, &options)?;
    world.evaluate("(import agel/native)")?;
    let compiler_ir = world
        .evaluate_with("(native-compile native-compiler-source)", &options)?
        .values
        .pop()
        .unwrap();
    let compiler = Native::compile(&compiler_ir)?;
    let source = world
        .evaluate_with(
            include_str!("../../../examples/jit-agent-swarm.agel"),
            &options,
        )?
        .values
        .pop()
        .unwrap();
    let ir = compiler.invoke(&[source], Limits::default())?.value;
    let native = Native::compile(&ir)?;
    let initial = world.evaluate("swarm-world")?.values.pop().unwrap();
    let mut swarm = NativeState::new(native, initial.clone());
    assert_eq!(
        swarm
            .transact(
                0,
                &Value::Int(3),
                Limits {
                    fuel: 10,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        CommitError::Execution(Fault::Fuel)
    );
    assert_eq!(swarm.state(), &initial);
    assert_eq!(swarm.revision(), 0);
    println!("PASS: exhausted execution preserves state, queued messages and revision");
    let receipt = swarm.transact(0, &Value::Int(3), Limits::default())?;
    println!(
        "Committed Agel scheduler + agent behaviors, revision {}: {}",
        receipt.revision,
        swarm.state()
    );
    println!(
        "{} tail calls; peak depth {}; {} fuel",
        receipt.tail_calls, receipt.peak_call_depth, receipt.fuel_used
    );
    assert_eq!(
        swarm
            .transact(0, &Value::Int(1), Limits::default())
            .unwrap_err(),
        CommitError::StaleRevision
    );
    println!("PASS: stale revision rejected. No I/O or model authority; isolated hosted scheduler, not the freestanding OS.");
    Ok(())
}

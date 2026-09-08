use agel_core::{EvaluationOptions, Value, World};
use agel_jit::managed::{Fault, Limits, Native};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::default();
    let mut options = EvaluationOptions::default();
    options.budget.fuel = 10_000_000;
    agel_stdlib::install(&mut world, &options)?;
    world.evaluate("(import agel/native)")?;
    let source = world
        .evaluate("native-compiler-source")?
        .values
        .pop()
        .unwrap();
    let start = Instant::now();
    let seed_ir = world
        .evaluate_with("(native-compile native-compiler-source)", &options)?
        .values
        .pop()
        .unwrap();
    println!(
        "Agel frontend compiling itself on the Rust seed: {:?}",
        start.elapsed()
    );
    let start = Instant::now();
    let stage_one = Native::compile(&seed_ir)?;
    println!("Cranelift compiling the frontend IR: {:?}", start.elapsed());
    let start = Instant::now();
    let next = stage_one.invoke(std::slice::from_ref(&source), Limits::default())?;
    assert_eq!(next.value, seed_ir);
    println!(
        "Native frontend compiling itself: {:?}; {} fuel, {} logical allocations",
        start.elapsed(),
        next.fuel_used,
        next.allocated_values
    );
    let stage_two = Native::compile(&next.value)?;
    assert_eq!(
        stage_two.invoke(&[source], Limits::default())?.value,
        seed_ir
    );
    println!("PASS: seed IR = stage-one IR = stage-two IR");

    let program = world
        .evaluate(&format!(
            "'{}",
            include_str!("../../../examples/jit-closure-workshop.agel")
        ))?
        .values
        .pop()
        .unwrap();
    let program_ir = stage_two.invoke(&[program], Limits::default())?.value;
    let native = Native::compile(&program_ir)?;
    let result = native.invoke(&[Value::Int(40)], Limits::default())?;
    println!("Native closure/collection workshop: {}", result.value);
    assert_eq!(
        native
            .invoke(
                &[Value::Int(40)],
                Limits {
                    fuel: 10,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Fuel
    );
    println!("PASS: too-small fuel budget rejects execution; no model calls");
    println!(
        "This frontend self-compiles. The Rust runtime/backend and full OS are not self-hosted."
    );
    Ok(())
}

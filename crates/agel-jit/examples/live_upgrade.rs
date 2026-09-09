use agel_core::{EvaluationOptions, Value, World};
use agel_jit::{
    managed::{Fault, Limits, Native},
    state::NativeState,
};
use std::time::Instant;

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    let Value::Map(entries) = value else {
        panic!("expected map")
    };
    &entries
        .iter()
        .find(|(k, _)| k == &Value::Symbol(key.into()))
        .expect("expected field")
        .1
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::default();
    let mut options = EvaluationOptions::default();
    options.budget.fuel = 10_000_000;
    agel_stdlib::install(&mut world, &options)?;
    world.evaluate("(import agel/native) (import agel/native-agent-kernel) (import agel/native-system-builder)")?;
    let compiler_ir = world
        .evaluate_with("(native-compile native-compiler-source)", &options)?
        .values
        .pop()
        .unwrap();
    let compiler = Native::compile(&compiler_ir)?;
    let builder_source = world
        .evaluate(
            "(list 'fn '(sources)
      (list native-system-builder-source native-compiler-source
        (list 'quote native-agent-kernel-source) 'sources))",
        )?
        .values
        .pop()
        .unwrap();
    let builder_ir = compiler.invoke(&[builder_source], Limits::default())?.value;
    let builder = Native::compile(&builder_ir)?;
    world.evaluate_with(
        include_str!("../../../examples/jit-live-upgrade.agel"),
        &options,
    )?;
    let mut sources = world.evaluate("evolution-sources")?.values.pop().unwrap();
    let initial = world.evaluate("evolution-world")?.values.pop().unwrap();
    let source = builder.invoke_refs(&[&sources], Limits::default())?.value;
    // Check native composition against the Agel seed once during bootstrap.
    let expected = world
        .evaluate_with("(native-system-source evolution-sources)", &options)?
        .values
        .pop()
        .unwrap();
    assert_eq!(source, expected);
    let invalid_sources = world
        .evaluate("(dict 'counter '(fn (heap message) world))")?
        .values
        .pop()
        .unwrap();
    assert_eq!(
        builder
            .invoke(&[invalid_sources], Limits::default())
            .unwrap_err(),
        Fault::Signaled
    );
    let ir = compiler.invoke(&[source], Limits::default())?.value;
    let mut machine = NativeState::new(Native::compile(&ir)?, initial);
    // No World/evaluator calls below: designer, composer and frontend run native.
    machine.transact(0, &Value::Int(1), Limits::default())?;
    let proposal = field(field(field(machine.state(), "agents"), "designer"), "state").clone();
    println!("Compiled designer proposed: {proposal}");
    let Value::Map(entries) = &mut sources else {
        unreachable!()
    };
    entries
        .iter_mut()
        .find(|(k, _)| k == &Value::Symbol("counter".into()))
        .unwrap()
        .1 = proposal;
    let start = Instant::now();
    let candidate_source = builder.invoke(&[sources], Limits::default())?.value;
    let candidate_ir = compiler
        .invoke(&[candidate_source], Limits::default())?
        .value;
    let lowering = start.elapsed();
    let start = Instant::now();
    let candidate_code = Native::compile(&candidate_ir)?;
    println!(
        "Candidate native Agel composition/lowering: {lowering:?}; backend compilation: {:?}",
        start.elapsed()
    );
    let candidate = machine.preview(1, candidate_code, &Value::Int(1), Limits::default())?;
    assert_eq!(candidate.ir(), &candidate_ir);
    assert_eq!(
        field(
            field(field(&candidate.probe().value, "agents"), "counter"),
            "state"
        ),
        &Value::Int(10)
    );
    let before = machine.state().clone();
    machine.promote(1, candidate)?;
    assert_eq!(machine.state(), &before);
    assert_eq!(field(machine.state(), "pending"), &Value::Int(2));
    println!("Preview predicted 10; promotion preserved state and both queued messages.");
    machine.transact(2, &Value::Int(1), Limits::default())?;
    assert_eq!(
        field(field(field(machine.state(), "agents"), "counter"), "state"),
        &Value::Int(10)
    );
    machine.rollback_program(3, &Value::Int(1), Limits::default())?;
    assert_eq!(
        field(field(field(machine.state(), "agents"), "counter"), "state"),
        &Value::Int(10)
    );
    machine.transact(4, &Value::Int(1), Limits::default())?;
    assert_eq!(
        field(field(field(machine.state(), "agents"), "counter"), "state"),
        &Value::Int(11)
    );
    assert_eq!(field(machine.state(), "pending"), &Value::Int(0));
    println!("Upgraded counter: 10. Rollback kept state; next queued message used old code: 11.");
    println!("PASS: native agent proposal -> native Agel composition/compilation -> preview -> promotion -> rollback. Host-authorized, in-memory; no models or graphical-OS integration.");
    Ok(())
}

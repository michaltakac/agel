use agel_core::{EvaluationOptions, Value, World};
use agel_jit::managed::{Limits, Native};

pub const PROGRAM: &str = include_str!("../../../examples/jit-text-workshop.agel");

fn read_one(reader: &Native, source: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let result = reader.invoke(
        &[
            Value::String(source.into()),
            Value::Int(65536),
            Value::Int(64),
        ],
        Limits::default(),
    )?;
    let Value::List(mut forms) = result.value else {
        return Err("expected one function".into());
    };
    if forms.len() != 1 {
        return Err("expected exactly one function".into());
    }
    Ok(forms.remove(0))
}

pub fn run(source: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let mut world = World::default();
    let mut options = EvaluationOptions::default();
    options.budget.fuel = 20_000_000;
    agel_stdlib::install(&mut world, &options)?;
    world.evaluate("(import agel/native) (import agel/native-reader)")?;
    let ir = world
        .evaluate_with("(native-compile native-compiler-source)", &options)?
        .values
        .pop()
        .unwrap();
    let compiler = Native::compile(&ir)?;
    let reader_source = world
        .evaluate("native-reader-source")?
        .values
        .pop()
        .unwrap();
    let reader_ir = compiler.invoke(&[reader_source], Limits::default())?.value;
    let reader = Native::compile(&reader_ir)?;
    drop(world);
    // Seed is gone. Re-read/rebuild both tools from text through native Agel.
    let source_ir = compiler
        .invoke(
            &[read_one(&reader, agel_stdlib::NATIVE_READER)?],
            Limits::default(),
        )?
        .value;
    assert_eq!(source_ir, reader_ir);
    let reader = Native::compile(&source_ir)?;
    let compiler_ir = compiler
        .invoke(
            &[read_one(&reader, agel_stdlib::NATIVE_COMPILER)?],
            Limits::default(),
        )?
        .value;
    assert_eq!(compiler_ir, ir);
    let compiler = Native::compile(&compiler_ir)?;
    println!(
        "PASS: Agel read and rebuilt its reader and compiler from text; IR matches bootstrap."
    );
    let ir = compiler
        .invoke(&[read_one(&reader, source)?], Limits::default())?
        .value;
    let result = Native::compile(&ir)?.invoke(&[Value::Int(10)], Limits::default())?;
    println!(
        "Native text workshop: {} ({} execution fuel)",
        result.value, result.fuel_used
    );
    Ok(result.value)
}

#[cfg(not(test))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(path)?,
        None => PROGRAM.into(),
    };
    run(&text)?;
    println!("Rust still supplies runtime mechanisms and machine-code emission; no model calls or OS update.");
    Ok(())
}

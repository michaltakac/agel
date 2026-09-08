use agel_core::{EvaluationOptions, Value, World};
use agel_jit::Compiled;
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default())?;
    let start = Instant::now();
    let ir = world
        .evaluate(include_str!("../../../examples/jit-kernel.agel"))?
        .values
        .pop()
        .expect("example returns IR");
    let lowering = start.elapsed();
    println!("Agel-authored IR: {ir}");
    let start = Instant::now();
    let native = Compiled::compile(&ir)?;
    let compilation = start.elapsed();
    let fuel = native.required_fuel();
    let expected = world
        .evaluate("(jit-run kernel-ir '(6 50))")?
        .values
        .pop()
        .unwrap();
    let value = native.invoke(&[6, 50], fuel)?;
    assert_eq!(value, expected);
    assert_eq!(value, Value::Int(37));
    println!("Native machine-code result: {value}; conservative IR fuel: {fuel}");
    if std::env::args().any(|arg| arg == "--ir") {
        println!("{}", native.clif());
    }

    // This measures the native invocation wrapper only, NOT world transactions,
    // parsing, compilation, model calls or general application throughput.
    let mut samples = Vec::new();
    for sample in 0..8 {
        let start = Instant::now();
        for n in 0..100_000_i64 {
            black_box(native.invoke(black_box(&[n % 1000, 500]), black_box(fuel))?);
        }
        if sample != 0 {
            samples.push(start.elapsed().as_nanos());
        }
    }
    samples.sort_unstable();
    println!("Agel lowering: {lowering:?}; native compilation: {compilation:?}");
    println!(
        "Native call wrapper: {:.1} ns/call (median of 7 x 100000, one warmup batch)",
        samples[3] as f64 / 100_000.0
    );
    println!("No model calls. This is a finite integer-function JIT, not complete self-hosting.");
    Ok(())
}

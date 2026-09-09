//! Paired native frontend benchmark: same IR/input, compilation excluded.
use agel_core::{EvaluationOptions, World};
use agel_jit::managed::{Limits, Native, NativeOptions};
use std::{hint::black_box, time::Instant};

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
    let ir = world
        .evaluate_with("(native-compile native-compiler-source)", &options)?
        .values
        .pop()
        .unwrap();
    let baseline = Native::compile_with(
        &ir,
        NativeOptions {
            tail_calls: false,
            cache_builtins: false,
            collection_interval: 0,
        },
    )?;
    let optimized = Native::compile(&ir)?;
    let mut samples = [Vec::new(), Vec::new()];
    let mut stats = [(0, 0, 0); 2];
    for round in 0..8 {
        let order = if round % 2 == 0 { [0, 1] } else { [1, 0] };
        for index in order {
            let native = if index == 0 { &baseline } else { &optimized };
            let start = Instant::now();
            let result = native.invoke_refs(black_box(&[&source]), Limits::default())?;
            let elapsed = start.elapsed().as_nanos();
            assert_eq!(result.value, ir);
            if round > 0 {
                samples[index].push(elapsed);
            }
            stats[index] = (
                result.allocated_values,
                result.fuel_used,
                result.peak_call_depth,
            );
        }
    }
    for i in 0..2 {
        samples[i].sort_unstable();
        println!(
            "{}: {:.3} ms, {} allocations, {} fuel, peak call depth {}",
            if i == 0 { "baseline " } else { "optimized" },
            samples[i][3] as f64 / 1_000_000.0,
            stats[i].0,
            stats[i].1,
            stats[i].2
        );
    }
    println!("Same compiler IR/input; median of 7 alternating runs after warmup. Native invocation includes arena/import/export; excludes parsing and compilation. No models.");
    Ok(())
}

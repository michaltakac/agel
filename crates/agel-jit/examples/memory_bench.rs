//! Paired retained-slot benchmark, not an RSS measurement.
use agel_core::{EvaluationOptions, Value, World};
use agel_jit::managed::{Limits, Native, NativeOptions};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default())?;
    let ir = world
        .evaluate(
            "(import agel/native) (native-compile
      '(fn (n) (let ((loop (fn (self n acc)
          (if (= n 0) acc (self self (- n 1) (+ acc 1)))))) (loop loop n 0))))",
        )?
        .values
        .pop()
        .unwrap();
    let old = Native::compile_with(
        &ir,
        NativeOptions {
            collection_interval: 0,
            ..NativeOptions::default()
        },
    )?;
    let new = Native::compile(&ir)?;
    let mut samples = [Vec::new(), Vec::new()];
    for round in 0..8 {
        for index in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
            let code = if index == 0 { &old } else { &new };
            let start = Instant::now();
            let result = code.invoke(&[Value::Int(10_000)], Limits::default())?;
            let elapsed = start.elapsed().as_nanos();
            assert_eq!(result.value, Value::Int(10_000));
            if round > 0 {
                samples[index].push(elapsed);
            }
            if round == 7 {
                println!("{}: peak arena slots {}, cumulative allocations {}, reclaimed {}, collections {}, fuel {}",
                    if index == 0 { "no collection" } else { "collection   " }, result.peak_arena_slots,
                    result.allocated_values, result.reclaimed_slots, result.collections, result.fuel_used);
            }
        }
    }
    for (index, sample) in samples.iter_mut().enumerate() {
        sample.sort_unstable();
        println!(
            "{} median: {:.3} ms",
            if index == 0 {
                "no collection"
            } else {
                "collection"
            },
            sample[3] as f64 / 1_000_000.0
        );
    }
    println!("10,000 tail steps, same IR/input/options except collection. Seven alternating samples after warmup; invocation includes import/export, excludes compilation. Arena slots exclude GC scratch buffers and allocator overhead; not RSS. No models.");
    Ok(())
}

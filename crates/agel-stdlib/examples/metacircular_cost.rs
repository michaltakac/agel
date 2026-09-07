//! Deterministic evaluator work, not wall-clock benchmarks or model tokens.
use agel_core::{EvaluationOptions, World};

fn main() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    world
        .evaluate("(import agel/meta) (def environment (meta-base-env))")
        .unwrap();
    for source in [
        "(+ 20 22)",
        "(let ((x 40)) ((fn (y) (+ x y)) 2))",
        "((fn (self) (self self 5)) (fn (self n) (if (= n 0) 1 (* n (self self (- n 1))))))",
    ] {
        let direct = world.evaluate(source).unwrap();
        let interpreted = world
            .evaluate(&format!("(meta-eval '{source} environment)"))
            .unwrap();
        assert_eq!(direct.values, interpreted.values);
        println!(
            "{source}\n  seed: {} steps; Agel interpreter: {} steps; {:.1}x; model calls: 0",
            direct.steps_used,
            interpreted.steps_used,
            interpreted.steps_used as f64 / direct.steps_used as f64
        );
    }
}

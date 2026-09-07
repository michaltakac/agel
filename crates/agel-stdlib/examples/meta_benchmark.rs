//! Release-mode end-to-end batches: includes parsing, world transaction and commit.
use agel_core::{EvaluationOptions, Value, World};
use std::time::Instant;

fn measure(world: &World, source: &str, expected: &Value) -> (u128, u64) {
    let batch = format!("(begin {})", vec![source; 20].join(" "));
    let mut times = Vec::new();
    let mut steps = 0;
    for sample in 0..8 {
        let mut candidate = world.fork_isolated();
        let mut options = EvaluationOptions::default();
        options.budget.fuel = 10_000_000;
        let start = Instant::now();
        let commit = candidate.evaluate_with(&batch, &options).unwrap();
        let elapsed = start.elapsed().as_nanos();
        assert_eq!(commit.values.last(), Some(expected));
        steps = commit.steps_used;
        if sample != 0 {
            times.push(elapsed);
        }
    }
    times.sort_unstable();
    (times[times.len() / 2] / 20, steps / 20)
}

fn main() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    world
        .evaluate("(import agel/meta) (def environment (meta-base-env))")
        .unwrap();
    for (name, source, expected) in [
        ("arithmetic", "(+ 20 22)", Value::Int(42)),
        (
            "lexical",
            "(let ((x 40)) ((fn (y) (+ x y)) 2))",
            Value::Int(42),
        ),
        (
            "factorial",
            "((fn (self) (self self 5)) (fn (self n) (if (= n 0) 1 (* n (self self (- n 1))))))",
            Value::Int(120),
        ),
    ] {
        let direct = measure(&world, source, &expected);
        let interpreted = measure(
            &world,
            &format!("(meta-eval '{source} environment)"),
            &expected,
        );
        let mut prepared = world.fork_isolated();
        let start = Instant::now();
        let preparation = prepared
            .evaluate(&format!("(def plan (meta-analyze '{source}))"))
            .unwrap();
        let prepare_ns = start.elapsed().as_nanos();
        let analyzed = measure(&prepared, "(plan environment)", &expected);
        println!("{name}: seed={}ns/{}steps interpreted={}ns/{}steps analyzed={}ns/{}steps prepare={}ns/{}steps speedup={:.2}x",
            direct.0, direct.1, interpreted.0, interpreted.1, analyzed.0, analyzed.1,
            prepare_ns, preparation.steps_used, interpreted.0 as f64 / analyzed.0 as f64);
    }
}

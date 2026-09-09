use agel_core::{EvaluationOptions, Value, World};
use agel_jit::{
    managed::{Fault, Limits, Native, NativeOptions},
    state::{CommitError, NativeState},
};

fn compile(source: &str, interval: usize) -> Native {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    let ir = world
        .evaluate(&format!("(import agel/native) (native-compile '{source})"))
        .unwrap()
        .values
        .pop()
        .unwrap();
    Native::compile_with(
        &ir,
        NativeOptions {
            collection_interval: interval,
            ..NativeOptions::default()
        },
    )
    .unwrap()
}
const LOOP: &str = "(fn (n) (let ((loop (fn (self n acc) (if (= n 0) acc (self self (- n 1) (+ acc 1)))))) (loop loop n 0)))";

#[test]
fn collection_reduces_retention_without_resetting_cumulative_quotas() {
    let baseline = compile(LOOP, 0)
        .invoke(&[Value::Int(2000)], Limits::default())
        .unwrap();
    let code = compile(LOOP, 256);
    let collected = code.invoke(&[Value::Int(2000)], Limits::default()).unwrap();
    assert_eq!(baseline.value, collected.value);
    assert_eq!(baseline.allocated_values, collected.allocated_values);
    assert_eq!(baseline.allocated_edges, collected.allocated_edges);
    assert_eq!(baseline.collections, 0);
    assert!(collected.collections > 10);
    assert!(collected.reclaimed_slots > 10_000);
    assert!(collected.peak_arena_slots * 10 < baseline.peak_arena_slots);
    assert!(collected.fuel_used > baseline.fuel_used);
    assert_eq!(
        code.invoke(
            &[Value::Int(2000)],
            Limits {
                values: baseline.allocated_values - 1,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Fault::Heap
    );
    assert_eq!(
        code.invoke(
            &[Value::Int(2000)],
            Limits {
                fuel: baseline.fuel_used,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Fault::Fuel
    );
}

#[test]
fn captured_frames_shared_maps_and_caches_survive_repeated_compaction() {
    let source = "(fn (n)
      (let ((make (fn (x) (fn (y) (+ x y)))))
        (let ((data (dict 'f (make 7) 'payload '(nil #t \"ľšč🙂\" (x y)))))
          (let ((loop (fn (self n data)
                        (if (= n 0) (list ((get data 'f) 35) (get data 'payload))
                            (begin (list n n n) (self self (- n 1) data))))))
            (loop loop n data)))))";
    let baseline = compile(source, 0)
        .invoke(&[Value::Int(500)], Limits::default())
        .unwrap();
    for interval in [1, 256, 1024, 4096] {
        let code = compile(source, interval);
        for _ in 0..3 {
            let result = code.invoke(&[Value::Int(500)], Limits::default()).unwrap();
            assert_eq!(result.value, baseline.value);
            assert!(result.collections > 0);
        }
    }
}

#[test]
fn suspended_native_callers_are_not_untraced_safepoints() {
    let source = "(fn () (+ 1 (let ((loop (fn (self n) (if (= n 0) 41 (self self (- n 1)))))) (loop loop 1000))))";
    // The outer addition retains native temporaries while the inner loop runs.
    let result = compile(source, 256).invoke(&[], Limits::default()).unwrap();
    assert_eq!(result.value, Value::Int(42));
    assert_eq!(result.collections, 0);
}

#[test]
fn live_growing_data_is_traced_and_not_mistaken_for_garbage() {
    let source = "(fn () (let ((loop (fn (self n xs) (if (= n 0) xs (self self (- n 1) (cons n xs)))))) (loop loop 500 nil)))";
    let result = compile(source, 256).invoke(&[], Limits::default()).unwrap();
    let baseline = compile(source, 0).invoke(&[], Limits::default()).unwrap();
    assert_eq!(result.value, baseline.value);
    assert!(result.collections > 0);
}

#[test]
fn failed_collected_transactions_keep_the_original_state_and_revision() {
    let source = "(fn (state n) (let ((loop (fn (self n acc) (if (= n 0) acc (self self (- n 1) (+ acc 1)))))) (loop loop n state)))";
    let mut state = NativeState::new(compile(source, 256), Value::Int(40));
    for fuel in [0, 400, 2000, 10_000] {
        assert_eq!(
            state
                .transact(
                    0,
                    &Value::Int(1000),
                    Limits {
                        fuel,
                        ..Limits::default()
                    }
                )
                .unwrap_err(),
            CommitError::Execution(Fault::Fuel)
        );
        assert_eq!(state.state(), &Value::Int(40));
        assert_eq!(state.revision(), 0);
    }
    let receipt = state
        .transact(0, &Value::Int(1000), Limits::default())
        .unwrap();
    assert_eq!(state.state(), &Value::Int(1040));
    assert!(receipt.collections > 0);
    assert!(receipt.reclaimed_slots > 0);
}

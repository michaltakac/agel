use agel_core::{EvaluationOptions, Value, World};
use agel_jit::managed::{Fault, Limits, Native, NativeOptions};

fn ir(source: &str) -> Value {
    let mut w = World::default();
    agel_stdlib::install(&mut w, &EvaluationOptions::default()).unwrap();
    w.evaluate(&format!("(import agel/native) (native-compile '{source})"))
        .unwrap()
        .values
        .pop()
        .unwrap()
}
const LOOP: &str = "(fn (n) (let ((loop (fn (self n acc) (if (= n 0) acc (self self (- n 1) (+ acc 1)))))) (loop loop n 0)))";

#[test]
fn long_tail_recursion_is_stack_bounded_but_still_metered() {
    let code = Native::compile(&ir(LOOP)).unwrap();
    let limits = Limits {
        call_depth: 4,
        ..Limits::default()
    };
    let result = code.invoke(&[Value::Int(10_000)], limits).unwrap();
    assert_eq!(result.value, Value::Int(10_000));
    assert!(result.peak_call_depth <= 3);
    assert!(result.tail_calls >= 10_000);
    assert_eq!(
        code.invoke(
            &[Value::Int(10_000)],
            Limits {
                fuel: 1000,
                ..limits
            }
        )
        .unwrap_err(),
        Fault::Fuel
    );
    assert_eq!(
        code.invoke(
            &[Value::Int(10_000)],
            Limits {
                values: 100,
                ..limits
            }
        )
        .unwrap_err(),
        Fault::Heap
    );
}

#[test]
fn optimization_switches_preserve_values_and_reduce_allocations() {
    let ir = ir(LOOP);
    let baseline = Native::compile_with(
        &ir,
        NativeOptions {
            tail_calls: false,
            cache_builtins: false,
            collection_interval: 0,
        },
    )
    .unwrap();
    let cached = Native::compile_with(
        &ir,
        NativeOptions {
            tail_calls: false,
            cache_builtins: true,
            collection_interval: 0,
        },
    )
    .unwrap();
    let optimized = Native::compile(&ir).unwrap();
    let old = baseline
        .invoke(&[Value::Int(100)], Limits::default())
        .unwrap();
    let cache = cached
        .invoke(&[Value::Int(100)], Limits::default())
        .unwrap();
    let new = optimized
        .invoke(&[Value::Int(100)], Limits::default())
        .unwrap();
    assert_eq!(old.value, cache.value);
    assert_eq!(old.value, new.value);
    assert!(cache.allocated_values < old.allocated_values);
    assert!(new.peak_call_depth < old.peak_call_depth);
    assert_eq!(old.tail_calls, 0);
}

#[test]
fn tail_apply_nested_calls_and_lazy_branches_preserve_control_flow() {
    for (source, expected) in [
        ("(fn () (let ((f (fn (self n) (if (= n 0) 41 (apply self (list self (- n 1))))))) (+ 1 (f f 1000))))", Value::Int(42)),
        ("(fn () (if #t (apply apply (list + '(20 22))) (/ 1 0)))", Value::Int(42)),
        ("(fn () (let ((f (fn () 7))) (begin (f) (+ 1 (f)))))", Value::Int(8)),
        ("(fn () ((fn (x) x) ((fn () 42))))", Value::Int(42)),
    ] {
        let code = Native::compile(&ir(source)).unwrap();
        assert_eq!(code.invoke(&[], Limits { call_depth: 8, ..Limits::default() }).unwrap().value, expected);
    }
    for (source, fault) in [
        ("(fn () (apply 1 nil))", Fault::Type),
        ("(fn () (apply (fn (x) x) nil))", Fault::Arity),
        ("(fn () (apply apply nil))", Fault::Arity),
        ("(fn () ((fn () (/ 1 0))))", Fault::DivisionByZero),
    ] {
        assert_eq!(
            Native::compile(&ir(source))
                .unwrap()
                .invoke(&[], Limits::default())
                .unwrap_err(),
            fault
        );
    }
}

#[test]
fn forged_tail_annotations_are_rejected_and_v1_is_still_supported() {
    let mut w = World::default();
    for text in [
        "(agel/native-v1 (fn 0 (tail-call (builtin +) nil)))",
        "(agel/native-v2 (fn 0 (if (tail-call (builtin +) nil) (const 1) (const 2))))",
        "(agel/native-v2 (fn 0 (begin (tail-call (builtin +) nil) (const 2))))",
        "(agel/native-v2 (fn 0 (call (builtin +) ((tail-call (builtin +) nil)))))",
        "(agel/native-v2 (fn 0 (call (tail-call (builtin +) nil) nil)))",
    ] {
        let value = w
            .evaluate(&format!("'{text}"))
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert!(
            matches!(Native::compile(&value), Err(Fault::Invalid(_))),
            "{text}"
        );
    }
    let value = w
        .evaluate("'(agel/native-v1 (fn 0 (call (builtin +) ((const 20) (const 22)))))")
        .unwrap()
        .values
        .pop()
        .unwrap();
    assert_eq!(
        Native::compile(&value)
            .unwrap()
            .invoke(&[], Limits::default())
            .unwrap()
            .value,
        Value::Int(42)
    );
}

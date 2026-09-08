use agel_core::{EvaluationOptions, Value, World};
use agel_jit::{Compiled, Error};

fn world() -> World {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    world.evaluate("(import agel/jit)").unwrap();
    world
}

fn lower(world: &mut World, source: &str) -> Value {
    world
        .evaluate(&format!("(jit-compile '{source})"))
        .unwrap()
        .values
        .pop()
        .unwrap()
}

#[test]
fn native_code_matches_agel_source_and_agel_ir_oracle() {
    let mut world = world();
    for source in [
        "(fn (x y) (+ (* x x) y))",
        "(fn (x y) (if (< x y) (- y x) (- x y)))",
        "(fn (x y) (= x y))",
        "(fn (x y) (if 0 (+ x y) (- x y)))",
        "(fn (x y) (if (< x y) (< x 0) (= y 0)))",
        "(fn (x y) (if #f (+ 9223372036854775807 1) (+ x y)))",
    ] {
        let ir = lower(&mut world, source);
        let native = Compiled::compile(&ir).unwrap();
        assert!(native.clif().contains("return"));
        for x in [-10, -1, 0, 1, 10] {
            for y in [-10, -1, 0, 1, 10] {
                let expected = world
                    .evaluate(&format!("({source} {x} {y})"))
                    .unwrap()
                    .values
                    .pop()
                    .unwrap();
                let oracle = world
                    .evaluate(&format!("(jit-run '{ir} (list {x} {y}))"))
                    .unwrap()
                    .values
                    .pop()
                    .unwrap();
                assert_eq!(oracle, expected);
                assert_eq!(
                    native.invoke(&[x, y], native.required_fuel()).unwrap(),
                    expected
                );
            }
        }
    }
}

#[test]
fn arithmetic_overflow_is_an_error_not_a_machine_trap() {
    let mut world = world();
    for (source, args) in [
        ("(fn (x y) (+ x y))", [i64::MAX, 1]),
        ("(fn (x y) (- x y))", [i64::MIN, 1]),
        ("(fn (x y) (* x y))", [i64::MIN, -1]),
        ("(fn (x y) (* x y))", [i64::MAX, 2]),
    ] {
        let native = Compiled::compile(&lower(&mut world, source)).unwrap();
        assert_eq!(
            native.invoke(&args, native.required_fuel()),
            Err(Error::Overflow)
        );
        assert!(native.invoke(&[2, 1], native.required_fuel()).is_ok());
    }
}

#[test]
fn signed_multiplication_matches_checked_rust_at_boundaries() {
    let mut world = world();
    let native = Compiled::compile(&lower(&mut world, "(fn (x y) (* x y))")).unwrap();
    for left in [
        i64::MIN,
        i64::MIN + 1,
        -3_037_000_500,
        -1,
        0,
        1,
        3_037_000_500,
        i64::MAX,
    ] {
        for right in [i64::MIN, -2, -1, 0, 1, 2, i64::MAX] {
            let expected = left
                .checked_mul(right)
                .map(Value::Int)
                .ok_or(Error::Overflow);
            assert_eq!(
                native.invoke(&[left, right], native.required_fuel()),
                expected
            );
        }
    }
}

#[test]
fn wrong_arity_and_insufficient_fuel_never_enter_native_code() {
    let mut world = world();
    let native = Compiled::compile(&lower(&mut world, "(fn (x) (+ x 1))")).unwrap();
    assert_eq!(native.invoke(&[], 100), Err(Error::Arity));
    assert_eq!(native.invoke(&[1, 2], 100), Err(Error::Arity));
    assert_eq!(
        native.invoke(&[41], native.required_fuel() - 1),
        Err(Error::Fuel)
    );
    assert_eq!(
        native.invoke(&[41], native.required_fuel()),
        Ok(Value::Int(42))
    );
}

#[test]
fn malformed_ir_is_rejected_before_codegen() {
    let mut world = world();
    for source in [
        "nil",
        "'(agel/jit-v2 0 (i64 42))",
        "'(agel/jit-v1 -1 (i64 42))",
        "'(agel/jit-v1 9 (i64 42))",
        "'(agel/jit-v1 1 (arg 1))",
        "'(agel/jit-v1 1 (arg -1))",
        "'(agel/jit-v1 0 (call libc))",
        "'(agel/jit-v1 0 (i64 #t))",
        "'(agel/jit-v1 0 (bool 1))",
        "'(agel/jit-v1 0 (add (bool #t) (i64 1)))",
        "'(agel/jit-v1 0 (if (bool #t) (i64 1) (bool #f)))",
        "'(agel/jit-v1 0 (i64 42 extra))",
    ] {
        let value = world.evaluate(source).unwrap().values.pop().unwrap();
        assert!(
            matches!(Compiled::compile(&value), Err(Error::Invalid(_))),
            "{source}"
        );
    }
    let mut node = Value::List(vec![Value::Symbol("i64".into()), Value::Int(1)]);
    for _ in 0..40 {
        node = Value::List(vec![
            Value::Symbol("add".into()),
            node.clone(),
            Value::List(vec![Value::Symbol("i64".into()), Value::Int(1)]),
        ]);
    }
    let ir = Value::List(vec![
        Value::Symbol("agel/jit-v1".into()),
        Value::Int(0),
        node,
    ]);
    assert!(matches!(Compiled::compile(&ir), Err(Error::Invalid(_))));
    // A shallow, wide tree must also hit the total-node limit.
    let mut node = Value::List(vec![Value::Symbol("i64".into()), Value::Int(1)]);
    for _ in 0..8 {
        node = Value::List(vec![Value::Symbol("add".into()), node.clone(), node]);
    }
    let ir = Value::List(vec![
        Value::Symbol("agel/jit-v1".into()),
        Value::Int(0),
        node,
    ]);
    assert!(matches!(Compiled::compile(&ir), Err(Error::Invalid(_))));
}

#[test]
fn unsupported_source_is_not_silently_reinterpreted() {
    let mut world = world();
    for source in [
        "(fn (x x) x)",
        "(fn (+) (+ 1 2))",
        "(fn (x) unknown)",
        "(fn (x) (/ x 2))",
        "(fn (x) (send x 42))",
        "(fn (x) x x)",
    ] {
        assert!(world.evaluate(&format!("(jit-compile '{source})")).is_err());
    }
}

#[test]
fn executable_owners_can_be_created_used_and_dropped_repeatedly() {
    let mut world = world();
    let ir = lower(&mut world, "(fn () 42)");
    for _ in 0..30 {
        let native = Compiled::compile(&ir).unwrap();
        assert_eq!(native.invoke(&[], 1), Ok(Value::Int(42)));
    }
}

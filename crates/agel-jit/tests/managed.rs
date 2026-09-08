use agel_core::{EvaluationOptions, Value, World};
use agel_jit::managed::{Fault, Limits, Native};

fn world() -> World {
    let mut w = World::default();
    agel_stdlib::install(&mut w, &EvaluationOptions::default()).unwrap();
    w.evaluate("(import agel/native)").unwrap();
    w
}
fn evaluate(w: &mut World, source: &str) -> Value {
    let mut options = EvaluationOptions::default();
    options.budget.fuel = 10_000_000;
    w.evaluate_with(source, &options)
        .unwrap()
        .values
        .pop()
        .unwrap()
}
fn compile(w: &mut World, source: &str) -> Native {
    Native::compile(&evaluate(w, &format!("(native-compile '{source})"))).unwrap()
}

#[test]
fn native_lexical_closures_collections_and_calls_match_seed() {
    let mut w = world();
    for expression in [
        "(let ((x 20)) (let ((x 7) (y x)) (+ x y)))",
        "(let ((x 1) (x 2)) x)",
        "(let ((make (fn (x) (fn (y) (+ x y))))) (let ((f (make 10))) (f 32)))",
        "(let ((+ (fn (a b) (- a b)))) (+ 10 3))",
        "(let ((f (fn (x) x (+ x 1)))) (f 41))",
        "(let ((fns (list (fn (x) (+ x 2)) (fn (x) (* x 2))))) ((car (cdr fns)) 21))",
        "(let ((m (dict 'f (fn (x) (+ x 10))))) ((get m 'f) 32))",
        "(apply + '(1 2 3 4))",
        "(list (+) (*) (- 5) (- 10 2 3) (/ 20 2 2))",
        "(if 0 42 (/ 1 0))",
        "(if nil (/ 1 0) 42)",
        "(begin)",
        "(list (car nil) (cdr nil) (list) (count \"ľščť🙂\"))",
        "(list (type-of +) (type-of (fn () 0)))",
        "(let ((m (dict '(a b) 1 'x 2 '(a b) 3))) (list (get m '(a b)) (keys m) (dissoc m 'x)))",
        "(let ((m (dict 'x 1))) (list m (assoc m 'x 2) (has-key? m 'x) (get m 'missing)))",
        "(list (= '(1 (2 3)) '(1 (2 3))) (= (dict 'x 1) (dict 'x 1)) (keys (dict)))",
        "(let ((f (fn (self n) (if (= n 0) 1 (* n (self self (- n 1))))))) (f f 10))",
        "(let ((map (fn (self f xs) (if (= xs nil) nil (cons (f (car xs)) (self self f (cdr xs))))))) (map map (fn (x) (* x x)) '(1 2 3 4)))",
    ] {
        let source = format!("(fn () {expression})");
        let native = compile(&mut w, &source);
        let expected = evaluate(&mut w, expression);
        assert_eq!(native.invoke(&[], Limits::default()).unwrap().value, expected, "{expression}");
    }
}

#[test]
fn budgets_cover_input_collections_output_expansion_and_small_host_stacks() {
    let mut w = world();
    let identity = compile(&mut w, "(fn (x) x)");
    assert_eq!(
        identity
            .invoke(
                &[Value::String("large".into())],
                Limits {
                    text_bytes: 2,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Heap
    );
    assert_eq!(
        identity
            .invoke(
                &[Value::List(vec![Value::Int(1); 10])],
                Limits {
                    edges: 2,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Heap
    );
    assert_eq!(
        identity
            .invoke(&[Value::Agent(1)], Limits::default())
            .unwrap_err(),
        Fault::NonData
    );

    let duplicate = compile(&mut w, "(fn (n) (let ((f (fn (self n x) (if (= n 0) x (self self (- n 1) (list x x)))))) (f f n '(1))))");
    assert_eq!(
        duplicate
            .invoke(
                &[Value::Int(15)],
                Limits {
                    values: 3000,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Heap
    );
    let result = duplicate
        .invoke(&[Value::Int(3)], Limits::default())
        .unwrap();
    assert_eq!(
        duplicate
            .invoke(
                &[Value::Int(3)],
                Limits {
                    fuel: result.fuel_used,
                    ..Limits::default()
                }
            )
            .unwrap()
            .value,
        result.value
    );
    assert_eq!(
        duplicate
            .invoke(
                &[Value::Int(3)],
                Limits {
                    fuel: result.fuel_used - 1,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Fuel
    );
    assert_eq!(
        duplicate
            .invoke(
                &[Value::Int(3)],
                Limits {
                    values: result.allocated_values - 1,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Heap
    );
    assert_eq!(
        duplicate
            .invoke(
                &[Value::Int(3)],
                Limits {
                    edges: result.allocated_edges - 1,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Heap
    );

    let ir = evaluate(
        &mut w,
        "(native-compile '(fn () (let ((f (fn (self) (+ 1 (self self))))) (f f))))",
    );
    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || {
            let code = Native::compile(&ir).unwrap();
            assert_eq!(
                code.invoke(&[], Limits::default()).unwrap_err(),
                Fault::Depth
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn collection_graph_depth_and_forged_ir_size_are_bounded() {
    let mut w = world();
    let grow = compile(&mut w, "(fn () (let ((f (fn (self n x) (if (= n 0) x (self self (- n 1) (list x)))))) (f f 140 nil)))");
    assert_eq!(
        grow.invoke(&[], Limits::default()).unwrap_err(),
        Fault::Depth
    );
    let mut data = Value::Nil;
    for _ in 0..140 {
        data = Value::List(vec![data]);
    }
    let ir = Value::List(vec![
        Value::Symbol("agel/native-v1".into()),
        Value::List(vec![
            Value::Symbol("fn".into()),
            Value::Int(0),
            Value::List(vec![Value::Symbol("const".into()), data]),
        ]),
    ]);
    assert!(matches!(Native::compile(&ir), Err(Fault::Invalid(_))));
    let node = Value::List(vec![Value::Symbol("const".into()), Value::Int(1)]);
    let mut body = vec![Value::Symbol("begin".into())];
    body.extend(std::iter::repeat_n(node, 17_000));
    let ir = Value::List(vec![
        Value::Symbol("agel/native-v1".into()),
        Value::List(vec![
            Value::Symbol("fn".into()),
            Value::Int(0),
            Value::List(body),
        ]),
    ]);
    assert!(matches!(Native::compile(&ir), Err(Fault::Invalid(_))));
}

#[test]
fn compiler_compiles_itself_and_bootstrap_stages_agree() {
    let mut w = world();
    let source = evaluate(&mut w, "native-compiler-source");
    let seed_ir = evaluate(&mut w, "(native-compile native-compiler-source)");
    let stage_one = Native::compile(&seed_ir).unwrap();
    let stage_one_ir = stage_one
        .invoke(std::slice::from_ref(&source), Limits::default())
        .unwrap();
    assert_eq!(stage_one_ir.value, seed_ir);
    let stage_two = Native::compile(&stage_one_ir.value).unwrap();
    assert_eq!(
        stage_two
            .invoke(std::slice::from_ref(&source), Limits::default())
            .unwrap()
            .value,
        seed_ir
    );
    for text in [
        "(fn (x) (+ x 1))",
        "(fn (x) (let ((f (fn (y) (+ x y)))) (f 2)))",
        "(fn (x) (get (dict 'value x) 'value))",
    ] {
        let source = evaluate(&mut w, &format!("'{text}"));
        let ir = evaluate(&mut w, &format!("(native-compile '{text})"));
        for stage in [&stage_one, &stage_two] {
            let compiled = stage
                .invoke(std::slice::from_ref(&source), Limits::default())
                .unwrap()
                .value;
            assert_eq!(compiled, ir);
            let expected = evaluate(&mut w, &format!("({text} 40)"));
            assert_eq!(
                Native::compile(&compiled)
                    .unwrap()
                    .invoke(&[Value::Int(40)], Limits::default())
                    .unwrap()
                    .value,
                expected
            );
        }
    }
}

#[test]
fn limits_errors_and_repeated_invocations_are_isolated() {
    let mut w = world();
    let loop_code = compile(
        &mut w,
        "(fn () (let ((f (fn (self) (+ 1 (self self))))) (f f)))",
    );
    assert_eq!(
        loop_code
            .invoke(
                &[],
                Limits {
                    fuel: 200,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Fuel
    );
    assert_eq!(
        loop_code
            .invoke(
                &[],
                Limits {
                    call_depth: 8,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Depth
    );
    let code = compile(&mut w, "(fn (x) (list (+ x 1) x))");
    for _ in 0..10 {
        assert_eq!(
            code.invoke(
                &[Value::Int(1)],
                Limits {
                    values: 2,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
            Fault::Heap
        );
        assert_eq!(
            code.invoke(&[Value::Int(i64::MAX)], Limits::default())
                .unwrap_err(),
            Fault::Overflow
        );
        assert_eq!(
            code.invoke(&[Value::Int(1)], Limits::default())
                .unwrap()
                .value,
            Value::List(vec![Value::Int(2), Value::Int(1)])
        );
    }
    assert_eq!(
        code.invoke(&[], Limits::default()).unwrap_err(),
        Fault::Arity
    );
    for (source, error) in [
        ("(fn () (/ 1 0))", Fault::DivisionByZero),
        ("(fn () (/ -9223372036854775808 -1))", Fault::Overflow),
        ("(fn () (+ #t 1))", Fault::Type),
        ("(fn () ((fn (x) x)))", Fault::Arity),
        ("(fn () (car 1))", Fault::Type),
        ("(fn () (let ((f (fn () 1))) (= f f)))", Fault::Type),
        ("(fn () (count (dict (fn () 1) 2)))", Fault::Type),
        ("(fn () (count (dict (list +) 2)))", Fault::Type),
        ("(fn () (signal 'stop \"stop\"))", Fault::Signaled),
        ("(fn () (fn () 1))", Fault::NonData),
    ] {
        assert_eq!(
            compile(&mut w, source)
                .invoke(&[], Limits::default())
                .unwrap_err(),
            error
        );
    }
}

#[test]
fn malformed_source_and_forged_ir_are_rejected() {
    let mut w = world();
    for source in [
        "(fn (x x) x)",
        "(fn (x) unknown)",
        "(fn () (if 1 2))",
        "(fn () (let ((x)) x))",
        "(fn () (send 1 2))",
        "(fn () (fn))",
        "(fn (1) 1)",
    ] {
        assert!(
            w.evaluate(&format!("(native-compile '{source})")).is_err(),
            "{source}"
        );
    }
    for ir in [
        "nil",
        "(agel/native-v1 (local 0 0))",
        "(agel/native-v1 (fn 0 (local 0 0)))",
        "(agel/native-v1 (fn 1 (local 1 0)))",
        "(agel/native-v1 (fn -1 (const 1)))",
        "(agel/native-v1 (fn 65 (const 1)))",
        "(agel/native-v1 (fn 0 (builtin send)))",
        "(agel/native-v1 (fn 0 (const 1 2)))",
    ] {
        assert!(
            matches!(
                Native::compile(&evaluate(&mut w, &format!("'{ir}"))),
                Err(Fault::Invalid(_))
            ),
            "{ir}"
        );
    }
}

#[path = "../examples/module_workshop.rs"]
mod workshop;
use agel_core::Value;
use agel_jit::managed::Limits;

#[test]
fn modules_and_macros_compile_through_native_agel() {
    let tools = workshop::Tools::bootstrap().unwrap();
    let (source, program) = tools.link(workshop::PACKAGE, "dock", "behavior").unwrap();
    assert_eq!(
        source.to_string(),
        "(fn (self state message) (+ state (* message 2)))"
    );
    assert_eq!(
        program
            .invoke(
                &[Value::Int(0), Value::Int(40), Value::Int(1)],
                Limits::default()
            )
            .unwrap()
            .value,
        Value::Int(42)
    );
    assert!(tools
        .workbench(workshop::PACKAGE)
        .unwrap()
        .contains("paint s n"));
}

#[test]
fn imports_private_bindings_lexical_scope_and_lazy_templates() {
    let tools = workshop::Tools::bootstrap().unwrap();
    for (source, expected) in [
        ("(module a (export inc) (def hidden 1) (def inc (fn (x) (+ x hidden)))) (module b (import a) (export run) (def run (fn (x) (let ((x (inc x))) ((fn (x) (+ x 1)) x)))))", 42),
        ("(module b (export run) (defsyntax choose (c a b) (if c a b)) (def run (fn (x) (choose #t (+ x 2) (/ 1 0)))))", 42),
        ("(module b (export run) (defsyntax twice (x) (* x 2)) (def run (fn (x) (let ((twice (fn (n) (+ n 2)))) (twice x)))))", 42),
    ] {
        let (_, program) = tools.link(source, "b", "run").unwrap();
        assert_eq!(program.invoke(&[Value::Int(40)], Limits::default()).unwrap().value, Value::Int(expected));
    }
}

#[test]
fn malformed_or_unauthorized_modules_fail_closed() {
    let tools = workshop::Tools::bootstrap().unwrap();
    for source in [
        "(module b (import missing) (export run) (def run (fn () 1)))",
        "(module a (export pub) (def hidden 1) (def pub 2)) (module b (import a) (export run) (def run (fn () hidden)))",
        "(module b (export run) (def run (fn () ambient)))",
        "(module b (export run) (def run (fn () 1)) (def run (fn () 2)))",
        "(module b (export run) (def run (fn () 1))) (module b (export x) (def x 1))",
        "(module b (export missing) (def run (fn () 1)))",
        "(module b (export run run) (def run (fn () 1)))",
        "(module b (export run) (export run) (def run (fn () 1)))",
        "(module b (export run) (def run (fn (+) (+ 1 2))))",
        "(module b (export run) (def run (fn (x x) x)))",
        "(module b (export run) (defsyntax bad (x x) (+ x 1)) (def run (fn () 1)))",
        "(module b (export run) (defsyntax bad (x) (fn (y) x)) (def run (fn () 1)))",
        "(module b (export run) (defsyntax bad (x) (eval x)) (def run (fn () 1)))",
        "(module b (export run) (defsyntax twice (x) (* x 2)) (def run (fn () (twice 1 2))))",
        "(module b (export run) (defsyntax twice (x) (* x 2)) (def run (fn () twice)))",
        "(module b (export run) (def unused (/ 1 0)) (def run (fn () 1)))",
        "(module b (export run) (def run (fn () (let ((x 1) (x 2)) x))))",
    ] {
        assert!(tools.link(source, "b", "run").is_err(), "accepted {source}");
    }
    // Failed linking is pure and leaves reusable tools working.
    assert!(tools.workbench(workshop::PACKAGE).is_ok());
}

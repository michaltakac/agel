use agel_core::{EvaluationOptions, Value, World};
use agel_jit::{
    managed::{Fault, Limits, Native},
    state::{CommitError, NativeState},
};

fn world() -> World {
    let mut w = World::default();
    agel_stdlib::install(&mut w, &EvaluationOptions::default()).unwrap();
    w.evaluate(
        "(import agel/native) (import agel/native-agents) (import agel/native-agent-kernel)",
    )
    .unwrap();
    w
}
fn eval(w: &mut World, text: &str) -> Value {
    let mut options = EvaluationOptions::default();
    options.budget.fuel = 10_000_000;
    w.evaluate_with(text, &options)
        .unwrap()
        .values
        .pop()
        .unwrap()
}
const CATALOG: &str = "(dict
    'relay '(fn (heap message) (dict 'state (+ heap 1) 'outbox (list (list 'sink message))))
    'sink '(fn (heap message) (dict 'state (cons message heap) 'outbox nil)))";
fn code(w: &mut World, catalog: &str) -> Native {
    let source = eval(w, &format!("(native-system-source {catalog})"));
    // Compile the system using the native frontend, not the Rust evaluator.
    let compiler_ir = eval(w, "(native-compile native-compiler-source)");
    let compiler = Native::compile(&compiler_ir).unwrap();
    let ir = compiler.invoke(&[source], Limits::default()).unwrap().value;
    Native::compile(&ir).unwrap()
}
fn initial(w: &mut World, peers: &str) -> Value {
    eval(
        w,
        &format!(
            "(def test-world (native-send (native-send
      (native-spawn (native-spawn (native-empty 8) 'relay 'relay 0 {peers}) 'sink 'sink nil nil)
      'relay 10) 'sink 20))"
        ),
    )
}

#[test]
fn agel_scheduler_and_behaviors_execute_natively_with_fifo_and_atomic_commits() {
    let mut w = world();
    let native = code(&mut w, CATALOG);
    let initial = initial(&mut w, "'(sink)");
    let source = eval(&mut w, &format!("(native-system-source {CATALOG})"));
    let expected = eval(&mut w, &format!("({source} test-world 3)"));
    let mut machine = NativeState::new(native, initial.clone());
    let receipt = machine
        .transact(0, &Value::Int(3), Limits::default())
        .unwrap();
    assert_eq!(machine.state(), &expected);
    assert_eq!(receipt.revision, 1);
    assert!(receipt.tail_calls > 0);
    assert_eq!(field(machine.state(), "turns"), &Value::Int(3));
    assert_eq!(field(machine.state(), "pending"), &Value::Int(0));
    assert_eq!(
        field(field(field(machine.state(), "agents"), "sink"), "state"),
        &Value::List(vec![Value::Int(10), Value::Int(20)])
    );
    assert_eq!(
        machine
            .transact(0, &Value::Int(1), Limits::default())
            .unwrap_err(),
        CommitError::StaleRevision
    );
    assert_eq!(machine.state(), &expected);

    let mut split = NativeState::new(code(&mut w, CATALOG), initial);
    split
        .transact(0, &Value::Int(1), Limits::default())
        .unwrap();
    split
        .transact(1, &Value::Int(2), Limits::default())
        .unwrap();
    assert_eq!(split.state(), machine.state());
}
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    let Value::Map(xs) = value else {
        panic!("not a map")
    };
    &xs.iter()
        .find(|(k, _)| k == &Value::Symbol(key.into()))
        .unwrap()
        .1
}

#[test]
fn failed_permissions_budget_and_behavior_roll_back_state_and_messages() {
    let mut w = world();
    let original = initial(&mut w, "nil");
    let mut machine = NativeState::new(code(&mut w, CATALOG), original.clone());
    assert_eq!(
        machine
            .transact(0, &Value::Int(3), Limits::default())
            .unwrap_err(),
        CommitError::Execution(Fault::Signaled)
    );
    assert_eq!(machine.state(), &original);
    assert_eq!(machine.revision(), 0);

    let original = initial(&mut w, "'(sink)");
    let mut machine = NativeState::new(code(&mut w, CATALOG), original.clone());
    assert_eq!(
        machine
            .transact(
                0,
                &Value::Int(3),
                Limits {
                    fuel: 10,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        CommitError::Execution(Fault::Fuel)
    );
    assert_eq!(machine.state(), &original);
    assert_eq!(machine.revision(), 0);
    machine
        .transact(0, &Value::Int(3), Limits::default())
        .unwrap();

    let bad =
        "(dict 'relay '(fn (heap message) (dict 'state 99 'outbox (list (list 'sink message))))
        'sink '(fn (heap message) (/ 1 0)))";
    let mut machine = NativeState::new(code(&mut w, bad), original.clone());
    // The first turn succeeds internally; the second fails. Neither commits.
    assert_eq!(
        machine
            .transact(0, &Value::Int(2), Limits::default())
            .unwrap_err(),
        CommitError::Execution(Fault::DivisionByZero)
    );
    assert_eq!(machine.state(), &original);
    assert_eq!(machine.revision(), 0);
}

#[test]
fn behavior_sources_cannot_capture_host_world_and_invalid_worlds_fail() {
    let mut w = world();
    for source in [
        "(fn (heap message) world)",
        "(fn (heap message) turns)",
        "(fn (heap message) (send 1 message))",
        "(fn (heap) heap)",
    ] {
        assert!(w
            .evaluate(&format!("(native-system-source (dict 'x '{source}))"))
            .is_err());
    }
    let native = code(&mut w, CATALOG);
    initial(&mut w, "'(sink)");
    for expression in [
        "(assoc test-world 'pending 999)",
        "(assoc test-world 'capacity 1)",
        "(assoc (dissoc test-world 'front) 'extra nil)",
        "(assoc test-world 'back '((missing 1)))",
    ] {
        let bad = eval(&mut w, expression);
        assert!(native
            .invoke(&[bad, Value::Int(0)], Limits::default())
            .is_err());
    }
}

#[test]
fn long_message_loops_are_turn_bounded_without_growing_the_call_stack() {
    let mut w = world();
    let native = code(&mut w, "(dict 'ping '(fn (heap message) (dict 'state (+ heap 1) 'outbox (list (list 'ping message)))))");
    let state = eval(
        &mut w,
        "(native-send (native-spawn (native-empty 1) 'ping 'ping 0 '(ping)) 'ping 1)",
    );
    let mut machine = NativeState::new(native, state);
    let receipt = machine
        .transact(
            0,
            &Value::Int(1000),
            Limits {
                call_depth: 16,
                fuel: 10_000_000,
                ..Limits::default()
            },
        )
        .unwrap();
    assert_eq!(field(machine.state(), "turns"), &Value::Int(1000));
    assert_eq!(field(machine.state(), "pending"), &Value::Int(1));
    assert_eq!(
        field(field(field(machine.state(), "agents"), "ping"), "state"),
        &Value::Int(1000)
    );
    assert!(receipt.peak_call_depth <= 16);
}

#[test]
fn invalid_outboxes_and_non_data_outputs_do_not_commit() {
    let mut w = world();
    for behavior in [
        "(fn (heap message) (dict 'state 99 'outbox (list (list 'ping 1) (list 'ping 2))))",
        "(fn (heap message) (dict 'state 99 'outbox '((missing 1))))",
        "(fn (heap message) (dict 'state 99 'outbox '(wrong-shape)))",
        "(fn (heap message) (dict 'state 99 'outbox nil 'peers '(all)))",
        "(fn (heap message) (dict 'state (fn () 1) 'outbox nil))",
    ] {
        let native = code(&mut w, &format!("(dict 'ping '{behavior})"));
        let initial = eval(
            &mut w,
            "(native-send (native-spawn (native-empty 1) 'ping 'ping 0 '(ping)) 'ping 1)",
        );
        let mut machine = NativeState::new(native, initial.clone());
        assert!(machine
            .transact(0, &Value::Int(1), Limits::default())
            .is_err());
        assert_eq!(machine.state(), &initial);
        assert_eq!(machine.revision(), 0);
    }
}

#[test]
fn empty_systems_and_empty_list_values_are_valid() {
    let mut w = world();
    let native = code(&mut w, "(dict)");
    let empty = eval(
        &mut w,
        "(assoc (assoc (native-empty 1) 'front (keys (dict))) 'back (keys (dict)))",
    );
    assert_eq!(
        native
            .invoke_refs(&[&empty, &Value::Int(10)], Limits::default())
            .unwrap()
            .value,
        empty
    );
    let native = code(
        &mut w,
        "(dict 'sink '(fn (heap message) (dict 'state message 'outbox (keys (dict)))))",
    );
    let initial = eval(
        &mut w,
        "(native-send (native-spawn (native-empty 1) 'sink 'sink nil (keys (dict))) 'sink 42)",
    );
    let result = native
        .invoke(&[initial, Value::Int(1)], Limits::default())
        .unwrap()
        .value;
    assert_eq!(
        field(field(field(&result, "agents"), "sink"), "state"),
        &Value::Int(42)
    );
}

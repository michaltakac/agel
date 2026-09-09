use agel_core::{EvaluationOptions, Value, World};
use agel_jit::{
    managed::{Fault, Limits, Native},
    state::{CommitError, NativeState},
};

#[path = "../examples/live_upgrade.rs"]
mod demo;

fn compile(source: &str) -> Native {
    let mut w = World::default();
    agel_stdlib::install(&mut w, &EvaluationOptions::default()).unwrap();
    let ir = w
        .evaluate(&format!("(import agel/native) (native-compile '{source})"))
        .unwrap()
        .values
        .pop()
        .unwrap();
    Native::compile(&ir).unwrap()
}
const OLD: &str = "(fn (state message) (+ state 1))";
const NEW: &str = "(fn (state message) (+ state 10))";

#[test]
fn native_agent_proposes_composes_and_upgrades_without_evaluator_roundtrips() {
    demo::main().unwrap();
}

#[test]
fn candidates_are_bound_to_machine_and_revision_and_never_commit_probe_output() {
    let mut a = NativeState::new(compile(OLD), Value::Int(0));
    let mut b = NativeState::new(compile(OLD), Value::Int(0));
    let candidate = a
        .preview(0, compile(NEW), &Value::Nil, Limits::default())
        .unwrap();
    assert_eq!(candidate.probe().value, Value::Int(10));
    assert_eq!(
        b.promote(0, candidate).unwrap_err(),
        CommitError::WrongOwner
    );
    assert_eq!(b.revision(), 0);
    let candidate = a
        .preview(0, compile(NEW), &Value::Nil, Limits::default())
        .unwrap();
    a.transact(0, &Value::Nil, Limits::default()).unwrap();
    assert_eq!(
        a.promote(1, candidate).unwrap_err(),
        CommitError::StaleRevision
    );
    assert_eq!(a.state(), &Value::Int(1));
    let candidate = a
        .preview(1, compile(NEW), &Value::Nil, Limits::default())
        .unwrap();
    a.promote(1, candidate).unwrap();
    assert_eq!(a.state(), &Value::Int(1));
    a.transact(2, &Value::Nil, Limits::default()).unwrap();
    assert_eq!(a.state(), &Value::Int(11));
}

#[test]
fn failed_probes_and_rollback_keep_code_state_and_revision_unchanged() {
    let mut machine = NativeState::new(compile(OLD), Value::Int(0));
    let old_ir = machine.program_ir().clone();
    for bad in [
        "(fn (state message) (/ 1 0))",
        "(fn (state message) (fn () 1))",
        "(fn () 1)",
    ] {
        assert!(machine
            .preview(0, compile(bad), &Value::Nil, Limits::default())
            .is_err());
        assert_eq!(machine.program_ir(), &old_ir);
        assert_eq!(machine.revision(), 0);
    }
    assert_eq!(
        machine
            .rollback_program(0, &Value::Nil, Limits::default())
            .unwrap_err(),
        CommitError::NoPreviousProgram
    );
    let candidate = machine
        .preview(
            0,
            compile("(fn (state message) '(not-an-integer))"),
            &Value::Nil,
            Limits::default(),
        )
        .unwrap();
    machine.promote(0, candidate).unwrap();
    machine.transact(1, &Value::Nil, Limits::default()).unwrap();
    let before = machine.state().clone();
    let code = machine.program_ir().clone();
    assert_eq!(
        machine
            .rollback_program(2, &Value::Nil, Limits::default())
            .unwrap_err(),
        CommitError::Execution(Fault::Type)
    );
    assert_eq!(machine.state(), &before);
    assert_eq!(machine.program_ir(), &code);
    assert_eq!(machine.revision(), 2);
}

#[test]
fn rollback_is_code_only_and_can_toggle_without_rewinding_state() {
    let mut machine = NativeState::new(compile(OLD), Value::Int(5));
    let candidate = machine
        .preview(0, compile(NEW), &Value::Nil, Limits::default())
        .unwrap();
    machine.promote(0, candidate).unwrap();
    let new_ir = machine.program_ir().clone();
    assert_eq!(
        machine
            .rollback_program(
                1,
                &Value::Nil,
                Limits {
                    fuel: 0,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        CommitError::Execution(Fault::Fuel)
    );
    assert_eq!(machine.program_ir(), &new_ir);
    assert_eq!(machine.revision(), 1);
    machine
        .rollback_program(1, &Value::Nil, Limits::default())
        .unwrap();
    assert_eq!(machine.state(), &Value::Int(5));
    machine
        .rollback_program(2, &Value::Nil, Limits::default())
        .unwrap();
    assert_eq!(machine.program_ir(), &new_ir);
    machine.transact(3, &Value::Nil, Limits::default()).unwrap();
    assert_eq!(machine.state(), &Value::Int(15));
}

#[test]
fn discarded_and_competing_previews_do_not_change_active_code() {
    let mut machine = NativeState::new(compile(OLD), Value::Int(0));
    let original = machine.program_ir().clone();
    let discarded = machine
        .preview(0, compile(NEW), &Value::Nil, Limits::default())
        .unwrap();
    drop(discarded);
    assert_eq!(machine.program_ir(), &original);
    assert_eq!(machine.revision(), 0);
    assert!(matches!(
        machine.preview(
            0,
            compile(NEW),
            &Value::Nil,
            Limits {
                fuel: 0,
                ..Limits::default()
            }
        ),
        Err(CommitError::Execution(Fault::Fuel))
    ));
    let first = machine
        .preview(0, compile(NEW), &Value::Nil, Limits::default())
        .unwrap();
    let second = machine
        .preview(0, compile(NEW), &Value::Nil, Limits::default())
        .unwrap();
    machine.promote(0, first).unwrap();
    assert_eq!(
        machine.promote(1, second).unwrap_err(),
        CommitError::StaleRevision
    );
    assert_eq!(machine.state(), &Value::Int(0));
}

use agel_core::{EvaluationOptions, HostError, HostWord, Value, World};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

static TRACE: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static FAIL_AT: AtomicUsize = AtomicUsize::new(0);

fn append(arguments: &[Value]) -> Result<Value, HostError> {
    let event = arguments[0].clone();
    let mut trace = TRACE.lock().unwrap();
    if trace.len() + 1 == FAIL_AT.load(Ordering::Relaxed) {
        return Err(HostError {
            kind: "disk/full".into(),
            message: "audit write failed".into(),
        });
    }
    trace.push(event);
    Ok(Value::Bool(true))
}

fn invoke(_: &[Value]) -> Result<Value, HostError> {
    TRACE
        .lock()
        .unwrap()
        .push(Value::Symbol("provider-called".into()));
    Ok(Value::Map(vec![
        (Value::Symbol("allowed".into()), Value::Bool(true)),
        (
            Value::Symbol("reason".into()),
            Value::String("fixture".into()),
        ),
        (Value::Symbol("cost-microusd".into()), Value::Int(7)),
    ]))
}

static HOST: [HostWord; 2] = [
    HostWord {
        name: "audit-append",
        capability: "",
        call: append,
    },
    HostWord {
        name: "audit-invoke",
        capability: "",
        call: invoke,
    },
];

#[test]
fn audit_precedes_dispatch_and_survives_transaction_failure() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    world.install_host(&HOST);
    let options = EvaluationOptions {
        host: &HOST,
        ..EvaluationOptions::default()
    };
    let source = "(import agel/judge-log)
        (judge-with-log \"call-1\" (dict 'text \"candidate\") audit-invoke audit-append)";

    world.evaluate_with(source, &options).unwrap();
    let trace = TRACE.lock().unwrap().clone();
    assert_eq!(trace.len(), 3);
    assert!(trace[0].to_string().contains("judge-start"));
    assert_eq!(trace[1], Value::Symbol("provider-called".into()));
    assert!(trace[2].to_string().contains("cost-microusd 7"));

    TRACE.lock().unwrap().clear();
    FAIL_AT.store(1, Ordering::Relaxed);
    assert!(world.evaluate_with(source, &options).is_err());
    assert!(TRACE.lock().unwrap().is_empty());

    FAIL_AT.store(3, Ordering::Relaxed);
    assert!(world.evaluate_with(source, &options).is_err());
    assert_eq!(TRACE.lock().unwrap().len(), 2);

    TRACE.lock().unwrap().clear();
    FAIL_AT.store(0, Ordering::Relaxed);
    let receipt = world
        .evaluate_with(
            "(judge-with-log \"call-2\" nil
           (fn (request) (signal 'provider/down \"provider unavailable\")) audit-append)",
            &options,
        )
        .unwrap();
    assert!(receipt
        .values
        .last()
        .unwrap()
        .to_string()
        .contains("cost-microusd nil"));
    assert_eq!(TRACE.lock().unwrap().len(), 2);
}

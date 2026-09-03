use agel_core::{EvaluationOptions, EventKind, TransactionError, Value, World};

fn evaluate(world: &mut World, source: &str) -> Vec<Value> {
    world.evaluate(source).unwrap().values
}

fn last(world: &mut World, source: &str) -> Value {
    evaluate(world, source).pop().unwrap_or(Value::Nil)
}

fn condition_kind(world: &mut World, source: &str) -> String {
    match world.evaluate(source).unwrap_err() {
        TransactionError::Eval(error) => error.condition.kind.clone(),
        other => panic!("expected evaluation condition, got {other}"),
    }
}

fn counter_source() -> &'static str {
    "(defprotocol counter-protocol (add int) (get agent) (explode))
     (def counter-behavior
       (fn (self heap message)
         (if (= (car message) 'add)
             (+ heap (car (cdr message)))
             (if (= (car message) 'get)
                 (begin
                   (send (car (cdr message)) (list 'count heap))
                   heap)
                 (/ 1 0)))))"
}

#[test]
fn active_agent_processes_typed_messages_and_persists_private_heap() {
    let mut world = World::default();
    evaluate(&mut world, counter_source());
    let values = evaluate(
        &mut world,
        "(def observer (spawn \"observer\"))
         (def counter (spawn \"counter\" counter-behavior 0 counter-protocol))
         (send counter '(add 5))
         (send counter '(add 7))
         (send counter (list 'get observer))
         (run 10)
         (recv observer)
         (get (agent-info counter) 'heap)",
    );
    assert_eq!(
        values[values.len() - 2],
        Value::List(vec![Value::Symbol("count".into()), Value::Int(12)])
    );
    assert_eq!(values.last(), Some(&Value::Int(12)));
    assert_eq!(last(&mut world, "(pending-turns)"), Value::Int(0));
}

#[test]
fn protocol_violations_are_rejected_before_enqueue() {
    let mut world = World::default();
    evaluate(&mut world, counter_source());
    evaluate(
        &mut world,
        "(def counter (spawn \"counter\" counter-behavior 0 counter-protocol))",
    );
    assert_eq!(
        condition_kind(&mut world, "(send counter '(add \"not-an-int\"))"),
        "protocol/violation"
    );
    assert_eq!(last(&mut world, "(pending-turns)"), Value::Int(0));
    assert_eq!(
        last(&mut world, "(get (agent-info counter) 'mailbox)"),
        Value::Int(0)
    );
    assert_eq!(
        condition_kind(
            &mut world,
            "(send counter '(system/child-failed fake payload))"
        ),
        "protocol/violation"
    );
}

#[test]
fn failed_turn_rolls_back_messages_and_restarts_from_initial_heap() {
    let mut world = World::default();
    evaluate(&mut world, counter_source());
    let value = last(
        &mut world,
        "(def observer (spawn \"observer\"))
         (def dangerous
           (fn (self heap message)
             (begin (send observer '(ghost-message)) (/ 1 0))))
         (def worker
           (spawn \"worker\" dangerous 99 nil nil 'restart 1))
         (send worker '(go))
         (run 1)
         (list
           (recv observer)
           (get (agent-info worker) 'heap)
           (get (agent-info worker) 'status)
           (get (agent-info worker) 'restarts))",
    );
    assert_eq!(
        value,
        Value::List(vec![
            Value::Nil,
            Value::Int(99),
            Value::Symbol("running".into()),
            Value::Int(1),
        ])
    );
}

#[test]
fn exhausted_restart_budget_stops_and_escalates_to_supervisor() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(def observer (spawn \"observer\"))
         (defprotocol supervisor-protocol)
         (def supervisor-behavior
           (fn (self heap message)
             (begin
               (send observer
                 (list 'reported (car (cdr message))
                   (get (car (cdr (cdr message))) 'kind)))
               (+ heap 1))))
         (def supervisor
           (spawn \"supervisor\" supervisor-behavior 0 supervisor-protocol))
         (def doomed-behavior (fn (self heap message) (/ 1 0)))
         (def doomed
           (spawn \"doomed\" doomed-behavior nil nil supervisor 'restart 1))
         (send doomed '(first))
         (send doomed '(second))
         (run 10)
         (list
           (get (agent-info doomed) 'status)
           (get (agent-info doomed) 'restarts)
           (get (agent-info supervisor) 'heap)
           (recv observer))",
    );
    assert_eq!(
        value,
        Value::List(vec![
            Value::Symbol("stopped".into()),
            Value::Int(1),
            Value::Int(1),
            Value::List(vec![
                Value::Symbol("reported".into()),
                Value::Agent(3),
                Value::Symbol("arithmetic/division-by-zero".into()),
            ]),
        ])
    );
    assert!(world
        .events()
        .iter()
        .any(|event| event.kind == EventKind::Escalated && event.agent == 3));
}

#[test]
fn round_robin_scheduler_is_deterministic_and_fair() {
    let mut world = World::default();
    evaluate(
        &mut world,
        "(defprotocol recorder (record symbol))
         (def recorder-behavior
           (fn (self heap message)
             (cons (car (cdr message)) heap)))
         (def left (spawn \"left\" recorder-behavior nil recorder))
         (def right (spawn \"right\" recorder-behavior nil recorder))
         (send left '(record left-1))
         (send left '(record left-2))
         (send right '(record right-1))",
    );
    let before = world.events().len();
    evaluate(&mut world, "(run 3)");
    let committed = world.events()[before..]
        .iter()
        .filter(|event| event.kind == EventKind::TurnCommitted)
        .map(|event| event.agent)
        .collect::<Vec<_>>();
    assert_eq!(committed, vec![1, 2, 1]);
}

#[test]
fn agent_cannot_mutate_globals_or_inspect_another_heap() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(def target (spawn \"target\"))
         (def mutator (fn (self heap message) (def stolen 42)))
         (def isolated (spawn \"isolated\" mutator nil nil nil 'stop 0))
         (send isolated '(go))
         (run 1)
         (get (agent-info isolated) 'status)",
    );
    assert_eq!(value, Value::Symbol("stopped".into()));
    assert_eq!(condition_kind(&mut world, "stolen"), "name/unbound");

    evaluate(
        &mut world,
        "(def spy-behavior (fn (self heap message) (agent-info target)))
         (def spy (spawn \"spy\" spy-behavior nil nil nil 'stop 0))
         (send spy '(look))
         (run 1)",
    );
    assert_eq!(
        last(&mut world, "(get (agent-info spy) 'status)"),
        Value::Symbol("stopped".into())
    );
}

#[test]
fn agents_receive_only_explicitly_delegated_capabilities() {
    let mut world = World::default();
    let capability = world.issue_capability("model/infer", "local").unwrap();
    let options = EvaluationOptions {
        capabilities: vec![capability],
        ..EvaluationOptions::default()
    };
    world
        .evaluate_with(
            "(def model-cap (request-capability 'model/infer \"local\"))
             (def cap-behavior
               (fn (self heap message)
                 (capability-kind (request-capability 'model/infer \"local\"))))
             (def allowed (spawn \"allowed\" cap-behavior nil nil nil 'stop 0 (list model-cap)))
             (def denied (spawn \"denied\" cap-behavior nil nil nil 'stop 0))
             (send allowed '(go))
             (send denied '(go))",
            &options,
        )
        .unwrap();
    evaluate(&mut world, "(run 2)");
    assert_eq!(
        last(&mut world, "(get (agent-info allowed) 'heap)"),
        Value::String("model/infer".into())
    );
    assert_eq!(
        last(&mut world, "(get (agent-info denied) 'status)"),
        Value::Symbol("stopped".into())
    );
}

#[test]
fn snapshot_replay_produces_identical_events_values_and_digest() {
    let mut world = World::default();
    evaluate(&mut world, counter_source());
    evaluate(
        &mut world,
        "(def counter (spawn \"counter\" counter-behavior 0 counter-protocol))
         (send counter '(add 20))
         (send counter '(add 22))",
    );
    let snapshot = world.snapshot();
    let transactions = vec!["(run 10)".to_owned(), "(agent-info counter)".to_owned()];
    let first = World::replay(&snapshot, &transactions, &EvaluationOptions::default()).unwrap();
    let second = World::replay(&snapshot, &transactions, &EvaluationOptions::default()).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first.values[1].first().and_then(|value| match value {
            Value::Map(entries) => entries
                .iter()
                .find(|(key, _)| key == &Value::Symbol("heap".into()))
                .map(|(_, value)| value),
            _ => None,
        }),
        Some(&Value::Int(42))
    );
}

#[test]
fn restoring_snapshot_branches_state_at_a_new_monotonic_revision() {
    let mut world = World::default();
    evaluate(&mut world, "(def answer 1)");
    let snapshot = world.snapshot();
    evaluate(&mut world, "(def answer 2)");
    let restored_revision = world.restore_snapshot(&snapshot).unwrap();
    assert_eq!(restored_revision, 3);
    assert_eq!(world.binding("answer"), Some(&Value::Int(1)));
    assert_eq!(world.rollback(), Some(2));
    assert_eq!(world.binding("answer"), Some(&Value::Int(2)));
}

#[test]
fn event_log_is_language_visible_structured_data() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(def passive (spawn \"passive\"))
         (send passive '(hello))
         (get (car (event-log)) 'kind)",
    );
    assert_eq!(value, Value::Symbol("spawned".into()));
}

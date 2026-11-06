use agel_core::{EvaluationOptions, ModelCompletion, World};

fn setup() -> (World, EvaluationOptions) {
    let mut world = World::default();
    let options = EvaluationOptions {
        capabilities: vec![
            world.issue_capability("model/infer", "writer").unwrap(),
            world.issue_capability("model/infer", "judge").unwrap(),
        ],
        ..Default::default()
    };
    agel_stdlib::install(&mut world, &options).unwrap();
    for source in [
        include_str!("../live-tools.agel"),
        include_str!("../repair.agel"),
    ] {
        world.evaluate_with(source, &options).unwrap();
    }
    world
        .evaluate_with(include_str!("repair.agel"), &options)
        .unwrap();
    (world, options)
}

fn reply(world: &mut World, options: &EvaluationOptions, provider: &str, text: &str) {
    let pending = world.pending_model_requests();
    assert_eq!(pending.len(), 1);
    let request = &pending[0];
    assert_eq!(request.provider, provider);
    assert!(!request.prompt.contains("HELD_OUT_SECRET"));
    world.claim_model_request(request.id, options).unwrap();
    world
        .complete_model_request(ModelCompletion::success(request, text), options)
        .unwrap();
    world.evaluate_with("(run)", options).unwrap();
}

const GOOD: &str = "(dict 'diagnosis \"division truncates, round positive inputs without overflow\" 'after '(fn (n size) (if (= n 0) 0 (+ (/ (- n 1) size) 1))) 'predictions '(2 0))";

#[test]
fn general_repair_predicts_judges_replays_and_activates() {
    let (mut world, options) = setup();
    reply(&mut world, &options, "writer", GOOD);
    assert_eq!(
        world
            .evaluate("(get (repair-state worker) 'status)")
            .unwrap()
            .values[0]
            .to_string(),
        "judging"
    );
    reply(
        &mut world,
        &options,
        "judge",
        "patch noul 950 reason choice 5 none 900 900 25 25 25 25",
    );
    let result = world.evaluate("(def state (repair-state worker)) (list (get state 'status) (get (live-tools-call state 'batches '(10 3)) 'value) (evolution-current (get state 'versions) 'cell 'batches) (evolution-source (get state 'versions) 1))").unwrap();
    assert_eq!(
        result.values.last().unwrap().to_string(),
        "(done 4 2 (fn (n size) (/ n size)))"
    );
    let scores = world.evaluate("(list (get (repair-evaluate state 1) 'correct) (get (repair-evaluate state 2) 'correct) (get (repair-evaluate state 2) 'total))").unwrap();
    assert_eq!(scores.values[0].to_string(), "(3 6 6)");
    assert!(world.pending_model_requests().is_empty());
}

#[test]
fn held_out_failures_and_false_predictions_retry_with_reasons() {
    let (mut world, options) = setup();
    reply(
        &mut world,
        &options,
        "writer",
        "(dict 'diagnosis \"round up\" 'after '(fn (n size) (if (= n 0) 0 2)) 'predictions '(2 0))",
    );
    reply(
        &mut world,
        &options,
        "judge",
        "patch noul 999 reason choice 5 none 900 900 25 25 25 25",
    );
    assert!(world.pending_model_requests()[0]
        .prompt
        .contains("recorded step differs"));
    assert!(world.pending_model_requests()[0]
        .prompt
        .contains("(if (= n 0) 0 2)"));
    reply(&mut world, &options, "writer", "(dict 'diagnosis \"repair\" 'after '(fn (n size) (/ (+ n (- size 1)) size)) 'predictions '(9 0))");
    reply(
        &mut world,
        &options,
        "judge",
        "patch noul 999 reason choice 5 none 900 900 25 25 25 25",
    );
    let state = world.evaluate("(list (get (repair-state worker) 'status) (evolution-current (get (repair-state worker) 'versions) 'cell 'batches))").unwrap();
    assert_eq!(state.values[0].to_string(), "(exhausted 1)");
}

#[test]
fn malformed_reports_and_refused_judges_never_activate() {
    let (mut world, options) = setup();
    reply(
        &mut world,
        &options,
        "writer",
        "(send worker '(repair/start))",
    );
    assert_eq!(world.pending_model_requests()[0].provider, "writer");
    reply(&mut world, &options, "writer", GOOD);
    reply(
        &mut world,
        &options,
        "judge",
        "patch noul 1001 reason choice 5 none 900 900 25 25 25 25",
    );
    assert_eq!(
        world
            .evaluate("(get (repair-state worker) 'reason)")
            .unwrap()
            .values[0]
            .to_string(),
        "\"judge score outside 0..1000\""
    );
    assert_eq!(
        world
            .evaluate("(evolution-current (get (repair-state worker) 'versions) 'cell 'batches)")
            .unwrap()
            .values[0]
            .to_string(),
        "1"
    );
    assert!(world.pending_model_requests().is_empty());
}

#[test]
fn refusal_prevents_replay_and_provider_failure_preserves_the_tool() {
    let (mut world, options) = setup();
    reply(
        &mut world,
        &options,
        "writer",
        "(dict 'diagnosis \"unsafe proposal\" 'after '(fn (n size) (/ n 0)) 'predictions '(2 0))",
    );
    reply(
        &mut world,
        &options,
        "judge",
        "patch noul 100 reason choice 5 boundary 900 25 25 900 25 25",
    );
    assert!(world.pending_model_requests()[0]
        .prompt
        .contains("boundary"));
    let request = world.pending_model_requests().remove(0);
    world.claim_model_request(request.id, &options).unwrap();
    world
        .complete_model_request(
            ModelCompletion::failure(&request, "provider/down", "provider unavailable"),
            &options,
        )
        .unwrap();
    world.evaluate_with("(run)", &options).unwrap();
    let result = world.evaluate("(def state (repair-state worker)) (list (get state 'status) (get (live-tools-call state 'batches '(6 3)) 'value) (count (filter (fn (entry) (= (get entry 'event) 'replay)) (get (get state 'versions) 'history))))").unwrap();
    assert_eq!(result.values.last().unwrap().to_string(), "(exhausted 2 1)");
}

#[test]
fn pure_session_uses_the_same_gates_without_dispatching() {
    let (mut world, options) = setup();
    world
        .evaluate_with(
            "(def session (repair-start (repair-new (repair-state worker))))",
            &options,
        )
        .unwrap();
    let source = format!(
        "(def session (repair-complete session (get (repair-pending session) 'id) 'writer {}))",
        agel_core::Value::String(GOOD.into())
    );
    world.evaluate_with(&source, &options).unwrap();
    let status = world.evaluate("(get session 'status)").unwrap();
    assert_eq!(status.values[0].to_string(), "judging");
    world.evaluate_with("(def session (repair-complete session (get (repair-pending session) 'id) 'judge \"patch noul 950 reason choice 5 none 900 900 25 25 25 25\"))", &options).unwrap();
    assert_eq!(world.evaluate("(list (get session 'status) (repair-pending session) (get (repair-evaluate session 2) 'correct))").unwrap().values[0].to_string(), "(done nil 6)");
    assert_eq!(
        world.pending_model_requests().len(),
        1,
        "pure session must not add a runtime request"
    );
}

#[test]
fn native_inference_framing_is_agel_and_preserves_bytes() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../bytes.agel")).unwrap();
    world.evaluate(include_str!("../inference.agel")).unwrap();
    let result = world.evaluate("(import agel/inference) (inference-call (dict 'writer \"writer\") (fn (request) (if (= request \"AGEL-INFER/1 writer\\n6869\") \"ok 410acebb\" (signal 'test/failed \"wrong envelope\"))) 'writer \"hi\")").unwrap();
    assert_eq!(
        result.values.last(),
        Some(&agel_core::Value::String("A\nλ".into()))
    );
    assert!(world.evaluate("(inference-call (dict 'writer \"writer\\njudge\") (fn (request) (signal 'test/failed \"must not dispatch\")) 'writer \"hi\")").unwrap_err().to_string().contains("invalid provider route"));
}

#[test]
fn program_repair_uses_activation_and_synchronous_audit_order() {
    let (mut world, options) = setup();
    world.evaluate_with("(def program-versions (evolution-edit (evolution-new) 'program 'batches nil before (list (dict 'arguments '(6 3) 'expected 2)) (fn (r) (dict 'allowed #t 'reason \"fixture\" 'cost-microusd 0)))) (def activated 0) (def session (repair-start (repair-new (assoc (assoc (assoc (repair-state worker) 'kind 'program) 'versions program-versions) 'activate (fn (r cases) (begin (def activated (+ activated 1)) (dict 'passed #t 'reason \"artifact verified\" 'attempt (get r 'attempt)))))))) (def audit-events nil)", &options).unwrap();
    let call = format!("(def session (repair-exchange session (fn (provider prompt) (begin (def audit-events (append audit-events '(infer))) {})) (fn (record) (def audit-events (append audit-events (list (get record 'event)))))))", agel_core::Value::String(GOOD.into()));
    world.evaluate_with(&call, &options).unwrap();
    assert_eq!(
        world.evaluate("audit-events").unwrap().values[0].to_string(),
        "(inference-start infer inference-result)"
    );
    world.evaluate_with("(def session (repair-complete session (get (repair-pending session) 'id) 'judge \"patch noul 950 reason choice 5 none 900 900 25 25 25 25\"))", &options).unwrap();
    assert_eq!(world.evaluate("(list (get session 'status) activated (evolution-current (get session 'versions) 'program 'batches))").unwrap().values[0].to_string(), "(done 1 2)");
}

#[test]
fn deferred_text_approval_does_not_commit_before_advance() {
    let (mut world, options) = setup();
    world.evaluate_with("(def session (repair-start (repair-new (assoc (repair-state worker) 'defer-activation #t))))", &options).unwrap();
    let source = format!(
        "(def session (repair-complete session (get (repair-pending session) 'id) 'writer {}))",
        agel_core::Value::String(GOOD.into())
    );
    world.evaluate_with(&source, &options).unwrap();
    world.evaluate_with("(def session (repair-complete session (get (repair-pending session) 'id) 'judge \"patch noul 950 reason choice 5 none 900 900 25 25 25 25\"))", &options).unwrap();
    assert_eq!(world.evaluate("(list (get session 'status) (evolution-current (get session 'versions) 'cell 'batches))").unwrap().values[0].to_string(), "(approved 1)");
    world
        .evaluate_with("(def session (repair-advance session))", &options)
        .unwrap();
    assert_eq!(world.evaluate("(list (get session 'status) (evolution-current (get session 'versions) 'cell 'batches))").unwrap().values[0].to_string(), "(done 2)");
}

#[test]
fn hex_text_decoder_preserves_utf8_across_chunk_boundaries() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../bytes.agel")).unwrap();
    let text = format!("{}λ\n{}", "a".repeat(127), "b".repeat(130));
    let result = world
        .evaluate_with(
            &format!(
                "(import agel/bytes) (text-from-hex (text-hex {}))",
                agel_core::Value::String(text.clone())
            ),
            &EvaluationOptions {
                budget: agel_core::Budget {
                    fuel: 1_000_000,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.values.last(), Some(&agel_core::Value::String(text)));
    assert!(world.evaluate("(text-from-hex \"c080\")").is_err());
}

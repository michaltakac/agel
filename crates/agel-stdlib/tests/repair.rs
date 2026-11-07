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
    world
        .evaluate_with(include_str!("../repair.agel"), &options)
        .unwrap();
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

#[test]
fn polling_inference_services_agel_between_polls_and_retains_acknowledged_state() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../bytes.agel")).unwrap();
    world.evaluate(include_str!("../inference.agel")).unwrap();
    let result = world
        .evaluate(
            r#"
(import agel/inference)
(def polls 0)
(def ask (fn (request)
  (if (= request "AGEL-INFER/2 start writer\n6869") "pending 19"
    (if (= request "AGEL-INFER/2 poll 19")
      (begin (def polls (+ polls 1)) (if (= polls 2) "ok 410acebb" "pending 19"))
      (signal 'test/request "unexpected request")))))
(def result (inference-serviced (dict 'writer "writer") ask 'writer "hi" (fn (n) (+ n 1)) 0 8))
(list (get (get result 'reply) 'text) (get result 'state) (get result 'polls))
"#,
        )
        .unwrap();
    assert_eq!(result.values.last().unwrap().to_string(), "(\"A\\nλ\" 2 2)");
    assert_eq!(world.evaluate(r#"(def pending (dict 'status 'pending 'job "19")) (with-handler (* e) (get e 'message) (inference-poll (fn (r) "pending 20") pending))"#).unwrap().values.last().unwrap().to_string(), "\"inference job changed\"");
    let result = world
        .evaluate(
            r#"
(def result (inference-serviced (dict 'writer "writer")
  (fn (r) (if (= r "AGEL-INFER/2 poll 19") "error 646f776e" "pending 19"))
  'writer "hi" (fn (n) (+ n 1)) 7 8))
(list (get result 'state) (get result 'polls) (get (get result 'error) 'message))
"#,
        )
        .unwrap();
    assert_eq!(result.values.last().unwrap().to_string(), "(8 1 \"down\")");
    let result = world.evaluate(r#"(inference-serviced (dict 'writer "writer") (fn (r) "pending 19") 'writer "hi" (fn (n) (signal 'test/service "must not run")) 7 0)"#).unwrap();
    assert!(result
        .values
        .last()
        .unwrap()
        .to_string()
        .contains("status pending"));
    assert_eq!(
        world
            .evaluate(r#"(inference-call (dict 'writer "writer") (fn (r) "ok") 'writer "")"#)
            .unwrap()
            .values
            .last(),
        Some(&agel_core::Value::String(String::new()))
    );
    let long = world.evaluate_with(r#"(inference-serviced (dict 'writer "writer") (fn (r) "pending 19") 'writer "hi" (fn (n) (+ n 1)) 0 600)"#, &EvaluationOptions { budget: agel_core::Budget { fuel: 5_000_000, ..Default::default() }, ..Default::default() }).unwrap();
    assert!(long
        .values
        .last()
        .unwrap()
        .to_string()
        .contains("state 600"));
    for reply in [
        "pending ",
        "pending 0",
        "pending 019",
        "pending 1x",
        "pending 19\n20",
    ] {
        let source = format!(
            "(inference-start (dict 'writer \"writer\") (fn (r) {}) 'writer \"hi\")",
            agel_core::Value::String(reply.into())
        );
        assert!(world.evaluate(&source).is_err(), "{reply:?}");
    }
}

#[test]
fn external_dispatch_drives_a_retained_repair_actor_without_runtime_model_effects() {
    let (mut world, options) = setup();
    world.evaluate_with("(def external (make-repair-agent \"external-repair\" (assoc (assoc (repair-state worker) 'model-dispatch 'external) 'defer-activation #t) nil)) (send external '(repair/start)) (run)", &options).unwrap();
    assert_eq!(
        world.pending_model_requests().len(),
        1,
        "only the original hosted control may dispatch implicitly"
    );
    let call = format!("(def external-events nil) (def next (repair-agent-exchange external (fn (provider prompt) {}) (fn (record) (def external-events (append external-events (list (get record 'event))))))) (list (get next 'status) external-events)", agel_core::Value::String(GOOD.into()));
    assert_eq!(
        world
            .evaluate_with(&call, &options)
            .unwrap()
            .values
            .last()
            .unwrap()
            .to_string(),
        "(judging (inference-start inference-result))"
    );
    world.evaluate_with(r#"(repair-agent-exchange external (fn (provider prompt) "patch noul 100 reason choice 5 boundary 900 25 25 900 25 25") (fn (record) #t))"#, &options).unwrap();
    assert_eq!(
        world
            .evaluate("(get (repair-state external) 'status)")
            .unwrap()
            .values
            .last()
            .unwrap()
            .to_string(),
        "proposing"
    );
    assert!(world
        .evaluate("(repair-prompt (repair-state external))")
        .unwrap()
        .values
        .last()
        .unwrap()
        .to_string()
        .contains("boundary"));
    let retry = format!(
        "(repair-agent-exchange external (fn (provider prompt) {}) (fn (record) #t))",
        agel_core::Value::String(GOOD.into())
    );
    world.evaluate_with(&retry, &options).unwrap();
    world.evaluate_with("(repair-agent-exchange external (fn (provider prompt) \"patch noul 950 reason choice 5 none 900 900 25 25 25 25\") (fn (record) #t))", &options).unwrap();
    assert_eq!(world.evaluate("(list (get (repair-state external) 'status) (evolution-current (get (repair-state external) 'versions) 'cell 'batches))").unwrap().values.last().unwrap().to_string(), "(approved 1)");
    world.evaluate_with("(repair-agent-exchange external (fn (provider prompt) (signal 'test/infer \"must not dispatch during activation\")) (fn (record) #t))", &options).unwrap();
    assert_eq!(world.evaluate("(list (get (repair-state external) 'status) (get (repair-evaluate (repair-state external) 2) 'correct))").unwrap().values.last().unwrap().to_string(), "(done 6)");
    assert_eq!(world.pending_model_requests().len(), 1);
}

#[test]
fn native_session_example_retains_its_repair_actor_and_controller() {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: agel_core::Budget {
            fuel: 5_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    agel_stdlib::install(&mut world, &options).unwrap();
    for source in [
        include_str!("../bytes.agel"),
        include_str!("../inference.agel"),
        include_str!("../repair.agel"),
    ] {
        world.evaluate_with(source, &options).unwrap();
    }
    world.evaluate_with("(def file-append (fn (name text) #t)) (def console-log (fn (text) #t)) (def stream (dict 'owner (spawn \"test-stream\")))", &options).unwrap();
    world
        .evaluate_with(
            "(def print-form (fn (value) \"test-serialized-record\"))",
            &options,
        )
        .unwrap();
    world
        .evaluate_with(
            include_str!("../../../examples/native-session-support.agel"),
            &options,
        )
        .unwrap();
    world
        .evaluate_with(
            include_str!("../../../examples/native-session-repair.agel"),
            &options,
        )
        .unwrap();
    let fixture = format!("(def infer (fn (provider prompt) (if (= provider 'writer) {} \"patch noul 950 reason choice 5 none 900 900 25 25 25 25\")))", agel_core::Value::String(GOOD.into()));
    world.evaluate_with(&fixture, &options).unwrap();
    world
        .evaluate_with(
            include_str!("../../../examples/native-session-repair-run.agel"),
            &options,
        )
        .unwrap();
    assert!(world.pending_model_requests().is_empty());
    assert_eq!(world.evaluate_with("(list (retained-plan 5 3) (= worker (get controller 'repair-owner)) (get (agent-info worker) 'status))", &options).unwrap().values.last().unwrap().to_string(), "(2 #t running)");
}

#[test]
fn batches_report_unexecuted_work_and_use_the_repaired_plan() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../batches.agel")).unwrap();
    let result = world.evaluate("(import agel/batches) (def actions '(1 2 3 4 5)) (def old (run-batches (fn (n size) (/ n size)) + 0 actions 3)) (def new (run-batches (fn (n size) (if (= n 0) 0 (+ (/ (- n 1) size) 1))) + 0 actions 3)) (list (get old 'state) (get old 'remaining) (get new 'state) (get new 'remaining) (get new 'completed))").unwrap();
    assert_eq!(
        result.values.last().unwrap().to_string(),
        "(6 (4 5) 15 nil 2)"
    );
    assert!(world
        .evaluate("(run-batches (fn (n size) -1) + 0 actions 3)")
        .is_err());
    assert!(world
        .evaluate("(run-batches (fn (n size) 2) + 0 actions 0)")
        .is_err());
    assert_eq!(
        world
            .evaluate("(get (run-batches (fn (n size) 0) + 0 (keys (dict)) 3) 'processed)")
            .unwrap()
            .values
            .last()
            .unwrap()
            .to_string(),
        "0"
    );
}

#[test]
fn external_probe_gates_activation_and_retries_with_the_observed_reason() {
    let (mut world, options) = setup();
    let source = format!(
        r#"
      (def canary-ok #f)
      (def probe-calls 0)
      (def candidate (fn (provider prompt) {}))
      (def judge (fn (provider prompt) "patch noul 950 reason choice 5 none 900 900 25 25 25 25"))
      (def audit (fn (record) #t))
      (def subject (make-repair-agent "measured-repair"
        (assoc (assoc (assoc (repair-state worker) 'model-dispatch 'external) 'defer-activation #t)
          'probe (fn (state) (begin (def probe-calls (+ probe-calls 1))
            (dict 'passed canary-ok 'reason (if canary-ok "measured replay passed" "observed work ceiling exceeded"))))) nil))
      (send subject '(repair/start)) (run)
      (repair-agent-exchange subject candidate audit)
      (repair-agent-exchange subject judge audit)
      (send subject '(repair/advance)) (run)
      (send subject (list 'repair/probe-result -1 (dict 'passed #t 'reason "stale receipt"))) (run)
      (list (get (repair-state subject) 'status) probe-calls
        (evolution-current (get (repair-state subject) 'versions) 'cell 'batches))"#,
        agel_core::Value::String(GOOD.into())
    );
    assert_eq!(
        world
            .evaluate_with(&source, &options)
            .unwrap()
            .values
            .last()
            .unwrap()
            .to_string(),
        "(approved 0 1)"
    );
    world
        .evaluate_with("(repair-agent-exchange subject candidate audit)", &options)
        .unwrap();
    assert!(world
        .evaluate("(repair-prompt (repair-state subject))")
        .unwrap()
        .values
        .last()
        .unwrap()
        .to_string()
        .contains("observed work ceiling exceeded"));
    let result = world
        .evaluate_with(
            r#"
      (def canary-ok #t)
      (repair-agent-exchange subject candidate audit)
      (repair-agent-exchange subject judge audit)
      (repair-agent-exchange subject candidate audit)
      (list (get (repair-state subject) 'status) probe-calls
        (evolution-current (get (repair-state subject) 'versions) 'cell 'batches))"#,
            &options,
        )
        .unwrap();
    assert_eq!(result.values.last().unwrap().to_string(), "(done 2 2)");
}

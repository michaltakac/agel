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

use agel_core::{
    EvaluationOptions, EventKind, ModelCompletion, ModelCompletionError, ModelDispatchError,
    ReplayInput, Value, World,
};

fn setup_model_agent(world: &mut World) -> EvaluationOptions {
    let capability = world.issue_capability("model/infer", "claude").unwrap();
    let options = EvaluationOptions {
        capabilities: vec![capability],
        ..EvaluationOptions::default()
    };
    world
        .evaluate_with(
            "(def model-cap (request-capability 'model/infer \"claude\"))
             (defprotocol asker-protocol (ask string) (crash))
             (def asker-behavior
               (fn (self heap message)
                 (if (= (car message) 'ask)
                     (begin
                       (model-request 'claude (car (cdr message)) self)
                       (assoc heap 'state 'waiting))
                     (if (= (car message) 'system/model-result)
                         (assoc
                           (assoc heap 'state 'done)
                           'answer
                           (car (cdr (cdr (cdr message)))))
                         (if (= (car message) 'crash)
                             (/ 1 0)
                             (assoc heap 'state 'failed))))))
             (def asker
               (spawn \"asker\" asker-behavior (dict 'state 'idle)
                 asker-protocol nil 'stop 0 (list model-cap)))",
            &options,
        )
        .unwrap();
    options
}

#[test]
fn model_effect_is_committed_then_completed_as_a_trusted_message() {
    let mut world = World::default();
    let options = setup_model_agent(&mut world);
    world
        .evaluate("(send asker '(ask \"What is homoiconicity?\")) (run)")
        .unwrap();
    let pending = world.pending_model_requests();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].provider, "claude");
    assert_eq!(pending[0].prompt, "What is homoiconicity?");

    world.claim_model_request(pending[0].id, &options).unwrap();
    world
        .complete_model_request(
            ModelCompletion::success(&pending[0], "Code and data share a representation."),
            &options,
        )
        .unwrap();
    assert!(world.pending_model_requests().is_empty());
    let value = world
        .evaluate("(run) (get (get (agent-info asker) 'heap) 'answer)")
        .unwrap()
        .values
        .pop()
        .unwrap();
    assert_eq!(
        value,
        Value::String("Code and data share a representation.".into())
    );
    assert!(world
        .events()
        .iter()
        .any(|event| event.kind == EventKind::ModelCompleted));
}

#[test]
fn missing_capability_prevents_an_agent_from_creating_external_work() {
    let mut world = World::default();
    world
        .evaluate(
            "(def behavior
               (fn (self heap message)
                 (model-request 'claude \"unauthorized\" self)))
             (def denied (spawn \"denied\" behavior nil nil nil 'stop 0))
             (send denied '(go))
             (run)",
        )
        .unwrap();
    assert!(world.pending_model_requests().is_empty());
    assert_eq!(
        world
            .evaluate("(get (agent-info denied) 'status)")
            .unwrap()
            .values[0],
        Value::Symbol("stopped".into())
    );
}

#[test]
fn failed_agent_turn_rolls_back_its_model_request() {
    let mut world = World::default();
    let capability = world.issue_capability("model/infer", "claude").unwrap();
    let options = EvaluationOptions {
        capabilities: vec![capability],
        ..EvaluationOptions::default()
    };
    world
        .evaluate_with(
            "(def cap (request-capability 'model/infer \"claude\"))
             (def behavior
               (fn (self heap message)
                 (begin (model-request 'claude \"must disappear\" self) (/ 1 0))))
             (def worker
               (spawn \"worker\" behavior nil nil nil 'stop 0 (list cap)))
             (send worker '(go))
             (run)",
            &options,
        )
        .unwrap();
    assert!(world.pending_model_requests().is_empty());
    assert!(!world
        .events()
        .iter()
        .any(|event| event.kind == EventKind::ModelRequested));
}

#[test]
fn recorded_model_completion_replays_without_calling_a_provider() {
    let mut world = World::default();
    let options = setup_model_agent(&mut world);
    let snapshot = world.snapshot();
    world
        .evaluate("(send asker '(ask \"future?\")) (run)")
        .unwrap();
    let request = world.pending_model_requests().pop().unwrap();
    let inputs = vec![
        ReplayInput::Evaluate("(send asker '(ask \"future?\")) (run)".into()),
        ReplayInput::ClaimModel(1),
        ReplayInput::CompleteModel(ModelCompletion::success(
            &request,
            "agents all the way down",
        )),
        ReplayInput::Evaluate("(run)".into()),
    ];
    let left = World::replay_inputs(&snapshot, &inputs, &options).unwrap();
    let right = World::replay_inputs(&snapshot, &inputs, &options).unwrap();
    assert_eq!(left.final_digest, right.final_digest);
    assert_eq!(left.events, right.events);
}

#[test]
fn dispatch_claim_prevents_duplicate_external_work() {
    let mut world = World::default();
    let options = setup_model_agent(&mut world);
    world
        .evaluate("(send asker '(ask \"exactly once\")) (run)")
        .unwrap();
    let (_, claimed) = world.claim_model_request(1, &options).unwrap();
    assert_eq!(claimed.prompt, "exactly once");
    assert!(world.pending_model_requests().is_empty());
    assert_eq!(world.dispatching_model_requests(), vec![claimed.clone()]);
    assert_eq!(
        world.claim_model_request(1, &options).unwrap_err(),
        ModelDispatchError::NotPending(1)
    );
    world
        .complete_model_request(ModelCompletion::success(&claimed, "once"), &options)
        .unwrap();
    assert!(world.dispatching_model_requests().is_empty());
}

#[test]
fn completed_output_is_not_reissued_when_its_target_has_stopped() {
    let mut world = World::default();
    let options = setup_model_agent(&mut world);
    world
        .evaluate("(send asker '(ask \"survive me\")) (run)")
        .unwrap();
    let (_, request) = world.claim_model_request(1, &options).unwrap();
    world.evaluate("(send asker '(crash)) (run)").unwrap();
    world
        .complete_model_request(
            ModelCompletion::success(&request, "durably recorded"),
            &options,
        )
        .unwrap();
    assert!(world.pending_model_requests().is_empty());
    assert!(world.dispatching_model_requests().is_empty());
    assert!(world
        .events()
        .iter()
        .any(|event| event.kind == EventKind::ModelDeliveryDropped));
}

#[test]
fn completion_is_idempotence_guarded() {
    let mut world = World::default();
    let options = setup_model_agent(&mut world);
    world
        .evaluate("(send asker '(ask \"once\")) (run)")
        .unwrap();
    let request = world.pending_model_requests().pop().unwrap();
    world.claim_model_request(1, &options).unwrap();
    let completion = ModelCompletion::success(&request, "one answer");
    world
        .complete_model_request(completion.clone(), &options)
        .unwrap();
    assert_eq!(
        world
            .complete_model_request(completion, &options)
            .unwrap_err(),
        ModelCompletionError::AlreadyCompleted(1)
    );
}

#[test]
fn restoring_pending_snapshot_cannot_reissue_a_claimed_effect() {
    let mut world = World::default();
    let options = setup_model_agent(&mut world);
    world
        .evaluate("(send asker '(ask \"charge once\")) (run)")
        .unwrap();
    let pending_snapshot = world.snapshot();
    let (_, request) = world.claim_model_request(1, &options).unwrap();
    world
        .complete_model_request(ModelCompletion::success(&request, "paid result"), &options)
        .unwrap();
    world.restore_snapshot(&pending_snapshot).unwrap();
    assert_eq!(world.pending_model_requests().len(), 1);
    assert!(matches!(
        world.claim_model_request(1, &options),
        Err(ModelDispatchError::AlreadyClaimed(_))
    ));
    assert_eq!(world.effect_journal().entries().len(), 1);
}

#[test]
fn snapshot_restore_revokes_pre_restore_capabilities() {
    let mut world = World::default();
    let capability = world.issue_capability("model/infer", "claude").unwrap();
    let options = EvaluationOptions {
        capabilities: vec![capability],
        ..EvaluationOptions::default()
    };
    world
        .evaluate_with("(request-capability 'model/infer \"claude\")", &options)
        .unwrap();
    let snapshot = world.snapshot();
    world.restore_snapshot(&snapshot).unwrap();
    let error = world
        .evaluate_with("(request-capability 'model/infer \"claude\")", &options)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "evaluation error: capability/denied: no supplied capability permits model/infer on claude"
    );
}

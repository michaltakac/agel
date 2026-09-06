//! Agel's standard library, written in Agel and installed atomically.

use agel_core::{Commit, EvaluationOptions, TransactionError, World};

pub const SOURCE: &str = include_str!("../stdlib.agel");

pub fn install(world: &mut World, options: &EvaluationOptions) -> Result<Commit, TransactionError> {
    world.evaluate_with(SOURCE, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agel_core::{ModelCompletion, Value};

    fn installed() -> World {
        let mut world = World::default();
        install(&mut world, &EvaluationOptions::default()).unwrap();
        world
    }

    #[test]
    fn sequence_and_result_libraries_are_agel_code() {
        let mut world = installed();
        let values = world
            .evaluate(
                "(import agel/sequence)
                 (import agel/result)
                 (list
                   (map (fn (x) (* x x)) '(1 2 3 4))
                   (filter (fn (x) (= x 2)) '(1 2 3))
                   (foldl + 0 '(1 2 3 4))
                   (unwrap-or (err 'nope) 99))",
            )
            .unwrap()
            .values;
        assert_eq!(
            values.last(),
            Some(&Value::List(vec![
                Value::List(vec![
                    Value::Int(1),
                    Value::Int(4),
                    Value::Int(9),
                    Value::Int(16)
                ]),
                Value::List(vec![Value::Int(2)]),
                Value::Int(10),
                Value::Int(99),
            ]))
        );
    }

    #[test]
    fn worker_pool_distributes_tasks_round_robin() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/swarm)
                 (def observer (spawn \"observer\"))
                 (def left (make-worker \"left\" (fn (x) (+ x 100))))
                 (def right (make-worker \"right\" (fn (x) (+ x 200))))
                 (def pool (make-pool \"pool\" (list left right)))
                 (submit pool observer 1)
                 (submit pool observer 2)
                 (submit pool observer 3)
                 (run 10)
                 (list (recv observer) (recv observer) (recv observer)
                       (get (agent-info pool) 'heap))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![
                    Value::Symbol("result".into()),
                    Value::Agent(2),
                    Value::Int(101),
                ]),
                Value::List(vec![
                    Value::Symbol("result".into()),
                    Value::Agent(3),
                    Value::Int(202),
                ]),
                Value::List(vec![
                    Value::Symbol("result".into()),
                    Value::Agent(2),
                    Value::Int(103),
                ]),
                Value::List(vec![Value::Agent(3), Value::Agent(2)]),
            ])
        );
    }

    #[test]
    fn empty_pool_fails_before_spawning() {
        let mut world = installed();
        let error = world
            .evaluate("(import agel/swarm) (make-pool \"empty\" nil)")
            .unwrap_err();
        assert!(error.to_string().contains("swarm/no-workers"));
    }

    #[test]
    fn eager_and_bounded_fixed_points_are_language_code() {
        let mut world = installed();
        let value = world
            .evaluate(
                "(import agel/fixed-point)
                 (def factorial
                   (fix
                     (fn (recur)
                       (fn (n)
                         (if (= n 0) 1 (* n (recur (- n 1))))))))
                 (def bounded-factorial
                   (fix-bounded 6
                     (fn (recur)
                       (fn (n)
                         (if (= n 0) 1 (* n (recur (- n 1))))))))
                 (def converged
                   (converge-bounded 8
                     (fn (value) (if (< value 10) (+ value 3) value))
                     = 0))
                 (list (factorial 6) (bounded-factorial 6) converged)",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            value,
            Value::List(vec![Value::Int(720), Value::Int(720), Value::Int(12)])
        );

        let error = world
            .evaluate(
                "(def too-small
                   (fix-bounded 2
                     (fn (recur)
                       (fn (n)
                         (if (= n 0) 1 (* n (recur (- n 1))))))))
                 (too-small 6)",
            )
            .unwrap_err();
        assert!(error.to_string().contains("fixed-point/exhausted"));
    }

    #[test]
    fn agentic_fixed_point_is_turn_bounded_traced_and_exhaustible() {
        let mut world = installed();
        let value = world
            .evaluate(
                "(import agel/fixed-point)
                 (def observer (spawn \"observer\"))
                 (def countdown-step
                   (fn (state event)
                     (if (= state 0)
                         (fixed-done 'launched (list 'at state))
                         (fixed-continue (- state 1) (list 'at state)))))
                 (def driver
                   (make-fixed-agent \"countdown\" countdown-step
                     (fixed-policy 8 8 0 0) nil))
                 (fixed-start driver observer 3)
                 (run 10)
                 (def outcome (recv observer))
                 (def heap (get (agent-info driver) 'heap))
                 (list
                   (car outcome)
                   (car (cdr (cdr outcome)))
                   (get heap 'status)
                   (get heap 'steps)
                   (get heap 'model-calls)
                   (count (get heap 'trace))
                   (get heap 'trace-dropped))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            value,
            Value::List(vec![
                Value::Symbol("fixed/done".into()),
                Value::Symbol("launched".into()),
                Value::Symbol("done".into()),
                Value::Int(4),
                Value::Int(0),
                Value::Int(4),
                Value::Int(0),
            ])
        );

        let exhausted = world
            .evaluate(
                "(def bounded
                   (make-fixed-agent \"bounded\" countdown-step
                     (fixed-policy 2 1 0 0) nil))
                 (fixed-start bounded observer 4)
                 (run 10)
                 (def stopped (recv observer))
                 (def stopped-heap (get (agent-info bounded) 'heap))
                 (list
                   (car stopped)
                   (car (cdr (cdr stopped)))
                   (car (cdr (cdr (cdr stopped))))
                   (get stopped-heap 'status)
                   (get stopped-heap 'steps)
                   (count (get stopped-heap 'trace))
                   (get stopped-heap 'trace-dropped))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            exhausted,
            Value::List(vec![
                Value::Symbol("fixed/exhausted".into()),
                Value::Symbol("steps".into()),
                Value::Int(2),
                Value::Symbol("exhausted".into()),
                Value::Int(2),
                Value::Int(1),
                Value::Int(1),
            ])
        );
    }

    #[test]
    fn agentic_fixed_point_model_use_is_explicit_and_counted() {
        let mut world = World::default();
        let host_capability = world.issue_capability("model/infer", "claude").unwrap();
        let options = EvaluationOptions {
            capabilities: vec![host_capability],
            ..EvaluationOptions::default()
        };
        install(&mut world, &options).unwrap();
        world
            .evaluate_with(
                "(import agel/fixed-point)
                 (def model-cap (request-capability 'model/infer \"claude\"))
                 (def observer (spawn \"observer\"))
                 (def synthesis-step
                   (fn (state event)
                     (if (= (car event) 'fixed/start)
                         (fixed-model 'waiting 'claude
                           \"Give one bounded synthesis\" 'requesting)
                         (if (= (car event) 'system/model-result)
                             (fixed-done
                               (car (cdr (cdr (cdr event)))) 'answered)
                             (fixed-done 'provider-error 'failed)))))
                 (def thinker
                   (make-fixed-agent \"thinker\" synthesis-step
                     (fixed-policy 4 4 1 100) (list model-cap)))
                 (fixed-start thinker observer nil)
                 (run)",
                &options,
            )
            .unwrap();
        let request = world.pending_model_requests().pop().unwrap();
        assert_eq!(request.prompt, "Give one bounded synthesis");
        assert_eq!(
            world
                .evaluate("(get (get (agent-info thinker) 'heap) 'status)")
                .unwrap()
                .values[0],
            Value::Symbol("waiting".into())
        );

        world.claim_model_request(request.id, &options).unwrap();
        world
            .complete_model_request(
                ModelCompletion::success(&request, "A replayable answer."),
                &options,
            )
            .unwrap();
        let value = world
            .evaluate(
                "(run)
                 (def outcome (recv observer))
                 (def heap (get (agent-info thinker) 'heap))
                 (list
                   (car outcome)
                   (car (cdr (cdr outcome)))
                   (get heap 'model-calls)
                   (get heap 'status)
                   (get (car (get heap 'trace)) 'event))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            value,
            Value::List(vec![
                Value::Symbol("fixed/done".into()),
                Value::String("A replayable answer.".into()),
                Value::Int(1),
                Value::Symbol("done".into()),
                Value::List(vec![
                    Value::Symbol("system/model-result".into()),
                    Value::Int(1),
                    Value::Symbol("claude".into()),
                ]),
            ])
        );

        let limited = world
            .evaluate(
                "(def blocked
                   (make-fixed-agent \"blocked\" synthesis-step
                     (fixed-policy 3 3 0 100) (list model-cap)))
                 (fixed-start blocked observer nil)
                 (run)
                 (def blocked-outcome (recv observer))
                 (list
                   (car blocked-outcome)
                   (car (cdr (cdr blocked-outcome))))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert!(world.pending_model_requests().is_empty());
        assert_eq!(
            limited,
            Value::List(vec![
                Value::Symbol("fixed/exhausted".into()),
                Value::Symbol("model-calls".into()),
            ])
        );
    }

    #[test]
    fn agentic_fixed_point_evolves_at_a_message_ordered_boundary() {
        let mut world = installed();
        let value = world
            .evaluate(
                "(import agel/fixed-point)
                 (def observer (spawn \"observer\"))
                 (def slow
                   (fn (state event)
                     (if (= state 0)
                         (fixed-done 'slow 'done)
                         (fixed-continue (- state 1) (list 'slow state)))))
                 (def fast
                   (fn (state event)
                     (if (< state 2)
                         (fixed-done (list 'fast state) 'done)
                         (fixed-continue (- state 2) (list 'fast state)))))
                 (def driver
                   (make-fixed-agent \"evolving\" slow
                     (fixed-policy 10 10 0 0) nil))
                 (fixed-start driver observer 5)
                 (run 1)
                 (fixed-propose driver 0 fast observer)
                 (run 2)
                 (def preview (recv observer))
                 (fixed-commit driver 0 observer)
                 (run 2)
                 (def evolved (recv observer))
                 (run 10)
                 (def outcome (recv observer))
                 (def heap (get (agent-info driver) 'heap))
                 (list
                   (car evolved)
                   (car (cdr (cdr evolved)))
                   (car preview)
                   (car (cdr (cdr (cdr preview))))
                   (car outcome)
                   (car (cdr (cdr outcome)))
                   (get heap 'version)
                   (get heap 'steps))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            value,
            Value::List(vec![
                Value::Symbol("fixed/evolved".into()),
                Value::Int(1),
                Value::Symbol("fixed/preview".into()),
                Value::Int(1),
                Value::Symbol("fixed/done".into()),
                Value::List(vec![Value::Symbol("fast".into()), Value::Int(0)]),
                Value::Int(1),
                Value::Int(5),
            ])
        );

        let rejected_and_discarded = world
            .evaluate(
                "(fixed-propose driver 0 slow observer)
                 (run 1)
                 (def stale (recv observer))
                 (fixed-propose driver 1 slow observer)
                 (run 1)
                 (def second-preview (recv observer))
                 (fixed-discard driver observer)
                 (run 1)
                 (def discarded (recv observer))
                 (def final-heap (get (agent-info driver) 'heap))
                 (list
                   (car stale)
                   (car (cdr (cdr stale)))
                   (car second-preview)
                   (car discarded)
                   (get final-heap 'version)
                   (get final-heap 'pending-step))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            rejected_and_discarded,
            Value::List(vec![
                Value::Symbol("fixed/rejected".into()),
                Value::Symbol("stale-version".into()),
                Value::Symbol("fixed/preview".into()),
                Value::Symbol("fixed/discarded".into()),
                Value::Int(1),
                Value::Nil,
            ])
        );
    }

    #[test]
    fn agel_interprets_agel_code_as_data() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/meta)
                 (def meta-env (meta-base-env))
                 (list
                   (meta-eval '(if (= 1 2) 0 (* 6 7)) meta-env)
                   (meta-eval '((fn (x) (+ x 1)) 41) meta-env)
                   (meta-eval
                     '((fn (x) ((fn (y) (+ x y)) 2)) 40)
                     meta-env))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(42), Value::Int(42), Value::Int(42)])
        );
    }

    #[test]
    fn metacircular_evaluator_rejects_malformed_and_unbound_code() {
        let mut world = installed();
        world
            .evaluate("(import agel/meta) (def meta-env (meta-base-env))")
            .unwrap();
        for source in [
            "(meta-eval 'missing meta-env)",
            "(meta-eval '(if #t 42) meta-env)",
            "(meta-eval '((fn (x) x)) meta-env)",
            "(meta-eval '((fn (x) x) 1 2) meta-env)",
        ] {
            assert!(world.evaluate(source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn ui_scenes_and_actions_are_homoiconic_agel_data() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/ui)
                 (def save-intent (intent 'workspace/save 'editor nil 'filesystem/write))
                 (def scene
                   (window 'main \"Agel Workshop\"
                     (list
                       (column 'body
                         (list
                           (text 'greeting \"Build the system from inside it.\")
                           (button 'save \"Save world\" save-intent))))))
                 (list
                   (scene? scene)
                   (get (find-node scene 'save) 'kind)
                   (get (get (find-node scene 'save) 'props) 'action)
                   (get ui-spec 'representation))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Bool(true),
                Value::Symbol("button".into()),
                Value::Map(vec![
                    (Value::Symbol("kind".into()), Value::Symbol("intent".into())),
                    (
                        Value::Symbol("action".into()),
                        Value::Symbol("workspace/save".into()),
                    ),
                    (
                        Value::Symbol("target".into()),
                        Value::Symbol("editor".into()),
                    ),
                    (Value::Symbol("payload".into()), Value::Nil),
                    (
                        Value::Symbol("requires".into()),
                        Value::Symbol("filesystem/write".into()),
                    ),
                ]),
                Value::Symbol("persistent-data".into()),
            ])
        );
    }

    #[test]
    fn ui_patches_are_persistent_and_validated() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/ui)
                 (def original
                   (window 'main \"Before\"
                     (list (text 'status \"safe\"))))
                 (def changed
                   (apply-patches original
                     (list
                       (set-prop 'main 'title \"After\")
                       (set-prop 'status 'content \"live\"))))
                 (def duplicate
                   (apply-patch original
                     (set-children 'main
                       (list (text 'same \"one\") (text 'same \"two\")))))
                 (list
                   (get (get original 'props) 'title)
                   (get (get changed 'props) 'title)
                   (get (get (find-node changed 'status) 'props) 'content)
                   (scene? changed)
                   (scene? duplicate))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::String("Before".into()),
                Value::String("After".into()),
                Value::String("live".into()),
                Value::Bool(true),
                Value::Bool(false),
            ])
        );

        let error = world
            .evaluate("(apply-patch original (set-prop 'missing 'title \"No\"))")
            .unwrap_err();
        assert!(error.to_string().contains("ui/unknown-node"));
    }

    #[test]
    fn desktop_agent_previews_commits_and_rolls_back_live_scenes() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/ui)
                 (def human (spawn \"human\"))
                 (def initial (window 'main \"Original\" (list (text 'status \"ready\"))))
                 (def desktop (make-desktop \"desktop\" initial))
                 (propose desktop human 0
                   (list (set-prop 'main 'title \"Agentic\")
                         (set-prop 'status 'content \"previewing\")))
                 (run 1)
                 (def preview-message (recv human))
                 (commit desktop human)
                 (run 1)
                 (def commit-message (recv human))
                 (propose desktop human 0 (list (set-prop 'main 'title \"Stale\")))
                 (run 1)
                 (def stale-message (recv human))
                 (rollback desktop human)
                 (run 1)
                 (def rollback-message (recv human))
                 (list
                   (car preview-message)
                   (car commit-message)
                   (get (car (cdr (cdr stale-message))) 'kind)
                   (car rollback-message)
                   (get (agent-info desktop) 'status)
                   (get (get (agent-info desktop) 'heap) 'revision)
                   (get (get (agent-info desktop) 'heap) 'pending)
                   (get
                     (get (get (get (agent-info desktop) 'heap) 'scene) 'props)
                     'title))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Symbol("preview".into()),
                Value::Symbol("committed".into()),
                Value::Symbol("ui/stale-revision".into()),
                Value::Symbol("rolled-back".into()),
                Value::Symbol("running".into()),
                Value::Int(2),
                Value::Nil,
                Value::String("Original".into()),
            ])
        );
    }

    #[test]
    fn desktop_rejects_invalid_agent_edits_without_mutating_state() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/ui)
                 (def human (spawn \"human\"))
                 (def initial (window 'main \"Safe\" (list (text 'status \"ready\"))))
                 (def desktop (make-desktop \"desktop\" initial))
                 (propose desktop human 0
                   (list
                     (set-children 'main
                       (list (text 'duplicate \"one\")
                             (button 'duplicate \"two\"
                               (intent 'noop nil nil nil))))))
                 (run 1)
                 (def answer (recv human))
                 (list
                   (car answer)
                   (get (agent-info desktop) 'status)
                   (get (get (agent-info desktop) 'heap) 'revision)
                   (get (get (agent-info desktop) 'heap) 'preview)
                   (= (get (get (agent-info desktop) 'heap) 'scene) initial))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Symbol("rejected".into()),
                Value::Symbol("running".into()),
                Value::Int(0),
                Value::Nil,
                Value::Bool(true),
            ])
        );
    }

    #[test]
    fn default_desktop_compiles_to_a_deterministic_display_frame() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/desktop)
                 (import agel/ui)
                 (import agel/ui-layout)
                 (def scene (default-scene))
                 (def frame (compile-frame scene default-viewport default-theme))
                 (def same-frame (compile-frame scene default-viewport default-theme))
                 (def panel-hit (hit-test frame 20 50))
                 (def dock-hit (hit-test frame 20 720))
                 (list
                   (scene? scene)
                   (frame? frame)
                   (= frame same-frame)
                   (count (get frame 'boxes))
                   (count (display-list frame))
                   (count (get frame 'actions))
                   (get panel-hit 'id)
                   (get (get panel-hit 'intent) 'action)
                   (get dock-hit 'id)
                   (get (get dock-hit 'intent) 'target)
                   (hit-test frame 640 400))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Bool(true),
                Value::Bool(true),
                Value::Bool(true),
                Value::Int(16),
                Value::Int(24),
                Value::Int(8),
                Value::Symbol("launcher".into()),
                Value::Symbol("launcher/toggle".into()),
                Value::Symbol("terminal".into()),
                Value::Symbol("terminal".into()),
                Value::Nil,
            ])
        );

        let error = world
            .evaluate("(compile-frame (text 'bad 42) default-viewport default-theme)")
            .unwrap_err();
        assert!(error.to_string().contains("ui/invalid-display-list"));
        let error = world
            .evaluate("(compile-frame (window 'tiny \"Tiny\" nil) (rect 0 0 20 20) default-theme)")
            .unwrap_err();
        assert!(error.to_string().contains("ui/layout-overflow"));
        assert_eq!(
            world
                .evaluate("(frame? (dict 'kind 'display-frame 'viewport default-viewport))")
                .unwrap()
                .values
                .pop(),
            Some(Value::Bool(false))
        );
    }

    #[test]
    fn layout_agent_commits_frames_and_rejects_impossible_geometry() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/desktop)
                 (import agel/ui-layout)
                 (def observer (spawn \"display-server\"))
                 (def layout-engine (make-layout-agent \"layout-engine\"))
                 (def scene (default-scene))
                 (render-frame layout-engine observer scene default-viewport default-theme)
                 (run 1)
                 (def rendered (recv observer))
                 (hit-point layout-engine observer 20 720)
                 (run 1)
                 (def activated (recv observer))
                 (render-frame layout-engine observer scene (rect 0 0 20 20) default-theme)
                 (run 1)
                 (def rejected (recv observer))
                 (list
                   (car rendered)
                   (car activated)
                   (get (car (cdr activated)) 'id)
                   (car rejected)
                   (get (car (cdr rejected)) 'data)
                   (frame? (get (get (agent-info layout-engine) 'heap) 'frame))
                   (get (agent-info layout-engine) 'status))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Symbol("rendered".into()),
                Value::Symbol("action".into()),
                Value::Symbol("terminal".into()),
                Value::Symbol("render-rejected".into()),
                Value::Symbol("desktop".into()),
                Value::Bool(true),
                Value::Symbol("running".into()),
            ])
        );
    }

    #[test]
    fn vector_graphics_are_validated_agel_data_and_density_independent() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/desktop)
                 (import agel/ui-layout)
                 (import agel/vector)
                 (import agel/ui-vector)
                 (def layout
                   (compile-frame (default-scene) default-viewport default-theme))
                 (def retina (compile-vector-frame layout 2))
                 (def same (compile-vector-frame layout 2))
                 (list
                   (vector-frame? retina)
                   (= retina same)
                   (get retina 'physical-width)
                   (get retina 'physical-height)
                   (count (vector-display-list retina))
                   (balanced-vector-commands? (vector-display-list retina))
                   (paint? (linear-gradient (point 0 0) (point 1024 1024)
                             \"#000000\" \"#ffffff\"))
                   (shape? (path (list (move-to 0 0) (line-to 4 4)
                                       (close-path)))))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Bool(true),
                Value::Bool(true),
                Value::Int(2560),
                Value::Int(1600),
                Value::Int(24),
                Value::Bool(true),
                Value::Bool(true),
                Value::Bool(true),
            ])
        );

        let error = world
            .evaluate("(compile-vector-frame layout 0)")
            .unwrap_err();
        assert!(error.to_string().contains("vector/invalid-scale"));
        assert_eq!(
            world
                .evaluate("(balanced-vector-commands? (list (restore)))")
                .unwrap()
                .values
                .pop(),
            Some(Value::Bool(false))
        );
        assert_eq!(
            world
                .evaluate("(vector-command? (dict 'op 'fill-shape))")
                .unwrap()
                .values
                .pop(),
            Some(Value::Bool(false))
        );
    }

    #[test]
    fn vector_agent_retains_the_last_good_frame() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/desktop)
                 (import agel/ui-layout)
                 (import agel/ui-vector)
                 (def observer (spawn \"compositor\"))
                 (def vectorizer (make-vector-agent \"vectorizer\"))
                 (def layout
                   (compile-frame (default-scene) default-viewport default-theme))
                 (vectorize-frame vectorizer observer layout 2)
                 (run 1)
                 (def accepted (recv observer))
                 (vectorize-frame vectorizer observer layout 0)
                 (run 1)
                 (def rejected (recv observer))
                 (list
                   (car accepted)
                   (car rejected)
                   (vector-frame? (get (get (agent-info vectorizer) 'heap) 'frame))
                   (get (agent-info vectorizer) 'status))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Symbol("vectorized".into()),
                Value::Symbol("vector-rejected".into()),
                Value::Bool(true),
                Value::Symbol("running".into()),
            ])
        );
    }
}

//! Agel's standard library, written in Agel and installed atomically.

use agel_core::{Commit, EvaluationOptions, TransactionError, World};

pub const SOURCE: &str = include_str!("../stdlib.agel");

pub fn install(world: &mut World, options: &EvaluationOptions) -> Result<Commit, TransactionError> {
    world.evaluate_with(SOURCE, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agel_core::Value;

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
}

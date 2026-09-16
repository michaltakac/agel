use agel_core::{Budget, EvaluationOptions, TransactionError, Value, World};

#[test]
fn immutable_closure_code_is_shared_across_lookups() {
    let mut world = World::default();
    let Value::Closure(first) = last(&mut world, "(def f (fn (x) (+ x 1)))") else {
        panic!("expected closure");
    };
    let Value::Closure(second) = last(&mut world, "f") else {
        panic!("expected closure");
    };
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(last(&mut world, "(f 41)"), Value::Int(42));
}

#[test]
fn small_embedding_stack_preserves_language_depth_limit_and_rollback() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut world = World::default();
            world
                .evaluate("(def loop (fn (n) (loop (+ n 1))))")
                .unwrap();
            let revision = world.revision();
            let options = EvaluationOptions {
                budget: Budget {
                    max_call_depth: 64,
                    ..Budget::default()
                },
                ..EvaluationOptions::default()
            };
            let error = world
                .evaluate_with("(def temporary 42) (loop 0)", &options)
                .unwrap_err();
            assert!(error.to_string().contains("resource/call-depth"), "{error}");
            assert_eq!(world.revision(), revision);
            assert!(world.evaluate("temporary").is_err());
            assert_eq!(last(&mut world, "(+ 20 22)"), Value::Int(42));
        })
        .unwrap()
        .join()
        .unwrap();
}

fn last(world: &mut World, source: &str) -> Value {
    world
        .evaluate(source)
        .unwrap()
        .values
        .last()
        .cloned()
        .unwrap_or(Value::Nil)
}

fn eval_condition(world: &mut World, source: &str) -> String {
    match world.evaluate(source).unwrap_err() {
        TransactionError::Eval(error) => error.condition.kind,
        other => panic!("expected evaluation condition, got {other}"),
    }
}

#[test]
fn lexical_closures_capture_their_definition_environment() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(def addx (let ((x 10)) (fn (y) (+ x y))))
         (let ((x 100)) (addx 5))",
    );
    assert_eq!(value, Value::Int(15));
}

#[test]
fn closures_support_recursion_and_multiple_body_forms() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(def factorial
           (fn (n)
             (def ignored-globally 1)
             (if (= n 0) 1 (* n (factorial (- n 1))))))
         (factorial 6)",
    );
    assert_eq!(value, Value::Int(720));
}

#[test]
fn macro_introduced_bindings_cannot_capture_call_site_identifiers() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(defmacro with-temp (value body) (let ((tmp value)) body))
         (let ((tmp 99)) (with-temp 1 tmp))",
    );
    assert_eq!(value, Value::Int(99));
}

#[test]
fn macro_free_identifiers_resolve_at_the_definition_site() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(defmacro add (a b) (+ a b))
         (let ((+ (fn (a b) (- a b)))) (add 7 2))",
    );
    assert_eq!(value, Value::Int(9));
}

#[test]
fn modules_export_values_and_macros_explicitly() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(module math
           (export square unless)
           (def square (fn (x) (* x x)))
           (defmacro unless (condition body) (if condition nil body)))
         (import math)
         (list (square 7) (math/square 8) (unless #f 11))",
    );
    assert_eq!(
        value,
        Value::List(vec![Value::Int(49), Value::Int(64), Value::Int(11)])
    );
}

#[test]
fn module_macros_can_resolve_private_definition_site_helpers() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(module arithmetic
           (export double)
           (def private-add (fn (a b) (+ a b)))
           (defmacro double (value) (private-add value value)))
         (import arithmetic)
         (double 21)",
    );
    assert_eq!(value, Value::Int(42));
    assert_eq!(eval_condition(&mut world, "private-add"), "name/unbound");
}

#[test]
fn macro_introduced_handler_bindings_are_hygienic() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(defmacro ignore-failure (body fallback)
           (with-handler (* problem) fallback body))
         (let ((problem 77))
           (ignore-failure (signal 'test/failure \"nope\") problem))",
    );
    assert_eq!(value, Value::Int(77));
}

#[test]
fn invalid_module_rolls_back_the_entire_batch() {
    let mut world = World::default();
    let kind = eval_condition(
        &mut world,
        "(def before 1) (module broken (export missing))",
    );
    assert_eq!(kind, "module/missing-export");
    assert_eq!(world.binding("before"), None);
    assert_eq!(world.revision(), 0);
}

#[test]
fn structured_conditions_are_visible_to_handlers() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(with-handler (arithmetic/division-by-zero problem)
           (list (get problem 'kind) (get problem 'message))
           (/ 1 0))",
    );
    assert_eq!(
        value,
        Value::List(vec![
            Value::Symbol("arithmetic/division-by-zero".into()),
            Value::String("division by zero".into()),
        ])
    );
}

#[test]
fn handlers_can_select_named_restarts() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(with-restart (use-value replacement)
           replacement
           (with-handler (arithmetic/division-by-zero problem)
             (invoke-restart use-value 42)
             (/ 1 0)))",
    );
    assert_eq!(value, Value::Int(42));
}

#[test]
fn persistent_maps_do_not_mutate_prior_values() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(def original (dict 'a 1))
         (def changed (assoc original 'b 2))
         (list (count original) (get original 'b) (count changed) (get changed 'b))",
    );
    assert_eq!(
        value,
        Value::List(vec![
            Value::Int(1),
            Value::Nil,
            Value::Int(2),
            Value::Int(2)
        ])
    );
}

#[test]
fn capabilities_are_unforgeable_and_scope_checked() {
    let mut world = World::default();
    assert_eq!(
        eval_condition(
            &mut world,
            "(request-capability 'filesystem/read \"/tmp/file\")"
        ),
        "capability/denied"
    );

    let capability = world.issue_capability("filesystem/read", "/tmp").unwrap();
    let options = EvaluationOptions {
        capabilities: vec![capability.clone()],
        ..EvaluationOptions::default()
    };
    let commit = world
        .evaluate_with(
            "(request-capability 'filesystem/read \"/tmp/file\")",
            &options,
        )
        .unwrap();
    assert_eq!(commit.values, vec![Value::Capability(capability)]);

    let error = world
        .evaluate_with(
            "(request-capability 'filesystem/read \"/private/file\")",
            &options,
        )
        .unwrap_err();
    assert!(error.to_string().contains("capability/denied"));
}

#[test]
fn call_depth_budget_stops_recursion_and_rolls_back() {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 10_000,
            max_call_depth: 12,
            ..Budget::default()
        },
        capabilities: Vec::new(),
        pulse: None,
        host: &[],
    };
    let error = world
        .evaluate_with("(def loop (fn () (loop))) (loop)", &options)
        .unwrap_err();
    assert!(error.to_string().contains("resource/call-depth"));
    assert_eq!(world.binding("loop"), None);
    assert_eq!(world.revision(), 0);
}

#[test]
fn fuel_and_collection_budgets_are_deterministic() {
    let mut world = World::default();
    let low_fuel = EvaluationOptions {
        budget: Budget {
            fuel: 2,
            ..Budget::default()
        },
        capabilities: Vec::new(),
        pulse: None,
        host: &[],
    };
    let error = world.evaluate_with("(+ 1 2)", &low_fuel).unwrap_err();
    assert!(error.to_string().contains("resource/fuel-exhausted"));

    let tiny_collection = EvaluationOptions {
        budget: Budget {
            max_collection_len: 2,
            ..Budget::default()
        },
        capabilities: Vec::new(),
        pulse: None,
        host: &[],
    };
    let error = world
        .evaluate_with("(list 1 2 3)", &tiny_collection)
        .unwrap_err();
    assert!(error.to_string().contains("resource/collection-limit"));
}

#[test]
fn macro_expansion_is_preflighted_against_resource_limits() {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: Budget {
            max_collection_len: 3,
            ..Budget::default()
        },
        capabilities: Vec::new(),
        pulse: None,
        host: &[],
    };
    let error = world
        .evaluate_with(
            "(defmacro duplicate-four (x) (list x x x x)) (duplicate-four 1)",
            &options,
        )
        .unwrap_err();
    assert!(error.to_string().contains("resource/macro-expansion-limit"));
    assert_eq!(world.revision(), 0);
}

#[test]
fn recursive_macro_expansion_is_depth_bounded_and_transactional() {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 100_000,
            max_parse_depth: 8,
            ..Budget::default()
        },
        capabilities: Vec::new(),
        pulse: None,
        host: &[],
    };
    let error = world
        .evaluate_with(
            "(def before 1) (defmacro forever (x) (forever x)) (forever 0)",
            &options,
        )
        .unwrap_err();
    assert!(error.to_string().contains("resource/macro-depth"));
    assert_eq!(world.binding("before"), None);
    assert_eq!(world.revision(), 0);
}

#[test]
fn reader_limits_apply_before_candidate_evaluation() {
    let mut world = World::default();
    let source_limited = EvaluationOptions {
        budget: Budget {
            max_source_bytes: 4,
            ..Budget::default()
        },
        capabilities: Vec::new(),
        pulse: None,
        host: &[],
    };
    assert!(world.evaluate_with("(def x 1)", &source_limited).is_err());
    let depth_limited = EvaluationOptions {
        budget: Budget {
            max_parse_depth: 1,
            ..Budget::default()
        },
        capabilities: Vec::new(),
        pulse: None,
        host: &[],
    };
    assert!(world.evaluate_with("((x))", &depth_limited).is_err());
    assert_eq!(world.revision(), 0);
}

#[test]
fn agent_mailbox_mutations_remain_transactional() {
    let mut world = World::default();
    world.evaluate("(def worker (spawn \"worker\"))").unwrap();
    world
        .evaluate("(send worker 'hello) (signal 'test/failure \"abort\")")
        .unwrap_err();
    assert_eq!(last(&mut world, "(recv worker)"), Value::Nil);
}

#[test]
fn rollback_preserves_monotonic_revision_ids() {
    let mut world = World::default();
    world.evaluate("(def value 1)").unwrap();
    world.evaluate("(def value 2)").unwrap();
    assert_eq!(world.rollback(), Some(1));
    let commit = world.evaluate("(def value 3)").unwrap();
    assert_eq!(commit.revision, 3);
}

#[test]
fn reflection_and_apply_are_small_homoiconic_primitives() {
    let mut world = World::default();
    let value = last(
        &mut world,
        "(list
           (type-of '(+ 1 2))
           (type-of +)
           (has-key? (dict 'present nil) 'present)
           (has-key? (dict 'present nil) 'missing)
           (< 10 20)
           (< 20 10)
           (apply + '(10 20 12))
           (apply (fn (x y) (* x y)) '(6 7)))",
    );
    assert_eq!(
        value,
        Value::List(vec![
            Value::Symbol("list".into()),
            Value::Symbol("callable".into()),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
            Value::Bool(false),
            Value::Int(42),
            Value::Int(42),
        ])
    );
}

#[test]
fn integer_ordering_is_checked_and_type_safe() {
    let mut world = World::default();
    assert_eq!(last(&mut world, "(< -10 0)"), Value::Bool(true));
    let error = world.evaluate("(< 1 'two)").unwrap_err();
    assert!(error.to_string().contains("< expects two integers"));
}

mod host_words {
    use agel_core::{EvaluationOptions, HostError, HostWord, Value, World};

    fn twice(arguments: &[Value]) -> Result<Value, HostError> {
        match arguments {
            [Value::Int(value)] => Ok(Value::Int(value * 2)),
            _ => Err(HostError {
                kind: "type".into(),
                message: "twice expects an integer".into(),
            }),
        }
    }

    fn secret(_: &[Value]) -> Result<Value, HostError> {
        Ok(Value::Symbol("opened".into()))
    }

    static HOST: [HostWord; 2] = [
        HostWord {
            name: "twice",
            capability: "",
            call: twice,
        },
        HostWord {
            name: "secret",
            capability: "vault/read",
            call: secret,
        },
    ];

    fn options() -> EvaluationOptions {
        EvaluationOptions {
            host: &HOST,
            ..EvaluationOptions::default()
        }
    }

    #[test]
    fn a_host_word_applies_through_the_options_table() {
        let mut world = World::default();
        world.install_host(&HOST);
        let commit = world.evaluate_with("(twice 21)", &options()).unwrap();
        assert_eq!(commit.values, vec![Value::Int(42)]);
        let error = world.evaluate_with("(twice 'x)", &options()).unwrap_err();
        assert!(
            error.to_string().contains("twice expects an integer"),
            "{error}"
        );
        // The same binding with no table behind it is refused, not applied.
        let error = world.evaluate("(twice 21)").unwrap_err();
        assert!(error.to_string().contains("host/unavailable"), "{error}");
    }

    #[test]
    fn a_host_word_needs_its_capability_kind() {
        let mut world = World::default();
        world.install_host(&HOST);
        let error = world.evaluate_with("(secret)", &options()).unwrap_err();
        assert!(error.to_string().contains("capability/denied"), "{error}");
        let capability = world.issue_capability("vault/read", "*").unwrap();
        let mut granted = options();
        granted.capabilities.push(capability);
        let commit = world.evaluate_with("(secret)", &granted).unwrap();
        assert_eq!(commit.values, vec![Value::Symbol("opened".into())]);
        // An agent holds only what it was spawned with: the evaluation's
        // capabilities do not reach its turn.
        world
            .evaluate_with(
                "(def peek (fn (self heap message) (secret)))
                 (def bare (spawn \"bare\" peek nil nil))
                 (send bare 'go) (run 1)",
                &granted,
            )
            .unwrap();
        let status = world
            .evaluate_with("(get (agent-info bare) 'status)", &granted)
            .unwrap();
        assert_eq!(status.values, vec![Value::Symbol("stopped".into())]);
        let commit = world
            .evaluate_with(
                "(def cap (request-capability 'vault/read \"*\"))
                 (def keeper (spawn \"keeper\" peek nil nil nil 'stop 0 (list cap)))
                 (send keeper 'go) (run 1)
                 (get (agent-info keeper) 'status)",
                &granted,
            )
            .unwrap();
        assert_eq!(commit.values.last(), Some(&Value::Symbol("running".into())));
    }
}

mod canonical_digest {
    use agel_core::{Snapshot, World};

    /// The content digest of a fresh world, pinned: the canonical encoding
    /// changes only with its version prefix, and this vector catches an
    /// accidental change to either.
    const FRESH_WORLD: &str = "44a2357b2daf253d400069de6ab46390b76871619e8ad9923fbee7d2b0fe9ebe";

    #[test]
    fn a_fresh_world_has_the_pinned_digest() {
        assert_eq!(World::default().content_digest().to_hex(), FRESH_WORLD);
    }

    #[test]
    fn equal_states_digest_equal_and_a_definition_changes_it() {
        let mut left = World::default();
        let mut right = World::default();
        for world in [&mut left, &mut right] {
            world
                .evaluate("(def twice (fn (x) (* x 2))) (def worker (spawn \"worker\")) (send worker 'hello)")
                .unwrap();
        }
        assert_eq!(left.content_digest(), right.content_digest());
        assert_eq!(left.state_digest(), right.state_digest());
        right.evaluate("(def more 1)").unwrap();
        assert_ne!(left.content_digest(), right.content_digest());
        // A snapshot restores the same state, and the same digest.
        let snapshot: Snapshot = left.snapshot();
        let restored = World::from_snapshot(&snapshot).unwrap();
        assert_eq!(restored.content_digest(), left.content_digest());
    }
}

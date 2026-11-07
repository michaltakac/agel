use agel_core::{Value, World};

#[test]
fn encoder_refresh_switches_the_callable_with_the_active_version() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    for source in [
        include_str!("../bytes.agel"),
        include_str!("../services.agel"),
        include_str!("../work-profile.agel"),
        "(import agel/evolution) (import agel/judge-log)
         (def check (fn (ok reason) (if ok #t (signal 'test/failure reason))))
         (def audit (fn (_) #t)) (def retained-policy nil)",
        include_str!("../../../examples/native-service-optimize.agel"),
    ] {
        world.evaluate(source).unwrap();
    }
    let result = world
        .evaluate(
            r#"(def before (retained-quantum 256))
              (def new '(fn (limit) (* 64 (/ limit 64))))
              (def policy-versions
                (evolution-edit policy-versions 'cell 'service-quantum initial-policy new
                  (list (dict 'arguments '(327) 'expected 320))
                  (fn (_) (dict 'allowed #t 'reason "fixture"))))
              (def observed-length 512)
              (def policy-bound 327)
              (refresh-encoder)
              (list before (retained-quantum 256) (retained-quantum 513) policy-uses)"#,
        )
        .unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("(64 320 64 1)")
    );
}

#[test]
fn paired_scores_keep_unknown_cost_and_check_transfer_provenance() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../benchmark.agel")).unwrap();
    assert_eq!(
        world
            .evaluate(include_str!("benchmark.agel"))
            .unwrap()
            .values
            .pop(),
        Some(Value::Bool(true))
    );
}

#[test]
fn retained_combat_policy_replays_with_a_cap_below_the_measured_bound() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../benchmark.agel")).unwrap();
    world.evaluate(include_str!("../bytes.agel")).unwrap();
    world.evaluate(include_str!("../services.agel")).unwrap();
    world.evaluate(include_str!("../work-profile.agel")).unwrap();
    let history = world
        .evaluate(
            r#"(import agel/evolution)
             (def judge (fn (_) (dict 'allowed #t 'reason "fixture")))
             (def old '(fn (limit) 64))
             (def new '(fn (limit) (if (< limit 192) (* 64 (/ limit 64)) 192)))
             (def cases (list (dict 'arguments '(64) 'expected 64)))
             (def base (evolution-edit (evolution-new) 'cell 'service-quantum nil old cases judge))
             (dict 'event 'live-policy-versions
                   'versions (evolution-edit base 'cell 'service-quantum old new cases judge))"#,
        )
        .unwrap()
        .values
        .pop()
        .unwrap();
    let recorded = "00ff".repeat(512);
    let audit = format!(
        "{{event runtime-observation profile {{bytes 1024 bound 327 baseline {{calls 16}}}}}}\n(recorded-workload 1024 \"{recorded}\")\n{history}\n"
    );
    world
        .evaluate(&format!(
            "(def file-read (fn (_) {}))
             (def console-log (fn (_) nil))
             (def clock-us (fn () 0))
             (def print-form (fn (_) \"retained lineage\"))
             (def check (fn (ok reason) (if ok #t (signal 'test/failure reason))))",
            Value::String(audit)
        ))
        .unwrap();
    world
        .evaluate_with(
            include_str!("../../../examples/native-combat-transfer.agel"),
            &agel_core::EvaluationOptions {
                budget: agel_core::Budget {
                    fuel: 10_000_000,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        world
            .evaluate("(list retained-bound retained-quantum-replay (get retained-lineage 'version))")
            .unwrap()
            .values
            .pop(),
        Some(Value::List(vec![Value::Int(327), Value::Int(192), Value::Int(2)]))
    );
}

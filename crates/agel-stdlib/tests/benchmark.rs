use agel_core::{Value, World};

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
fn retained_combat_policy_replays_at_a_non_aligned_measurement_bound() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../benchmark.agel")).unwrap();
    world.evaluate(include_str!("../bytes.agel")).unwrap();
    let history = world
        .evaluate(
            r#"(import agel/evolution)
             (def judge (fn (_) (dict 'allowed #t 'reason "fixture")))
             (def old '(fn (limit) 64))
             (def new '(fn (limit) (* 64 (/ limit 64))))
             (def cases (list (dict 'arguments '(64) 'expected 64)))
             (def base (evolution-edit (evolution-new) 'cell 'service-quantum nil old cases judge))
             (dict 'event 'live-policy-versions
                   'versions (evolution-edit base 'cell 'service-quantum old new cases judge))"#,
        )
        .unwrap()
        .values
        .pop()
        .unwrap();
    let audit = format!(
        "{{event runtime-observation profile {{bytes 2 bound 327}}}}\n(recorded-workload 2 \"00ff\")\n{history}\n"
    );
    world
        .evaluate(&format!(
            "(def file-read (fn (_) {}))
             (def console-log (fn (_) nil))
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
            .evaluate("(list retained-bound (get retained-lineage 'version))")
            .unwrap()
            .values
            .pop(),
        Some(Value::List(vec![Value::Int(327), Value::Int(2)]))
    );
}

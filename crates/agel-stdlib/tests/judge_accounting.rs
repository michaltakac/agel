use agel_core::{EvaluationOptions, Value, World};

#[test]
fn lisp_prices_usage_and_preserves_unknown_costs() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    let result = world
        .evaluate(include_str!("judge_accounting.agel"))
        .unwrap()
        .values
        .pop();
    assert_eq!(result, Some(Value::Bool(true)));
}

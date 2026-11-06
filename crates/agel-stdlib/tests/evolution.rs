use agel_core::{EvaluationOptions, Value, World};

#[test]
fn lisp_evolution_contract() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    let result = world
        .evaluate(include_str!("evolution.agel"))
        .unwrap()
        .values
        .pop();
    assert_eq!(result, Some(Value::Bool(true)));
}

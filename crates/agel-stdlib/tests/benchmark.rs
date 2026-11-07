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

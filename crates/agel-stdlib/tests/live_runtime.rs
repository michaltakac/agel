use agel_core::{Value, World};

#[test]
fn gated_forms_change_existing_closures_without_recreating_them() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../evolution.agel")).unwrap();
    world
        .evaluate(include_str!("../live-runtime.agel"))
        .unwrap();
    assert_eq!(
        world
            .evaluate(include_str!("live-runtime.agel"))
            .unwrap()
            .values
            .pop(),
        Some(Value::Bool(true))
    );
}

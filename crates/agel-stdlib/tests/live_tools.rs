use agel_core::{Value, World};

#[test]
fn live_tool_dispatch_keeps_identity_history_and_mailbox() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../live-tools.agel")).unwrap();
    let result = world
        .evaluate(include_str!("live-tools.agel"))
        .unwrap()
        .values
        .pop();
    assert_eq!(result, Some(Value::Bool(true)));
}

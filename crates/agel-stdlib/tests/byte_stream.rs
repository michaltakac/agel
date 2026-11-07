use agel_core::{EvaluationOptions, Value, World};
#[test]
fn byte_adapters_bound_and_validate_the_transport() {
    let mut world = World::default();
    let options = EvaluationOptions::default();
    agel_stdlib::install(&mut world, &options).unwrap();
    for source in [
        include_str!("../bytes.agel"),
        include_str!("../byte-stream.agel"),
        include_str!("../buffered-stream.agel"),
    ] {
        world.evaluate(source).unwrap();
    }
    assert_eq!(
        world
            .evaluate(include_str!("byte-stream.agel"))
            .unwrap()
            .values
            .pop(),
        Some(Value::Bool(true))
    );
}

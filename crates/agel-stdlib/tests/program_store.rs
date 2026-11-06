use agel_core::{Budget, EvaluationOptions, Value, World};

#[test]
fn crc_matches_independent_bitwise_reference() {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 50_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    agel_stdlib::install(&mut world, &options).unwrap();
    world
        .evaluate_with(include_str!("../bytes.agel"), &options)
        .unwrap();
    let bytes = (0..=255).collect::<Vec<u8>>();
    let mut expected = u32::MAX;
    for byte in &bytes {
        expected ^= u32::from(*byte);
        for _ in 0..8 {
            expected = (expected >> 1) ^ (if expected & 1 != 0 { 0xedb8_8320 } else { 0 });
        }
    }
    let arguments = Value::List(
        bytes
            .into_iter()
            .map(|b| Value::Int(i64::from(b)))
            .collect(),
    );
    let actual = world
        .evaluate_with(
            &format!("(import agel/bytes) (bytes-crc32 '{arguments})"),
            &options,
        )
        .unwrap()
        .values
        .pop();
    assert_eq!(actual, Some(Value::Int(i64::from(!expected))));
}

#[test]
fn program_table_crc_and_publication_are_agel() {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 50_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    agel_stdlib::install(&mut world, &options).unwrap();
    for source in [
        include_str!("../bytes.agel"),
        include_str!("../program-store.agel"),
        include_str!("../native-program.agel"),
    ] {
        world.evaluate_with(source, &options).unwrap();
    }
    let result = world
        .evaluate_with(include_str!("program-store.agel"), &options)
        .unwrap()
        .values
        .pop();
    assert_eq!(result, Some(Value::Bool(true)));
}

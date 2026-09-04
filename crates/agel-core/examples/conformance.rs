use agel_core::{read_all, World};

fn main() {
    let source = std::fs::read_to_string("bootstrap/conformance.forms")
        .expect("run from the Agel repository root");
    let expected_forms = read_all(&source).expect("valid conformance forms").len();
    let mut world = World::default();
    let commit = world.evaluate(&source).expect("Rust seed evaluates suite");
    assert_eq!(commit.values.len(), expected_forms);
    for value in commit.values {
        println!("{value}");
    }

    let failures = std::fs::read_to_string("bootstrap/conformance-errors.forms")
        .expect("error conformance corpus exists");
    for source in failures
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';'))
    {
        let mut isolated = World::default();
        assert!(
            isolated.evaluate(source).is_err(),
            "Rust seed accepted {source}"
        );
        println!("error");
    }
}

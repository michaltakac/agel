//! World files: a world's canonical encoding read back, whole or as a
//! delta over a freshly installed library.
use agel_core::{EvaluationOptions, Value, World};

fn session() -> World {
    let mut world = World::new(0);
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    world
        .evaluate(
            "(import agel/sequence)
             (def squares (map (fn (x) (* x x)) (list 1 2 3)))
             (def twice (fn (x) (let ((two 2)) (* x two))))
             (defmacro unless (c b) (if c nil b))
             (def worker (spawn \"worker\")) (send worker 'hello)
             (module mine (export answer) (def answer 42))",
        )
        .unwrap();
    world
}

fn same_answers(world: &mut World) {
    let commit = world
        .evaluate("(import mine) (list squares (twice 21) (unless #f 'kept) (recv worker) answer)")
        .unwrap();
    assert_eq!(
        commit.values.last(),
        Some(&Value::List(vec![
            Value::List(vec![Value::Int(1), Value::Int(4), Value::Int(9)]),
            Value::Int(42),
            Value::Symbol("kept".into()),
            Value::Symbol("hello".into()),
            Value::Int(42),
        ]))
    );
}

#[test]
fn a_world_file_holds_the_whole_state() {
    let mut world = session();
    let bytes = world.to_canonical();
    let mut restored = World::from_canonical(&bytes).unwrap();
    assert_eq!(restored.content_digest(), world.content_digest());
    assert_eq!(restored.revision(), world.revision());
    same_answers(&mut restored);
    same_answers(&mut world);
    assert_eq!(restored.content_digest(), world.content_digest());
    // Refused, not guessed: a truncated file and a wrong version.
    assert!(World::from_canonical(&bytes[..bytes.len() / 2]).is_err());
    let error = World::from_canonical(&bytes[..3]).unwrap_err();
    assert!(
        error.to_string().contains("canonical encoding at byte"),
        "{error}"
    );
}

#[test]
fn a_delta_over_the_library_is_small_and_restores_the_session() {
    let mut base = World::new(0);
    agel_stdlib::install(&mut base, &EvaluationOptions::default()).unwrap();
    let mut world = session();
    let whole = world.to_canonical();
    let delta = world.to_canonical_over(&base);
    assert!(
        delta.len() * 20 < whole.len(),
        "delta {} whole {}",
        delta.len(),
        whole.len()
    );
    let mut restored = World::from_canonical_over(base.clone(), &delta).unwrap();
    assert_eq!(restored.content_digest(), world.content_digest());
    same_answers(&mut restored);
    same_answers(&mut world);
    assert_eq!(restored.content_digest(), world.content_digest());
    // A capability the saved world issued still permits in the restored one.
    let capability = world.issue_capability("vault/read", "*").unwrap();
    let delta = world.to_canonical_over(&base);
    let mut restored = World::from_canonical_over(base, &delta).unwrap();
    let mut options = EvaluationOptions::default();
    options.capabilities.push(capability);
    let commit = restored
        .evaluate_with(
            "(capability-kind (request-capability 'vault/read \"*\"))",
            &options,
        )
        .unwrap();
    assert_eq!(commit.values, vec![Value::String("vault/read".into())]);
    println!("delta {} bytes, whole {} bytes", delta.len(), whole.len());
}

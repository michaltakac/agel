use agel_core::{Value, World};
use agel_verify::{Proposal, TestCase, Verifier};

fn main() {
    let mut world = World::default();
    world.evaluate("(def transform (fn (x) (+ x 1)))").unwrap();

    let unsafe_change = Proposal::new(&world, "(def transform (fn (x) (/ x 0)))")
        .tests(TestCase::new("(transform 9)", Value::Int(10)));
    println!(
        "unsafe proposal: {}",
        Verifier::verify(&world, &unsafe_change).unwrap_err()
    );
    println!(
        "live behavior after rejection: {}",
        world.evaluate("(transform 9)").unwrap().values[0]
    );

    let safe_change = Proposal::new(&world, "(def transform (fn (x) (+ 1 x)))")
        .tests(TestCase::new("(transform 9)", Value::Int(10)))
        .tests(TestCase::new("(transform -1)", Value::Int(0)));
    let evidence = Verifier::verify(&world, &safe_change).unwrap();
    println!(
        "evidence {}: {} tests passed in isolated world",
        evidence.proposal_digest, evidence.tests_passed
    );
    let commit = Verifier::promote(&mut world, &safe_change, &evidence).unwrap();
    println!("promoted atomically as revision {}", commit.revision);
    println!(
        "live behavior after promotion: {}",
        world.evaluate("(transform 41)").unwrap().values[0]
    );
}

use agel_core::{EvaluationOptions, World};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::default();
    world.evaluate(
        "(defprotocol accumulator (add int))
         (def behavior (fn (self heap message) (+ heap (car (cdr message)))))
         (def actor (spawn \"replayable\" behavior 0 accumulator))
         (send actor '(add 20))
         (send actor '(add 22))",
    )?;

    let snapshot = world.snapshot();
    let input_log = vec!["(run)".to_owned(), "(agent-info actor)".to_owned()];
    let first = World::replay(&snapshot, &input_log, &EvaluationOptions::default())?;
    let second = World::replay(&snapshot, &input_log, &EvaluationOptions::default())?;

    assert_eq!(first, second, "identical input must replay identically");
    println!("snapshot  {:016x}", snapshot.digest());
    println!("final     {:016x}", first.final_digest);
    println!("steps     {}", first.steps_used);
    println!("events    {}", first.events.len());
    println!("final actor info: {}", first.values[1][0]);
    println!("replay verified: two independent executions are identical");
    Ok(())
}

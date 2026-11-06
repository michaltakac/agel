use agel_core::{Budget, EvaluationOptions, Value, World};
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("usage: journal FRAMED_FILE")?;
    let bytes = fs::read(path)?;
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 200_000_000,
            max_collection_len: 4 * 1024 * 1024,
            ..Budget::default()
        },
        ..EvaluationOptions::default()
    };
    world.evaluate_with(include_str!("../journal-frame.agel"), &options)?;
    world.evaluate("(import agel/journal-frame) (def journal-state (journal-reader))")?;
    for frame in bytes.chunks(512) {
        let frame = String::from_utf8_lossy(frame).into_owned();
        let source = format!(
            "(begin (def journal-state (journal-feed journal-state {})) #t)",
            Value::String(frame)
        );
        world.evaluate_with(&source, &options)?;
    }
    let result = world
        .evaluate("(get (journal-snapshot journal-state) 'records)")?
        .values
        .pop();
    if let Some(Value::List(records)) = result {
        for record in records {
            if let Value::String(text) = record {
                print!("{text}");
            }
        }
    }
    let health = world.evaluate(
        "(list (get journal-state 'corrupt) (get (journal-snapshot journal-state) 'incomplete))",
    )?;
    eprintln!(
        "journal health: {}",
        health.values.last().ok_or("no health result")?
    );
    Ok(())
}

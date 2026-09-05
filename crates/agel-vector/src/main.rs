use agel_core::{EvaluationOptions, World};
use agel_vector::SvgRenderer;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

fn main() -> io::Result<()> {
    let mut output = PathBuf::from("target/agel-desktop.svg");
    let mut program = None::<PathBuf>;
    let mut scale = 2_i64;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--output" => {
                output = arguments
                    .next()
                    .ok_or_else(|| invalid("--output requires a path"))?
                    .into()
            }
            "--scale" => {
                scale = arguments
                    .next()
                    .ok_or_else(|| invalid("--scale requires an integer"))?
                    .parse()
                    .map_err(|_| invalid("--scale requires an integer"))?;
            }
            "--program" => {
                program = Some(
                    arguments
                        .next()
                        .ok_or_else(|| invalid("--program requires a path"))?
                        .into(),
                )
            }
            "--help" | "-h" => {
                println!("Usage: agel-vector [--program FILE] [--output PATH] [--scale 1..8]");
                return Ok(());
            }
            _ => return Err(invalid(&format!("unknown argument: {argument}"))),
        }
    }

    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default())
        .map_err(|error| io::Error::other(error.to_string()))?;
    let source = match program {
        Some(path) => fs::read_to_string(path)?,
        None => format!(
            "(import agel/desktop) (import agel/ui-layout) (import agel/ui-vector)\n\
             (compile-vector-frame\n\
               (compile-frame (default-scene) default-viewport default-theme) {scale})"
        ),
    };
    let frame = world
        .evaluate(&source)
        .map_err(|error| io::Error::other(error.to_string()))?
        .values
        .pop()
        .ok_or_else(|| io::Error::other("Agel produced no vector frame"))?;
    let svg = SvgRenderer::default()
        .render(&frame)
        .map_err(|error| io::Error::other(error.to_string()))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, svg)?;
    println!("Rendered Agel vector frame to {}", output.display());
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

//! The renderer is the output boundary for language-authored frames: it must be
//! deterministic and must reject anything the language did not validly produce.

use agel_core::{EvaluationOptions, Value, World};
use agel_integrity::sha256;
use agel_vector::{RenderLimits, SvgRenderer};

const KITCHENSINK_SVG_SHA256: &str =
    "c7265e746eef554088d435ee626b533b23e123f07eef6c285c69e4252a2b333f";

fn world() -> World {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    world
}

fn frame(world: &mut World, source: &str) -> Value {
    world.evaluate(source).unwrap().values.pop().unwrap()
}

fn default_frame(scale: i64) -> Value {
    frame(
        &mut world(),
        &format!(
            "(import agel/desktop) (import agel/ui-layout) (import agel/ui-vector)
             (compile-vector-frame (compile-frame (default-scene) default-viewport default-theme) {scale})"
        ),
    )
}

#[test]
fn kitchensink_frame_renders_to_the_frozen_svg_digest() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/kitchensink.agel"
    ))
    .unwrap();
    let frame = frame(&mut world(), &source);
    let svg = SvgRenderer::default().render(&frame).unwrap();
    assert_eq!(sha256(svg.as_bytes()).to_hex(), KITCHENSINK_SVG_SHA256);
    assert_eq!(SvgRenderer::default().render(&frame).unwrap(), svg);
}

#[test]
fn default_desktop_is_deterministic_and_scales_without_changing_geometry() {
    let one = SvgRenderer::default().render(&default_frame(1)).unwrap();
    let two = SvgRenderer::default().render(&default_frame(2)).unwrap();
    assert_eq!(
        one,
        SvgRenderer::default().render(&default_frame(1)).unwrap()
    );
    assert_ne!(one, two);
    assert!(one.starts_with("<svg"));
    assert!(one.contains("viewBox=\"0 0 "));
    let viewbox = |svg: &str| {
        let start = svg.find("viewBox=\"").unwrap() + 9;
        let end = svg[start..].find('"').unwrap();
        svg[start..start + end].to_owned()
    };
    assert_eq!(viewbox(&one), viewbox(&two));
}

#[test]
fn malformed_and_oversized_frames_are_rejected() {
    let renderer = SvgRenderer::default();
    assert!(renderer.render(&Value::Nil).is_err());
    assert!(renderer.render(&Value::List(vec![])).is_err());
    let mut world = world();
    for source in [
        "(dict 'kind 'vector-frame)",
        "(dict 'kind 'wrong 'viewport (dict 'x 0 'y 0 'width 10 'height 10) 'scale 1 'commands nil)",
        "(dict 'kind 'vector-frame 'viewport (dict 'x 0 'y 0 'width 10 'height 10) 'scale 9 'commands nil)",
        "(dict 'kind 'vector-frame 'viewport (dict 'x 0 'y 0 'width 0 'height 10) 'scale 1 'commands nil)",
        "(dict 'kind 'vector-frame 'viewport (dict 'x 0 'y 0 'width 10 'height 10) 'scale 1 'commands (list (dict 'kind 'teleport)))",
    ] {
        let value = frame(&mut world, source);
        assert!(renderer.render(&value).is_err(), "accepted {source}");
    }
    assert!(renderer.render(&default_frame(9)).is_err());
    let tiny = SvgRenderer::new(RenderLimits {
        max_commands: 1,
        ..RenderLimits::default()
    });
    assert!(tiny.render(&default_frame(1)).is_err());
    let small_output = SvgRenderer::new(RenderLimits {
        max_output_bytes: 64,
        ..RenderLimits::default()
    });
    assert!(small_output.render(&default_frame(1)).is_err());
}

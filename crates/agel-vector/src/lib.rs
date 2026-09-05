//! A narrow, safe renderer boundary for Agel's self-described vector IR.
//!
//! Scene semantics, layout, vector construction, transforms, clips, and paints
//! live in `agel/vector` and `agel/ui-vector`. This crate distrusts that input,
//! validates it again, and emits deterministic SVG without third-party code.

use agel_core::Value;
use std::fmt::{self, Write as _};

const DEFAULT_MAX_COMMANDS: usize = 100_000;
const DEFAULT_MAX_PATH_SEGMENTS: usize = 1_000_000;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_DIMENSION: i64 = 32_768;
const MAX_SCALE: i64 = 8;

#[derive(Clone, Debug)]
pub struct RenderLimits {
    pub max_commands: usize,
    pub max_path_segments: usize,
    pub max_output_bytes: usize,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_commands: DEFAULT_MAX_COMMANDS,
            max_path_segments: DEFAULT_MAX_PATH_SEGMENTS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError(String);

impl RenderError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RenderError {}

#[derive(Clone, Debug)]
pub struct SvgRenderer {
    limits: RenderLimits,
}

impl Default for SvgRenderer {
    fn default() -> Self {
        Self::new(RenderLimits::default())
    }
}

impl SvgRenderer {
    pub fn new(limits: RenderLimits) -> Self {
        Self { limits }
    }

    pub fn render(&self, frame: &Value) -> Result<String, RenderError> {
        let frame = map(frame, "vector frame")?;
        expect_symbol(get(frame, "kind")?, "vector-frame", "frame kind")?;
        let viewport = rect(get(frame, "viewport")?)?;
        let scale = positive_int(get(frame, "scale")?, "scale")?;
        if scale > MAX_SCALE {
            return Err(RenderError::new("scale exceeds renderer limit of 8"));
        }
        let physical_width = positive_int(get(frame, "physical-width")?, "physical width")?;
        let physical_height = positive_int(get(frame, "physical-height")?, "physical height")?;
        if physical_width != checked_mul(viewport.width, scale, "physical width")?
            || physical_height != checked_mul(viewport.height, scale, "physical height")?
        {
            return Err(RenderError::new(
                "physical dimensions do not match viewport and scale",
            ));
        }
        if physical_width > MAX_DIMENSION || physical_height > MAX_DIMENSION {
            return Err(RenderError::new(
                "physical dimensions exceed renderer limit",
            ));
        }
        let commands = list(get(frame, "commands")?, "commands")?;
        if commands.len() > self.limits.max_commands {
            return Err(RenderError::new("vector command limit exceeded"));
        }

        let mut path_segments = 0usize;
        let mut depth = 0usize;
        for command in commands {
            validate_command(command, &mut path_segments, self.limits.max_path_segments)?;
            match symbol(get(map(command, "command")?, "op")?, "command operation")? {
                "push-transform" | "push-clip" => {
                    depth = depth
                        .checked_add(1)
                        .ok_or_else(|| RenderError::new("state stack overflow"))?;
                }
                "restore" => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| RenderError::new("unbalanced restore"))?;
                }
                _ => {}
            }
        }
        if depth != 0 {
            return Err(RenderError::new("unbalanced vector state stack"));
        }

        let mut svg = String::with_capacity(commands.len().saturating_mul(128));
        writeln!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{physical_width}\" height=\"{physical_height}\" viewBox=\"{} {} {} {}\" role=\"img\" aria-label=\"Agel agentic desktop\">",
            viewport.x, viewport.y, viewport.width, viewport.height
        )
        .expect("writing to a String cannot fail");
        svg.push_str("<defs>\n");
        for (index, command) in commands.iter().enumerate() {
            emit_definition(&mut svg, command, index)?;
        }
        svg.push_str("</defs>\n");
        svg.push_str(
            "<g shape-rendering=\"geometricPrecision\" text-rendering=\"geometricPrecision\">\n",
        );
        for (index, command) in commands.iter().enumerate() {
            emit_command(&mut svg, command, index)?;
            if svg.len() > self.limits.max_output_bytes {
                return Err(RenderError::new("SVG output limit exceeded"));
            }
        }
        svg.push_str("</g>\n</svg>\n");
        if svg.len() > self.limits.max_output_bytes {
            return Err(RenderError::new("SVG output limit exceeded"));
        }
        Ok(svg)
    }
}

#[derive(Clone, Copy)]
struct Rect {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

fn map<'a>(value: &'a Value, context: &str) -> Result<&'a [(Value, Value)], RenderError> {
    match value {
        Value::Map(entries) => Ok(entries),
        _ => Err(RenderError::new(format!("{context} must be a map"))),
    }
}

fn list<'a>(value: &'a Value, context: &str) -> Result<&'a [Value], RenderError> {
    match value {
        Value::List(values) => Ok(values),
        Value::Nil => Ok(&[]),
        _ => Err(RenderError::new(format!("{context} must be a list"))),
    }
}

fn get<'a>(entries: &'a [(Value, Value)], name: &str) -> Result<&'a Value, RenderError> {
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Value::Symbol(symbol) if symbol == name => Some(value),
            _ => None,
        })
        .ok_or_else(|| RenderError::new(format!("missing {name}")))
}

fn symbol<'a>(value: &'a Value, context: &str) -> Result<&'a str, RenderError> {
    match value {
        Value::Symbol(value) => Ok(value),
        _ => Err(RenderError::new(format!("{context} must be a symbol"))),
    }
}

fn string<'a>(value: &'a Value, context: &str) -> Result<&'a str, RenderError> {
    match value {
        Value::String(value) => Ok(value),
        _ => Err(RenderError::new(format!("{context} must be a string"))),
    }
}

fn int(value: &Value, context: &str) -> Result<i64, RenderError> {
    match value {
        Value::Int(value) => Ok(*value),
        _ => Err(RenderError::new(format!("{context} must be an integer"))),
    }
}

fn positive_int(value: &Value, context: &str) -> Result<i64, RenderError> {
    let value = int(value, context)?;
    if value > 0 {
        Ok(value)
    } else {
        Err(RenderError::new(format!("{context} must be positive")))
    }
}

fn nonnegative_int(value: &Value, context: &str) -> Result<i64, RenderError> {
    let value = int(value, context)?;
    if value >= 0 {
        Ok(value)
    } else {
        Err(RenderError::new(format!("{context} must be non-negative")))
    }
}

fn checked_mul(left: i64, right: i64, context: &str) -> Result<i64, RenderError> {
    left.checked_mul(right)
        .ok_or_else(|| RenderError::new(format!("{context} overflow")))
}

fn expect_symbol(value: &Value, expected: &str, context: &str) -> Result<(), RenderError> {
    if symbol(value, context)? == expected {
        Ok(())
    } else {
        Err(RenderError::new(format!("unexpected {context}")))
    }
}

fn rect(value: &Value) -> Result<Rect, RenderError> {
    let entries = map(value, "rectangle")?;
    Ok(Rect {
        x: int(get(entries, "x")?, "rectangle x")?,
        y: int(get(entries, "y")?, "rectangle y")?,
        width: positive_int(get(entries, "width")?, "rectangle width")?,
        height: positive_int(get(entries, "height")?, "rectangle height")?,
    })
}

fn point(value: &Value) -> Result<(i64, i64), RenderError> {
    let entries = map(value, "point")?;
    expect_symbol(get(entries, "kind")?, "point", "point kind")?;
    Ok((
        int(get(entries, "x")?, "point x")?,
        int(get(entries, "y")?, "point y")?,
    ))
}

fn color(value: &Value) -> Result<&str, RenderError> {
    let color = string(value, "color")?;
    let valid = color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if valid {
        Ok(color)
    } else {
        Err(RenderError::new("colors must use #RRGGBB syntax"))
    }
}

fn paint(value: &Value) -> Result<Paint<'_>, RenderError> {
    if let Value::String(_) = value {
        return Ok(Paint::Solid(color(value)?));
    }
    let entries = map(value, "paint")?;
    match symbol(get(entries, "kind")?, "paint kind")? {
        "solid" => Ok(Paint::Solid(color(get(entries, "color")?)?)),
        "linear-gradient" => {
            let (x1, y1) = point(get(entries, "from")?)?;
            let (x2, y2) = point(get(entries, "to")?)?;
            if [x1, y1, x2, y2]
                .iter()
                .any(|coordinate| !(0..=1024).contains(coordinate))
            {
                return Err(RenderError::new(
                    "gradient coordinates must be between 0 and 1024",
                ));
            }
            Ok(Paint::LinearGradient {
                x1,
                y1,
                x2,
                y2,
                start: color(get(entries, "start")?)?,
                end: color(get(entries, "end")?)?,
            })
        }
        _ => Err(RenderError::new("unknown paint kind")),
    }
}

enum Paint<'a> {
    Solid(&'a str),
    LinearGradient {
        x1: i64,
        y1: i64,
        x2: i64,
        y2: i64,
        start: &'a str,
        end: &'a str,
    },
}

fn validate_shape(
    value: &Value,
    path_segments: &mut usize,
    max_path_segments: usize,
) -> Result<(), RenderError> {
    let entries = map(value, "shape")?;
    match symbol(get(entries, "kind")?, "shape kind")? {
        "rounded-rectangle" => {
            rect(get(entries, "bounds")?)?;
            nonnegative_int(get(entries, "radius")?, "corner radius")?;
        }
        "ellipse" => {
            point(get(entries, "center")?)?;
            positive_int(get(entries, "radius-x")?, "ellipse radius x")?;
            positive_int(get(entries, "radius-y")?, "ellipse radius y")?;
        }
        "path" => {
            let segments = list(get(entries, "segments")?, "path segments")?;
            if segments.is_empty() {
                return Err(RenderError::new("path must contain a segment"));
            }
            *path_segments = path_segments
                .checked_add(segments.len())
                .ok_or_else(|| RenderError::new("path segment count overflow"))?;
            if *path_segments > max_path_segments {
                return Err(RenderError::new("path segment limit exceeded"));
            }
            for segment in segments {
                validate_segment(segment)?;
            }
        }
        _ => return Err(RenderError::new("unknown shape kind")),
    }
    Ok(())
}

fn validate_segment(value: &Value) -> Result<(), RenderError> {
    let entries = map(value, "path segment")?;
    match symbol(get(entries, "op")?, "path operation")? {
        "move-to" | "line-to" => {
            int(get(entries, "x")?, "path x")?;
            int(get(entries, "y")?, "path y")?;
        }
        "curve-to" => {
            for field in ["x1", "y1", "x2", "y2", "x", "y"] {
                int(get(entries, field)?, field)?;
            }
        }
        "close-path" => {}
        _ => return Err(RenderError::new("unknown path operation")),
    }
    Ok(())
}

fn validate_command(
    value: &Value,
    path_segments: &mut usize,
    max_path_segments: usize,
) -> Result<(), RenderError> {
    let command = map(value, "command")?;
    match symbol(get(command, "op")?, "command operation")? {
        "fill-shape" => {
            validate_shape(get(command, "shape")?, path_segments, max_path_segments)?;
            paint(get(command, "paint")?)?;
        }
        "stroke-shape" => {
            validate_shape(get(command, "shape")?, path_segments, max_path_segments)?;
            paint(get(command, "paint")?)?;
            positive_int(get(command, "width")?, "stroke width")?;
        }
        "draw-text" => {
            rect(get(command, "bounds")?)?;
            string(get(command, "content")?, "text content")?;
            symbol(get(command, "role")?, "text role")?;
            paint(get(command, "paint")?)?;
        }
        "push-transform" => validate_transform(get(command, "transform")?)?,
        "push-clip" => {
            validate_shape(get(command, "shape")?, path_segments, max_path_segments)?;
        }
        "restore" => {}
        _ => return Err(RenderError::new("unknown vector command")),
    }
    Ok(())
}

fn validate_transform(value: &Value) -> Result<(), RenderError> {
    let entries = map(value, "transform")?;
    expect_symbol(get(entries, "kind")?, "transform", "transform kind")?;
    for field in ["a", "b", "c", "d", "tx", "ty"] {
        int(get(entries, field)?, field)?;
    }
    Ok(())
}

fn emit_definition(svg: &mut String, value: &Value, index: usize) -> Result<(), RenderError> {
    let command = map(value, "command")?;
    let operation = symbol(get(command, "op")?, "command operation")?;
    if matches!(operation, "fill-shape" | "stroke-shape" | "draw-text") {
        if let Paint::LinearGradient {
            x1,
            y1,
            x2,
            y2,
            start,
            end,
        } = paint(get(command, "paint")?)?
        {
            writeln!(svg, "<linearGradient id=\"paint-{index}\" gradientUnits=\"objectBoundingBox\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"><stop offset=\"0%\" stop-color=\"{start}\"/><stop offset=\"100%\" stop-color=\"{end}\"/></linearGradient>", fixed(x1), fixed(y1), fixed(x2), fixed(y2)).unwrap();
        }
    } else if operation == "push-clip" {
        write!(svg, "<clipPath id=\"clip-{index}\">").unwrap();
        emit_shape(svg, get(command, "shape")?)?;
        svg.push_str("</clipPath>\n");
    }
    Ok(())
}

fn emit_command(svg: &mut String, value: &Value, index: usize) -> Result<(), RenderError> {
    let command = map(value, "command")?;
    match symbol(get(command, "op")?, "command operation")? {
        "fill-shape" => {
            write!(
                svg,
                "<g fill=\"{}\"{}>",
                paint_reference(get(command, "paint")?, index)?,
                data_id(command)?
            )
            .unwrap();
            emit_shape(svg, get(command, "shape")?)?;
            svg.push_str("</g>\n");
        }
        "stroke-shape" => {
            write!(svg, "<g fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" vector-effect=\"non-scaling-stroke\"{}>", paint_reference(get(command, "paint")?, index)?, positive_int(get(command, "width")?, "stroke width")?, data_id(command)?).unwrap();
            emit_shape(svg, get(command, "shape")?)?;
            svg.push_str("</g>\n");
        }
        "draw-text" => emit_text(svg, command, index)?,
        "push-transform" => {
            let matrix = map(get(command, "transform")?, "transform")?;
            writeln!(
                svg,
                "<g transform=\"matrix({} {} {} {} {} {})\">",
                fixed(int(get(matrix, "a")?, "a")?),
                fixed(int(get(matrix, "b")?, "b")?),
                fixed(int(get(matrix, "c")?, "c")?),
                fixed(int(get(matrix, "d")?, "d")?),
                int(get(matrix, "tx")?, "tx")?,
                int(get(matrix, "ty")?, "ty")?
            )
            .unwrap();
        }
        "push-clip" => writeln!(svg, "<g clip-path=\"url(#clip-{index})\">").unwrap(),
        "restore" => svg.push_str("</g>\n"),
        _ => return Err(RenderError::new("unknown vector command")),
    }
    Ok(())
}

fn data_id(command: &[(Value, Value)]) -> Result<String, RenderError> {
    let id = match get(command, "id")? {
        Value::Symbol(value) | Value::String(value) => value.clone(),
        Value::Int(value) => value.to_string(),
        _ => {
            return Err(RenderError::new(
                "command id must be a symbol, string, or integer",
            ))
        }
    };
    Ok(format!(" data-agel-id=\"{}\"", escape_xml(&id)))
}

fn paint_reference(value: &Value, index: usize) -> Result<String, RenderError> {
    Ok(match paint(value)? {
        Paint::Solid(color) => color.to_owned(),
        Paint::LinearGradient { .. } => format!("url(#paint-{index})"),
    })
}

fn emit_shape(svg: &mut String, value: &Value) -> Result<(), RenderError> {
    let shape = map(value, "shape")?;
    match symbol(get(shape, "kind")?, "shape kind")? {
        "rounded-rectangle" => {
            let bounds = rect(get(shape, "bounds")?)?;
            let radius = nonnegative_int(get(shape, "radius")?, "corner radius")?;
            write!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{radius}\"/>",
                bounds.x, bounds.y, bounds.width, bounds.height
            )
            .unwrap();
        }
        "ellipse" => {
            let (cx, cy) = point(get(shape, "center")?)?;
            let rx = positive_int(get(shape, "radius-x")?, "radius x")?;
            let ry = positive_int(get(shape, "radius-y")?, "radius y")?;
            write!(
                svg,
                "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\"/>"
            )
            .unwrap();
        }
        "path" => {
            svg.push_str("<path d=\"");
            for segment in list(get(shape, "segments")?, "segments")? {
                emit_segment(svg, segment)?;
            }
            svg.push_str("\"/>");
        }
        _ => return Err(RenderError::new("unknown shape kind")),
    }
    Ok(())
}

fn emit_segment(svg: &mut String, value: &Value) -> Result<(), RenderError> {
    let segment = map(value, "segment")?;
    match symbol(get(segment, "op")?, "segment operation")? {
        "move-to" => write!(
            svg,
            "M{} {} ",
            int(get(segment, "x")?, "x")?,
            int(get(segment, "y")?, "y")?
        )
        .unwrap(),
        "line-to" => write!(
            svg,
            "L{} {} ",
            int(get(segment, "x")?, "x")?,
            int(get(segment, "y")?, "y")?
        )
        .unwrap(),
        "curve-to" => write!(
            svg,
            "C{} {},{} {},{} {} ",
            int(get(segment, "x1")?, "x1")?,
            int(get(segment, "y1")?, "y1")?,
            int(get(segment, "x2")?, "x2")?,
            int(get(segment, "y2")?, "y2")?,
            int(get(segment, "x")?, "x")?,
            int(get(segment, "y")?, "y")?
        )
        .unwrap(),
        "close-path" => svg.push_str("Z "),
        _ => return Err(RenderError::new("unknown path operation")),
    }
    Ok(())
}

fn emit_text(
    svg: &mut String,
    command: &[(Value, Value)],
    index: usize,
) -> Result<(), RenderError> {
    let bounds = rect(get(command, "bounds")?)?;
    let content = escape_xml(string(get(command, "content")?, "text")?);
    let role = symbol(get(command, "role")?, "text role")?;
    let (x, y, anchor, size, weight) = match role {
        "button" => (
            bounds.x + bounds.width / 2,
            bounds.y + bounds.height / 2,
            "middle",
            15,
            600,
        ),
        "title" => (
            bounds.x + 12,
            bounds.y + bounds.height / 2,
            "start",
            14,
            650,
        ),
        _ => (
            bounds.x + 12,
            bounds.y + bounds.height / 2,
            "start",
            18,
            450,
        ),
    };
    writeln!(svg, "<text x=\"{x}\" y=\"{y}\" fill=\"{}\" text-anchor=\"{anchor}\" dominant-baseline=\"middle\" font-family=\"system-ui,-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif\" font-size=\"{size}\" font-weight=\"{weight}\"{}>{content}</text>", paint_reference(get(command, "paint")?, index)?, data_id(command)?).unwrap();
    Ok(())
}

fn fixed(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = value.unsigned_abs();
    let whole = magnitude / 1024;
    let fraction = magnitude % 1024;
    if fraction == 0 {
        format!("{sign}{whole}")
    } else {
        let decimal = (fraction * 1_000_000) / 1024;
        format!("{sign}{whole}.{decimal:06}")
            .trim_end_matches('0')
            .to_owned()
    }
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use agel_core::{EvaluationOptions, World};

    fn desktop_vector_frame(scale: i64) -> Value {
        let mut world = World::default();
        agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
        world
            .evaluate(&format!(
                "(import agel/desktop) (import agel/ui-layout) (import agel/ui-vector)\n\
                 (compile-vector-frame\n\
                   (compile-frame (default-scene) default-viewport default-theme) {scale})"
            ))
            .unwrap()
            .values
            .pop()
            .unwrap()
    }

    #[test]
    fn renders_retina_desktop_deterministically() {
        let frame = desktop_vector_frame(2);
        let renderer = SvgRenderer::default();
        let left = renderer.render(&frame).unwrap();
        let right = renderer.render(&frame).unwrap();
        assert_eq!(left, right);
        assert!(left.contains("width=\"2560\" height=\"1600\""));
        assert!(left.contains("viewBox=\"0 0 1280 800\""));
        assert!(left.contains("<linearGradient"));
        assert!(left.contains("data-agel-id=\"agent\""));
        assert!(left.contains("Welcome to your moldable operating system"));
    }

    #[test]
    fn rejects_tampered_density_and_unbalanced_state() {
        let mut frame = desktop_vector_frame(2);
        let Value::Map(entries) = &mut frame else {
            panic!("frame map")
        };
        let physical = entries
            .iter_mut()
            .find(|(key, _)| key == &Value::Symbol("physical-width".into()))
            .unwrap();
        physical.1 = Value::Int(1);
        assert!(SvgRenderer::default()
            .render(&frame)
            .unwrap_err()
            .to_string()
            .contains("physical dimensions"));

        let mut frame = desktop_vector_frame(1);
        let Value::Map(entries) = &mut frame else {
            panic!("frame map")
        };
        let commands = entries
            .iter_mut()
            .find(|(key, _)| key == &Value::Symbol("commands".into()))
            .unwrap();
        let Value::List(commands) = &mut commands.1 else {
            panic!("commands list")
        };
        commands.push(Value::Map(vec![(
            Value::Symbol("op".into()),
            Value::Symbol("restore".into()),
        )]));
        assert!(SvgRenderer::default()
            .render(&frame)
            .unwrap_err()
            .to_string()
            .contains("unbalanced restore"));
    }

    #[test]
    fn escapes_agent_authored_text() {
        let mut frame = desktop_vector_frame(1);
        let Value::Map(entries) = &mut frame else {
            panic!("frame map")
        };
        let commands = entries
            .iter_mut()
            .find(|(key, _)| key == &Value::Symbol("commands".into()))
            .unwrap();
        let Value::List(commands) = &mut commands.1 else {
            panic!("commands list")
        };
        let text = commands
            .iter_mut()
            .find_map(|command| {
                let Value::Map(entries) = command else {
                    return None;
                };
                entries
                    .iter_mut()
                    .find(|(key, _)| key == &Value::Symbol("content".into()))
            })
            .unwrap();
        text.1 = Value::String("<agent & human>".into());
        let svg = SvgRenderer::default().render(&frame).unwrap();
        assert!(svg.contains("&lt;agent &amp; human&gt;"));
        assert!(!svg.contains("<agent & human>"));
    }

    #[test]
    fn repository_vector_examples_evaluate_and_render() {
        for source in [
            include_str!("../../../examples/vector-desktop.agel"),
            include_str!("../../../examples/vector-primitives.agel"),
            include_str!("../../../examples/kitchensink.agel"),
        ] {
            let mut world = World::default();
            agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
            let frame = world.evaluate(source).unwrap().values.pop().unwrap();
            let svg = SvgRenderer::default().render(&frame).unwrap();
            assert!(svg.starts_with("<svg"));
            assert!(svg.ends_with("</svg>\n"));
        }
    }
}

# Agel vector graphics

Agel v0.2.2 has a real resolution-independent graphics pipeline. The semantic
scene, layout, visual primitives, paints, vector compilation, validation, and
frame lifecycle are Agel standard-library code. Rust owns only a narrow output
surface: validate one immutable vector frame and serialize it as deterministic
SVG.

```text
Agel scene → Agel layout frame → Agel vector frame → checked SVG boundary
```

This boundary is intentional. The current interpreter has no byte arrays,
framebuffer mapping, SIMD, font rasterizer, or native compiler. Putting those
facilities into the evaluator would enlarge the trusted language core and still
be slow. Keeping a tiny renderer service gives useful high-performance output
now; the same transparent IR can later feed an unprivileged framebuffer service,
GPU command encoder, printer, remote display, or agent vision surface.

## The vector postcard

`agel/vector` uses logical integer coordinates and a 1024-unit fixed-point affine
matrix. A display's scale (1 through 8 in the current service) changes physical
resolution without changing layout, hit-testing, or application state.

```lisp
(fill-shape 'mark
  (path
    (list
      (move-to 10 80)
      (curve-to 40 0 120 0 150 80)
      (close-path)))
  (linear-gradient
    (point 0 0) (point 1024 1024)
    "#71e6ff" "#8b6cff"))
```

The public primitives are:

- geometry: point, rounded rectangle, ellipse, arbitrary path;
- path segments: move, line, cubic curve, close;
- paint: strict `#RRGGBB` solid colors and object-bounding-box gradients;
- state: nested fixed-point affine transforms and shape clips;
- drawing: fill, stroke, and scalable text; and
- frames: logical viewport, physical density, ordered commands, semantic actions.

`vector-spec` and `ui-vector-spec` expose the contracts as values for agents.
There is no special graphics syntax.

## Safety and performance contract

The Agel compiler rejects malformed vector values and unbalanced state stacks.
The output service does not trust that admission result: it validates again,
uses checked dimension arithmetic, caps scale and dimensions, bounds commands,
path segments, and output bytes, accepts only fixed color syntax, escapes all
agent-authored text and IDs, and contains no `unsafe` code. Failed rendering
cannot partially mutate an Agel world.

SVG preserves curves and text as vectors and maps a logical frame directly to a
requested high-density surface. Serialization is one bounded linear pass after
validation and requires no third-party crates. This is the reference backend,
not a claim that browser SVG will be the final compositor.

## Run it

```sh
cargo run -q -p agel-vector -- \
  --program examples/vector-desktop.agel \
  --output target/vector-desktop.svg
open target/vector-desktop.svg

cargo run -q -p agel-vector -- \
  --program examples/vector-primitives.agel \
  --output target/vector-primitives.svg
open target/vector-primitives.svg
```

Edit either `.agel` file and rerun the command. You can replace every visual
value—hierarchy, text, sizes, radii, colors, gradients, paths, clips, and
transforms—without recompiling Rust.

## Native continuation

The next graphical milestone is an unprivileged software framebuffer compositor
inside the bootable system, followed by pointer/keyboard routing to semantic
intents. It should consume this vector contract rather than moving UI meaning
into a device driver. Later acceleration can replace the raster backend without
changing applications or granting model agents direct device authority.

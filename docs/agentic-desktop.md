# The Agel agentic desktop

Agel v0.2.0 begins the desktop as a language-level object model, before pixels.
The retained scene, components, properties, semantic actions, proposed edits,
preview, and rollback history are ordinary Agel values. The implementation is
the `agel/ui` module in `crates/agel-stdlib/stdlib.agel`, not a privileged Rust
widget toolkit.

This is deliberately an honest first stratum. It is executable today in the
hosted runtime, but it is not yet a graphical compositor, renderer, window
server, pointer stack, or native QEMU desktop. A later renderer can project the
same scene to a framebuffer without taking ownership of application meaning.

Agel v0.2.1 adds the language-owned half of that projection.
`agel/ui-layout` compiles scenes into strictly validated, renderer-neutral
display frames, and `agel/desktop` supplies a COSMIC-inspired default shell.

Agel v0.2.2 completes the first hosted graphical path. `agel/vector` defines
resolution-independent geometry and paint, `agel/ui-vector` translates layout
frames, and the bounded `agel-vector` service produces real SVG output. The
meaningful layers remain Agel code; the native service is a replaceable output
boundary that validates the vector value again. This is graphical output, but
not yet the native QEMU framebuffer or window/input server.

Agel v0.2.3 adds that first native framebuffer boundary. A VBE handoff maps the
display only into a dedicated ring-3 compositor. Its input is a bounded vector
frame authored in Agel syntax, build-validated, and revalidated inside the
domain. Since v0.2.4, keyboard and serial input can commit and roll back bounded
native scene edits while QEMU runs. Pointer input and the full hosted agent
runtime are not yet native.

## Design

Every node is a persistent map with four keys:

```lisp
{kind window
 id workshop
 props {title "Agel"}
 children (...)}
```

Node identities must be unique across a scene. `text`, `button`, `row`,
`column`, `panel`, and `window` are conveniences over the generic `node`
constructor. They are functions in Agel and applications can replace or extend
them without changing the evaluator.

Actions are semantic intents rather than renderer callbacks:

```lisp
(intent 'workspace/save 'editor nil 'filesystem/write)
```

The intent says what should happen, to what, with which payload, and which
authority a future broker must supply. Inspecting or activating a widget cannot
mint that authority. This representation gives human and model agents the same
stable action graph, avoiding coordinate-only automation as the primary API.

## Live change protocol

Edits are persistent data too:

```lisp
(list
  (set-prop 'title 'content "A different world")
  (replace-node 'toolbar new-toolbar)
  (set-children 'workspace new-children))
```

`make-desktop` creates an agent with a typed protocol. Every proposal names the
revision it was derived from, so a delayed agent cannot overwrite newer work:

- `inspect` returns the scene, preview, prior scene, pending patches, and revision;
- `propose` rejects a stale base revision, otherwise applies patches to a
  candidate and validates the complete scene;
- `commit` atomically promotes a valid preview;
- `discard` abandons a preview; and
- `rollback` swaps the current and previous committed scenes.

The runtime transaction around every agent turn is the lower safety net. A
failed turn rolls back heap changes, messages, child creation, and provisional
events together. Expected bad edits are handled one level above it: malformed
patches, missing targets, invalid node shapes, and duplicate IDs produce a
`rejected` response while the desktop remains alive and unchanged.

## Why this resembles COSMIC without copying its implementation

[COSMIC Epoch](https://github.com/pop-os/cosmic-epoch) demonstrates the value
of composing a desktop from a session, compositor, panel, applets, launcher,
settings, notifications, and focused applications. Its
[libcosmic](https://github.com/pop-os/libcosmic) provides shared application and
applet conventions, while [cosmic-panel](https://github.com/pop-os/cosmic-panel)
shows configurable panels assembled from installable applets.

Agel adopts the compositional lesson, but moves the extensibility boundary into
the language image. Components are data owned by capability-scoped agents;
layout and theme will be replaceable libraries; actions remain semantic; and
changes pass through preview and promotion. The protected native foundation
will remain much smaller: input isolation, display-memory authority, scheduling,
renderer containment, and independent recovery.

## Try it

```sh
cargo run -q -p agel-cli < examples/agentic-desktop.agel
cargo run -q -p agel-cli < examples/cosmic-desktop.agel
cargo run -q -p agel-vector -- \
  --program examples/vector-desktop.agel \
  --output target/vector-desktop.svg
open target/vector-desktop.svg
./scripts/run-graphics.sh
```

The final `ui-spec` expression is the machine-readable schema from inside Agel.
Try changing the quoted patch list, adding a constructor written in Agel, or
sending `discard` instead of `commit`.

## Next strata

The next desktop work should preserve this separation:

1. Move the current supervisor-normalized keyboard adapter into a separate
   input domain and add pointer events without granting device authority to
   application agents.
2. Move more of the hosted vector scene interpreter into the native world and
   pointer events through semantic, capability-checked intents.
3. Split compositor, panel, launcher, notifications, settings, and applications
   into supervised agents.
4. Permit natural-language agents to author patches, but require the same
   preview, validation, authority, promotion, and recovery path as human edits.

No desktop agent will own both the ability to author a privileged change and the
ability to waive its admission policy.

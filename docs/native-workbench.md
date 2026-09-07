# Native agent workbench

v0.2.11 is a small, real native programming playground, not a browser simulation.
Start `./scripts/run-graphics.sh --workbench`. This uses a separate persistent
`target/boot/agel-workbench.img`, preserving your existing workshop disk. On its
first boot, in the **fresh empty** world enter:

```text
:workbench
```

This stages eleven named source cells (`wb-0` through `wb-10`) from
[`workbench.agel`](../boot/desktop/workbench.agel), then reconstructs them inside
the unprivileged native evaluator. It refuses to overwrite an existing world.
After `:save`, this separate workbench restores on subsequent launches. Do not
erase your existing disk to try the demo. Plain `./scripts/run-graphics.sh`
continues to use your original workshop. Add `--web` after `--workbench` if you
need the optional host-layout text bridge.

## Try it inside the OS

Click either colored dock icon. The first increments the agent's counter by one;
the second increments it by two. Its first activation changes the first icon's
color. Tab cycles focus; Enter with an empty command line activates the focused
item. The status line reports selection or resulting state. These operations
are ordinary Agel functions, not model calls. Static background buttons remain
illustrations, not application launchers. Clicks do not erase a partly typed
command. Finish it or press Escape first.

```lisp
(scene-owner selected)
(inspect-agent)
(agent-pending dock)
(agent-turns dock)
```

`:source 1` opens the dock agent's behavior body in the graphical inspector and
prints the same source on serial. Its parameters are `self state message` for
this example. Escape closes the inspector. `:show wb-3` shows the complete
definition; `:cells` lists editable cells. Native text retains the seed's
uppercase ASCII display; the source itself is unchanged. Common code operators
now have distinct glyphs rather than all appearing as hyphens.

## Change live behavior without risking the working version

Enter these lines separately. A replacement is a stored three-argument function.

```lisp
(def twice (fn (self state message) (begin (paint self (+ state (* message 2))) (+ state (* message 2)))))
```

```text
:preview (begin (agent-become dock twice) (activate))
:promote
```

Preview runs the test action in a separate candidate world and displays its
scene. The live agent, mailbox, scene and rollback point remain unchanged.
`:discard` restores the live scene; `:promote` adopts the already-tested candidate
without running it again. `:rollback` restores the preceding committed world,
including its behavior and state. Inspect source before previewing: ordinary
evaluation, including a state query, invalidates a pending candidate. Promotion
then fails closed and requires a fresh preview. Source/definition metadata
requests do not evaluate code.

A deliberate failure:

```lisp
(def broken (fn (self state message) (/ 1 0)))
```

```text
:preview (begin (agent-become dock broken) (activate))
```

The candidate is rejected because its test turn faults. The live agent still
works. Replacing behavior without testing a relevant message does **not** prove
that behavior correct. These are bounded executable checks, not formal proofs.

## Keep the change after reboot

Live state and editable source cells are deliberately separate. Update the
original behavior cell, then save:

```text
:cell wb-3 (def behavior (fn (self state message) (begin (paint self (+ state (* message 2))) (+ state (* message 2)))))
:save
```

Save/reboot reconstructs the world from source, including the new behavior.
The demo counter starts at its source-defined initial value; it is not a saved
heap snapshot. `:save` already replay-validates source before disk publication.
Use `:reload` to reconstruct the last saved source workspace.

## Small primitives, library policy

- `(scene-bind identity owner)` binds the most recently appended rectangle to
  an existing agent. Identities must be positive and unique in the current scene.
- `(scene-hit x y)` returns the topmost shape's identity, or zero for an unbound
  shape/background. Rounded corners match the compositor. Unbound foreground
  shapes occlude rather than forwarding clicks through themselves.
- `(scene-owner identity)` returns that shape's agent.
- `(agent-become agent behavior)` replaces behavior at an operator boundary,
  retaining scalar state, queued messages and turn count. Behaviors cannot call
  it from inside an agent turn. It does not supply general state migration.

Identity is explicit, not a framebuffer address or drawing-array index. The
library uses stable identities 1 and 2 across repaints. `point`, `focus`,
`focus-next`, `activate`, and `inspect-agent` are ordinary Agel definitions.
The fixed source panel and hardware adapter remain Rust bootstrap scaffolding;
this is not yet a fully Agel-authored editor/widget toolkit.

Native PS/2 input is decoded with bounded packet storage, signed movement,
overflow rejection and edge-triggered clicks. QEMU still controls mouse capture
and its release shortcut; this does not add an absolute USB tablet driver.
The optional `--web` bridge still supports host-layout text, not pointer input.

## Limits and assurance

The existing native limits remain: eight agents, eight messages per mailbox,
12 rectangles, 16 persisted cells, 256 source bytes per command, bounded parser,
call depth and fuel. Drawing remains CPU-rasterized vector geometry with a seed
font, not GPU-accelerated typography. Full-frame redraw is currently used for
pointer/inspector updates; dirty-region rendering remains performance work.

Native agents still share globals and an evaluator protection domain. This is
not mutually untrusted per-agent isolation. Preview cannot make external effects
reversible; the native evaluator currently has no model/network/file effect
dispatch. No AI provider, microphone, or paid inference is used by this milestone.
Language commits are atomic; framebuffer projection is not a hardware-atomic
swap. The operator's recovery path and kernel authority are not agent-rewritable.

Run `sh scripts/test-native-agents.sh`, then build the native graphics image and
run `python3 scripts/test-native-workbench.py target/boot/agel-v1.img`. The latter
uses real QEMU mouse/key events, framebuffer assertions, candidate rejection,
promotion, rollback and a reboot into updated source. It writes
`target/native-workbench.png` from the actual guest display.

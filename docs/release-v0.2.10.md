# Agel v0.2.10 — Live native scenes authored in Agel

Ordinary Agel functions and native actors can now draw a bounded vector overlay
in the running graphical OS. Three small primitives support an Agel-written
dock library, transactional repaint, rejected-change recovery, and source
replay after reboot. QEMU's own graphical window is the default again;
`--web` explicitly selects the optional keyboard-layout bridge.

Try `examples/native-dock.txt` from `./scripts/run-graphics.sh`. It builds the
dock, saves its source, changes it through an actor message, and demonstrates
rollback without rebuilding or rebooting.

Validation includes real framebuffer comparisons for actor changes, rollback,
invalid geometry, clearing, and reboot replay, plus bounds/capacity and failed
turn tests. A captured frame is generated at `target/native-dock.png`.

This is a visual dock prototype, not yet a clickable launcher. Voice capture,
speech recognition, model-generated native patch promotion, SVG import, and
animation remain future work, documented in `docs/native-scenes.md`.

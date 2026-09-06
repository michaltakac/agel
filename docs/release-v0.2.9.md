# Agel v0.2.9 — Layout-aware graphical input

The old graphical decoder omitted most punctuation and used a partial US
physical layout. It could not inherit macOS Slovak text composition, and QEMU's
native window grabbed the pointer.

`./scripts/run-graphics.sh` now shows the real guest framebuffer in a local
browser console. Its text field uses the host input system, including Slovak
dead keys, Option symbols, and paste. Enter submits the exact UTF-8 line to
Agel's native evaluator. Clicking the frame focuses the input without capture.

The direct QEMU window is retained as `./scripts/run-graphics.sh --native`.
Its PS/2 decoder now covers US ASCII punctuation, uppercase and Caps Lock,
independent Shift and Ctrl keys, and Ctrl-U/C/H editing. This mode still uses
the guest's US physical layout.

Unicode source is preserved by the guest, with UTF-8 validation at submission
and codepoint-aware deletion. The seed framebuffer font still substitutes
question marks for unsupported bytes; the host input and transcript retain
the original characters. The browser console is a host bridge, not a browser
implemented inside Agel.

Verified with real QEMU Unicode/punctuation round trips, byte/control input
limits, physical-key quote/eval and shifted-symbol tests, and a browser session
evaluating `(def výsledok 42)`.

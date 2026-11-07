# v0.3.72 · Wait for the server to enable dialogue input

The native Cook trace showed the guest sending Continue after the window
opened and before it read the packet enabling that component. The
RuneScape example now requires a new window and a new pause-button event
for the matching component before sending a response.

The protocol fixture decodes the captured event and ignores an unrelated
mask. The quest fixture presents the window before the input event and
checks that the client waits for the event.

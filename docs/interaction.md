# Interaction is a protocol, not a privilege

Agel treats typing and speaking as two adapters onto the same input protocol.
The `agel-interaction` library deliberately knows nothing about microphones,
speech models, terminal widgets, Claude, or Codex. It classifies an input by:

- modality: `Text` or `VoiceTranscript`;
- intent: `Observe`, `Propose`, or `Authorize`; and
- an opaque presence proof minted by the trusted host after authentication.

An accepted input atomically creates a foreground acknowledgement and a
background task. The acknowledgement advertises a 200 ms response contract;
it does not pretend the background work finished in 200 ms. Model reasoning,
tool use, and agent orchestration consume the separately scheduled background
queue, then publish a completion back to the foreground lane. Both lanes are
bounded and return explicit backpressure.

`Authorize` is distinct from natural-language meaning. A transcript saying
“promote the kernel” is not authority. Both text and voice authorization require
a `PresenceProof` whose process-local seal matches the `PresenceAuthority`
bound to that interaction hub. Untrusted agents can construct inputs and
proposals but cannot manufacture a matching proof merely by making their output
resemble a human command.

Run the complete example:

```sh
cargo run -q -p agel-interaction --example two_lane
```

Future terminal, GUI, and audio adapters should depend on this library. They
must keep authentication evidence out of model-controlled text and preserve the
foreground/background separation under cancellation and overload.

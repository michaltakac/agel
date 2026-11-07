# v0.3.51 · Keep game code in examples

The RuneScape protocol, client, and pinned ISAAC fixture now live under
`examples/osrs`. Hosted and native runners explicitly load the example
modules. The reusable crypto, byte streams, services, replay, and judging
stay in Agel's general libraries.

The legacy DOOM desktop programs live under `examples/doom`. The default
desktop image excludes their data entries. Use
`./scripts/run-graphics.sh --doom-demo` or build with
`./scripts/build-boot.sh --features doom-demo` to include the legacy integration.

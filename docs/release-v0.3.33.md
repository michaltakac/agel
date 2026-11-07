# v0.3.33 · Standard research tools

`agel/live-tools` is installed with the standard library. Its catalog
includes `evidence-ingest`, `web-search` and `web-read`. Ingestion takes
HTML, plain text or transcript text and attaches its source locator.
HTML uses the Agel browser's document parser without changing the active
browser page. Search and read take provider functions through
`live-tools-with-research`. The catalog shows whether those providers
are bound.

`live-tools-context` gives a caller the agent-facing tool instructions and
inspectable catalog. For a custom tool, inspection returns its active Agel
source and version. An accepted edit takes effect in the same actor after
judgment and replay. The call history retains the evidence used by the test.

The integration test uses fixture search and fetch providers. It discovers a
guide, reads it, records transcript text, activates a custom Agel tool against
the recorded document, and calls it on the same actor. Run it with
`cargo test -p agel-stdlib --test live_tools`.

Live internet access, browser navigation from the tool actor and video
retrieval still need provider bindings. Transcript ingestion accepts
text supplied by the caller. The fixture test measures tool composition and
activation and has no external provider charges.

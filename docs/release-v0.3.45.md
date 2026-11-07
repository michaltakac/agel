# v0.3.45 · Read an HTTPS source inside Agel OS

`agel/research` can bind its `web-read` tool to an Agel byte stream.
The loopback relay fetches HTTPS documents from explicitly allowed
hostnames. It accepts UTF-8 HTML and plain text within a 32 KiB
response limit. Agel parses the returned document and retains its URL
as external evidence.

Start the relay with `python3 scripts/research-relay.py --host example.com`
and point the byte carrier at the loopback port it prints. In Agel,
bind the fetch function with `(research-stream-fetch (byte-stream byte-request))`.
Pass that function to `research-tools` or `live-tools-with-research`.

A QEMU guest read Example Domain and refused a hostname outside the
allowlist. The relay provides HTTPS transport for document reads.
Search and automatic video retrieval still need providers.

[Native research run](evidence/v0.3.45-native-research.txt)

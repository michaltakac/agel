# v0.3.47 · Live search through a configured endpoint

The standard `web-search` tool can use a live HTTPS query endpoint.
`research-query-provider` builds the URL in Agel, percent-encodes the
query, and records the response URL and media type as external evidence.
Providers can return HTML, plain text, or JSON. Agel keeps the JSON body
as text for a provider-specific consumer.

For example, start the relay with
`python3 scripts/research-relay.py --host en.wikipedia.org` and point the
guest's byte carrier at its printed loopback port. Load `bytes.agel` and
`byte-stream.agel` from `crates/agel-stdlib` before configuring the
[MediaWiki OpenSearch endpoint](https://www.mediawiki.org/wiki/API:Opensearch)
in Agel.

```lisp
(import agel/byte-stream)
(import agel/research)
(def fetch (research-stream-fetch (byte-stream byte-request)))
(def search (research-query-provider fetch
  "https://en.wikipedia.org/w/api.php?action=opensearch&limit=5&format=json&search="
  ""))
(def tools (research-tools search fetch))
((get (get tools 'web-search) 'invoke) "metacircular evaluator")
```

The same two providers can be passed to `live-tools-with-research`.
This example searches Wikipedia article titles. Each deployment supplies
its own endpoint and allowed hosts.

A 1 GiB QEMU guest ran the example and received the article title and URL.
[Native search trace](evidence/v0.3.47-native-search.txt)

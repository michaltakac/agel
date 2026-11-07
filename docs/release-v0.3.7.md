# v0.3.7 — Private OSRS world and Agel wire client

The first Agel OSRS client talks to a live private revision-241 world.
JS5 negotiation, rejected revisions, unsigned eight-byte game-login
challenges, cache-group requests and continuation-block parsing are Agel
source. The Rust carrier installs that source and provides bounded reads
and writes over a loopback TCP stream; it contains no OSRS packet logic.

This is the start of the game client, not a completed game workload.
Player authentication, packet encryption, game observations/actions and
native OS networking remain unfinished. DOOM has not yet been replaced.

## Private server and baseline

`scripts/osrs/prepare.py` installs and packages OpenRune commit
`c4cd45ef2d38a0badcc7a9f89773f8f1c29bb16f` into a Linux ARM64 image.
The small source patch makes its game listener configurable. The JRE,
additional ARM64 PostgreSQL binary and base image are pinned. Generated
cache files, local RSA keys and compiled artifacts remain under `target/`;
the private image is not a published release asset.

Only `127.0.0.1:43594` is published. The bundled account service hardcodes
wildcard HTTP/world-link listeners, so those remain inside the container.
The database persists in a named volume. Stop/resume does not clear it.

The stock server uses 150× base XP, bypasses quest requirements and grants
every account admin in development mode. The supplied setup uses saved
quest progress. The explicit baseline command sets realm 255 to 1× base
and global XP and disables automatic admin grants. Those loaded values
were verified after a real server restart. This is not a certification of
every mechanic, experience modifier or quest's parity with OSRS.

## Validation

- `sh scripts/test-osrs-wire.sh`: fragmented TCP, full unsigned challenge,
  outdated revision, early EOF, multi-block cache payload, invalid marker
  and wrong cache-group identity.
- `sh scripts/test-osrs-wire.sh --live`: the same fixtures plus successful
  revision negotiation, expected revision rejection, game-login challenge
  and a 200-byte cache master-index response from the actual private world.
- Full server install and distribution build, then container startup,
  database baseline update, stop/resume and loaded-configuration inspection.
- Carrier Clippy and formatting; Python syntax checks. Core/kernel source
  and the seed ABI are unchanged by this release.

The [live wire results](evidence/v0.3.7-osrs-wire.txt) and
[world manifest](evidence/v0.3.7-osrs-world.json) identify the tested
image, server commit, cache fingerprint, loopback mapping and loaded rates.
The first ARM container boot failed because the default server distribution
omits ARM PostgreSQL binaries; the image now adds the matching 14.22.0
artifact. A fixture also exposed recursive list appending at packet sizes
above the evaluator's call-depth bound; the Agel reader now accumulates
bytes in reverse and handles continuation blocks without that recursion.

The protocol source was checked against RSProt's pinned
`osrs-241-api` and `osrs-241-shared` artifacts,
`1.0.0-ALPHA-20261001`, rather than assuming older handshake formats.
See [RSProt](https://github.com/blurite/rsprot) and
[the setup guide](osrs-client.md) for usage and the precise boundary.

## Remaining work

Cache compression is identified but not decoded or CRC-verified. Cache
groups have a 65,532-byte compressed-size bound. The client runs in the
hosted Agel runtime; native byte transport is still needed. Admin skill/item
commands have not been exercised through this client, and no account has
played combat, crafting or quests. There are no scores, coins-per-dollar
claims, transfer results or Exo comparisons in this release.

Guardian-only logging, complete interrupted-call cost reconciliation,
durable whole-program activation and actual OS service version/revert
handling also remain open in the [release programme](exo-programme.md).

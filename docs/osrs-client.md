# Private OSRS world and Agel client

This is the first wire-protocol stage of replacing the DOOM workload. It
does not yet log in a player, render a world or execute a game action.
The client protocol is Agel source in `crates/agel-stdlib/osrs-wire.agel`.
The hosted `osrs` example gives it two bounded byte-stream operations over
a loopback TCP connection. Packet layout, exact reads, revision handling,
challenge handling, cache-group requests and continuation markers are Agel.
The module is loaded explicitly by the example; it is not installed in
every default Agel world.

## Build the world

The supplied setup targets Linux ARM64 containers, tested from macOS ARM64.
It needs Python 3.12 or newer, Docker, Git, a Java host for Gradle, and the
Rust toolchain. Gradle uses its managed Java 21 toolchain; on this machine
the launcher uses Homebrew Java 17. Downloads, generated cache data, local
RSA keys, compiled dependencies and container build context stay in `target/`.
They are not release artifacts and must not be published as an image.

```sh
python3 scripts/osrs/prepare.py
sh scripts/osrs/world.sh start
sh scripts/osrs/world.sh logs
```

The checkout is pinned to OpenRune commit
`c4cd45ef2d38a0badcc7a9f89773f8f1c29bb16f`, with the small listener patch in
`scripts/osrs/openrune.patch`. The JRE and additional ARM64 PostgreSQL binary
have pinned checksums. OpenRune supplies its own generated revision-241 cache.
Its content coverage still needs mechanic-by-mechanic verification; a
successful boot is not evidence of complete OSRS parity.

Only game port `127.0.0.1:43594` is published. The bundled account service's
HTTP and world-link listeners remain inside the container. The database
lives in the named `agel-osrs-postgres` volume. `stop` and `resume` preserve
that volume and container; `start` refuses an existing container name.
No command here deletes a database volume.

Before creating experimental accounts, wait for `Server ready`, then apply
the baseline and restart:

```sh
sh scripts/osrs/baseline.sh
sh scripts/osrs/world.sh stop
sh scripts/osrs/world.sh resume
sh scripts/osrs/world.sh logs
```

The baseline command changes only this container's realm 255: base and
global XP become 1×, and development mode stops automatically granting
admin to every login. Verify the new `RealmConfig` startup line before any
scored work. The generated `game.yml` uses `respect-progress` rather than
the stock quest-requirement bypass. This does not certify every quest's
implementation or every experience modifier. Other rates and account
settings still need explicit experiment manifests.

`python3 scripts/osrs/inspect.py` verifies the running container's published
ports, quest setting and latest loaded baseline, then records its image,
dependency pins and cache-tree fingerprint. Keep this output with a run.

## Exercise the client

```sh
sh scripts/test-osrs-wire.sh
sh scripts/test-osrs-wire.sh --live
cargo run -q -p agel-stdlib --example osrs -- 43594 examples/osrs/master-index.agel
```

The fixtures check fragmented TCP, a full unsigned eight-byte challenge,
outdated-revision refusal, early EOF, cache response identity, and JS5
continuation markers. `--live` performs revision negotiation, a deliberately
rejected revision, the pre-login challenge and a master-index cache request
against the actual local server. It fails if that server is unavailable.
The challenge remains eight bytes, avoiding signed-64-bit overflow.
Cache groups are bounded to 65,532 compressed bytes and returned as bytes;
compression is identified but not decompressed or CRC-verified yet.

The hosted byte carrier is not native OS networking. Player authentication,
RSA/XTEA/ISAAC, game packet codecs, observations, actions, admin skill/item
setup and native transport remain to be implemented and tested. Existing
OpenRune `master`, `reset` and `item` commands are source-inspected, not yet
verified through this client. No task score or Exo advantage follows from
these wire checks.

# Agel runtime evolution and OSRS

Goal: replace the DOOM workload with a private OSRS environment, make Agel
policy and eventually its OS services self-editable in Agel, and measure
whether specialization beats an identical fixed policy. The reference is
[Exo v1, sections 3–5](https://arxiv.org/html/2611.00463v1).

## Release sequence

Each step needs executable evidence and a release note naming its limits.
Version labels below are planned until a corresponding tag exists.

| Release | Work | Required evidence |
| --- | --- | --- |
| v0.3.3 | Agel version/diff/replay policy | Same Lisp contract hosted and in an OS process; failed candidates preserve active versions |
| v0.3.4 | Protected log storage and Agel wrapper | Before/after dispatch ordering; OS overwrite/truncate/unlink denial and normal reboot; opt-in until entry-point integration |
| v0.3.5 | Provider-wide attempt accounting | All existing Jev HTTP paths append Agel records; native requests wait for guest storage; pricing receipts, retries, resume and storage failure tests |
| v0.3.6 | Recoverable and larger native storage | Agel framing, interrupted-append recovery, capacity reservation, resume beyond 64 KiB and syscall exclusion tests |
| v0.3.7 | Private world and Agel wire client | Container-isolated OpenRune, verified baseline rates and pre-login/cache protocol against the live world |
| v0.3.8 | Working RuneScape smoke tests | Agel password login, ciphers, movement, live stats/inventory observations, skill/item setup, normal-account isolation and persisted server state |
| v0.3.9 | Observed-target combat | Agel NPC bit decoding, tracked indices, attack and rat removal; exactly 8 Attack XP plus Hitpoints XP saved on a fresh normal account |
| Next: guardian and canonical history | Protected append authority and actual activation | Missing-result reconciliation; whole-program isolated replay; durable commit/recovery |
| Following: private server and Agel client | Pinned server with required content, login, observe/action protocol in Agel | Byte relay only outside Agel; tested skill/item setup commands |
| Following: agents and experiment runner | Prompts, context selection, tools, learned code and reports in Agel | Frozen control and repeated paired tasks |
| Following: OS service evolution | Actual programs, domain generations and kernel slots | Isolated replay, version retention, watchdog recovery and reboot evidence |
| Following: comparative results | Standalone, transfer and quests | Correctness, all-in cost, active/wall time, seeds and raw logs published |

The existing native Rust evaluator, scheduler, filesystem, drivers and desktop
are still substantial implementations. Migrate their policy upward instead
of adding new task or learning logic in Rust. The protected bootstrap must
retain the authority to enforce append-only storage, isolate candidate code
and recover a failed kernel. Being written in Agel does not confer authority
to bypass that mechanism.

## Experiment contract

The paper uses an emulated RuneScape environment via RS-SDK. Its finite
objectives are maximum combat stats and directly crafting an adamant
platebody; money-making is evaluated near eight active hours. Transfer uses
new accounts while retaining agent state. Quests are a separate workload
where the paper reports specialization costing more. Preserve this negative
case in reporting.

Record server commit, cache revision, content coverage, rates, population,
model/reasoning setting, token prices and caching rules. RSMod targets
[mechanical accuracy](https://github.com/rsmod/rsmod/blob/main/README.md),
but that is not evidence of complete quest/content parity. Inspect and test
the required mechanics before treating a task as supported. A missing quest
is unsupported, never a successful zero-cost run. Pin server and client
dependencies outside the Agel source tree; do not commit downloaded caches.

Both experiment arms start with identical code, model, budget, observations,
action access, memory access and account setup. The control cannot activate
policy edits. Randomize paired run order and use multiple recorded seeds.
Keep setup/admin commands out of gameplay authority: training stats or
granting the finished item during a scored run invalidates it. Verify crafting
provenance, combat XP and quest completion from authoritative server state.

Report completion and correctness before efficiency. Include failed attempts,
judges, retries, reflection and specialization in total cost. Report held-out
cost separately. Unknown billing invalidates coins-per-dollar claims; it is
not zero. Transfer resets account state and records retained code/history
versions. Freeze all control policy hashes before experiments.

Paper reference values are context, not a controlled comparison: standalone
combat $3.71, crafting $2.99, money-making approximately 332k coins/model
dollar; transfer combat $1.92 and crafting $0.88. A different server cannot
establish a direct win from these numbers alone. Publish Agel/control paired
results and distinguish them from a reproduction with a matched Exo arm.

## Server inspection

RSMod commit `fa13b3f67172ef36e15d6b1514358aee61411796` was cloned to
`target/external/rsmod`; `./gradlew install --console=plain` succeeded with
Gradle's managed Java 21 toolchain. This downloaded the cache and generated
local RSA keys, all outside version control. The server has not yet been
started or exercised by an Agel client.

This checkout has cooking, fishing, magic and woodcutting skill modules, but
no smithing or quest modules. It cannot yet provide the required adamant
platebody and quest workloads. Its admin commands include `master`, `reset`
and `invadd`; per-skill setup still needs implementation and verification.
Before choosing this as the benchmark server, either supply the missing
faithful content or find a better-covered private OSRS implementation.

A more complete candidate is now checked out at `target/external/openrune`:
[OpenRune-Server](https://github.com/OpenRune/OpenRune-Server), commit
`c4cd45ef2d38a0badcc7a9f89773f8f1c29bb16f`. Source inspection found mining,
smithing (including platebody recipe handling) and quest implementations
such as Doric's Quest, The Knight's Sword and Gertrude's Cat. Its generated
progress report explicitly says presence does not prove wiki parity.
`./gradlew install --no-configuration-cache --console=plain` succeeded.
The default install task completed its work but failed Gradle configuration
cache serialization, so this explicit flag is required for this checkout.
The full `:server:app:classes` build also passed. The local checkout is
configured to bind the game listener to `127.0.0.1` and use
`quest-requirements.mode: respect-progress`; the stock quest bypass must
not enter scored experiments. These local changes still need live checks
and a reproducible server setup release.
v0.3.7 boots this world inside a Linux ARM64 container. Only
`127.0.0.1:43594` is published; the embedded account service remains inside
the container. A database setup command changes realm 255 to 1× base/global
XP and disables the stock development mode, which grants every player admin
on login. The loaded configuration was verified after restart. The server
cache fingerprint and image identity are recorded in the release evidence.

The Agel wire client has passed live revision negotiation, revision refusal,
the eight-byte login challenge and JS5 master-index retrieval. Its Rust host
only supplies bounded loopback byte reads/writes. This is hosted transport;
v0.3.8 adds password login, RSA/XTEA/ISAAC, game frames and stat/inventory
observations in Agel. Live tests move a normal account, reject its admin
commands, and use a separate setup account to set Smithing from 1 to 70 and
inventory coins from 0 to 100. The tests inspect live game packets and verify
persisted server rows. This is a smoke test, not the paper's task evaluation.
The private profile disables login proof-of-work for both experiment arms;
password authentication and game mechanics remain active. This profile
must remain explicit in comparison manifests. Native OS networking remains
unfinished. v0.3.9 adds a rat-combat smoke test: a fresh normal account chooses
an NPC from decoded observations, attacks, observes removal and verifies
exactly 8 Attack XP plus Hitpoints XP in saved state. NPC extended masks,
full spatial observations and high-level combat training remain unfinished.
Crafting, quests and the actual comparative workloads are not yet tested.
See [the client setup](osrs-client.md).

## Current boundary

v0.3.3 adds policy machinery, not a replacement OS or game result. Versioned
`domain` and `kernel-slot` entries are namespaced source records; they do not
change page tables or boot slots. Its functional history can be replaced by
its caller. v0.3.4 adds protected OS bytes and an opt-in Agel judge wrapper.
v0.3.5 connects existing Jev provider calls to Agel accounting and guest
storage where a guest exists. v0.3.6 adds a larger recoverable append store
and result-space reservation. Complete accounting for paid calls interrupted
by a crash/device failure and guardian-only append authority remain open.
The existing desktop gate and DOOM demo continue until their replacements
pass the later evidence gates.

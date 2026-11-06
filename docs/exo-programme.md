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
| v0.3.4 | Protected durable history | Every judge attempt logged before dispatch and after response; tokens, price basis, cost or explicitly unknown; reboot and truncation tests |
| v0.3.5 | Native replacement guardian | Whole-program admission; replay in an isolated domain; commit only after durable evidence; failure recovery and bounded retries |
| v0.3.6 | Private server and Agel client | Pinned RSMod revision/cache, login, observe/action protocol in Agel; byte relay only outside Agel; tested skill/item setup commands |
| v0.3.7 | Agel agents and experiment runner | Prompts, context selection, tools, learned code and reports in Agel; frozen control and repeated paired tasks |
| v0.3.8 | OS service evolution | Versioned actual programs, domain generations and kernel slots; isolated replay, watchdog recovery and reboot evidence |
| v0.3.9 | Comparative results | Standalone, transfer and quests; correctness, all-in cost, active/wall time, seeds and raw logs published |

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

## Current boundary

v0.3.3 adds policy machinery, not a replacement OS or game result. Versioned
`domain` and `kernel-slot` entries are namespaced source records; they do not
change page tables or boot slots. Its functional history can be replaced by
its caller and is not protected disk storage. The existing desktop gate and
DOOM demo continue until their replacements pass the later evidence gates.

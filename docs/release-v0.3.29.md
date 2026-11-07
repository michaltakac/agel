# v0.3.29 — Score paired task runs in Agel

The Agel benchmark module scores correctness, elapsed time, and model
cost from run records. It compares a fixed and an adaptive arm only when
they provide the same comparison identity and distinct accounts. It also
checks that a transfer record identifies a fresh account and retains the
recorded policy and history.

An unpriced dispatch makes the all-in cost and cost difference unknown.
Known judge costs stay visible. For a successful economy run with a
positive, fully priced cost, the scorer keeps net coins and cost in
nanodollars as an exact ratio. A zero-cost or incompletely priced run
has no coins-per-dollar score.

The recorded native one-rat pair from v0.3.20 exercises the scorer. Both
arms passed the saved-XP check. The adaptive arm took 126.373 seconds
longer on the task clock and recorded a judge charge of $0.000032130.
The writer had no dollar receipt, so the total cost difference is unknown.
This is one historical pair with uncontrolled combat randomness. See the
[fixed trace](evidence/v0.3.20-fixed-combat.txt) and
[adaptive trace](evidence/v0.3.20-adaptive-combat.txt).

```sh
cat crates/agel-stdlib/benchmark.agel examples/benchmarks/rat-combat-v0.3.20.agel | cargo run -q -p agel-cli
```

A fresh normal account also completed the hosted Cook's Assistant route
without a reconnect or admin setup. The server verified all three hand-ins,
one quest point, and 300 Cooking XP. The successful west-side run took
554.394 seconds. A later run with an explicit mill-exit step returned with
all three ingredients and timed out before the first cook dialogue. The
fixed route has a passing sample and a remaining hand-in failure.

The full Exo workloads, a live transfer comparison, and an adaptive quest
pair remain open. See the [native scorer and quest traces](evidence/v0.3.29-benchmark-and-quest.txt).

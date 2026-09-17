<!-- Research brief drafted by a Claude research agent (Sonnet) on 2026-09-17 from cited public sources, reviewed and acted on in docs/research/decisions-2026-09.md. It is background, not a claim about Agel's code; where it describes Agel it describes the state at v0.2.86. -->

# What Smalltalks and Lisps did, and what Agel should learn

Agel already runs a compiler frontend and an integer x86-64 backend *inside*
the guest OS, while the evaluator and most of the runtime remain Rust
(`crates/agel-core`, `crates/agel-stdlib/stdlib.agel` for the metacircular
subset). That is exactly the point at which Smalltalk and Lisp history has
the most to say: it is the "how do we get the last mile of self-hosting
without building an over-clever VM" problem, and it has been solved — and
mis-solved — many times.

## Smalltalk-80: the Blue Book VM and Deutsch–Schiffman

The canonical Smalltalk-80 VM (the "Blue Book", Goldberg & Robson 1983) is a
bytecode interpreter over an object memory in which **everything is an
object**, including activation records ("contexts") and even small integers
(SmallIntegers were tagged to avoid allocating them). Method activation used
first-class `MethodContext`/`BlockContext` objects rather than a native call
stack, which made the debugger trivial (a context is just an object you can
inspect, save, resume, or hand to another process) but made calls expensive:
every send allocated a context object and reference-counted it.

L. Peter Deutsch and Allan Schiffman's 1984 POPL paper, "Efficient
Implementation of the Smalltalk-80 System," is the paper that made
Smalltalk (and later every dynamic language) fast. Two ideas from it are
foundational:

1. **Dynamic translation** — compile bytecode to native code lazily, the
   first time a method executes, and cache the translation keyed on the
   method (what we now call a JIT). This is explicitly the ancestor of every
   later VM's JIT, including the JVM's.
2. **Inline caching** — at a send site, cache the method actually found by
   the last lookup and its receiver's class; on the next send, if the
   receiver is the same class, skip the full lookup. They observed what they
   called "dynamic locality of type usage": a given call site overwhelmingly
   sees the same receiver type across invocations. This single empirical
   observation underlies inline caches, polymorphic inline caches, hidden
   classes, and modern JS engines' "shapes."
3. **The "contexts" trick** — because Smalltalk contexts are supposed to be
   ordinary heap objects, but a real machine needs a fast native call stack,
   Deutsch–Schiffman's design keeps contexts *lazily materialized*: a call
   normally lives on a real stack, and only gets promoted to a full
   heap-allocated context object if something (the debugger, a
   continuation-like operation, block closure capture) actually needs to
   see it as an object. This "looks like an object if you ask, is a stack
   frame if you don't" duality reappears in Cog/Spur (below) and is close to
   how Agel should think about call frames if it ever needs first-class
   activation records without paying context-allocation cost on every call.

Source: Deutsch & Schiffman, "Efficient Implementation of the Smalltalk-80
System," POPL 1984 — https://dl.acm.org/doi/10.1145/800017.800542 (see also
the Rice University lecture notes summarizing it,
https://www.clear.rice.edu/comp512/Lectures/22ST80.pdf).

## Self: maps, polymorphic inline caches, adaptive optimization, Klein

Self (Ungar, Smith, and later Hölzle/Chambers at Sun/Stanford) pushed
Smalltalk's "everything is an object, sends are the only operation" idea
to prototypes with no classes at all — object = a bag of slots, method
dispatch by slot lookup up a parent chain. Doing that fast without classes
to hang inline caches or layout off required real innovation:

- **Maps** — Self objects that share the same slot names/layout share a
  single "map" object describing that layout (name → offset), instead of
  storing per-instance metadata. This is precisely what "hidden classes" /
  "shapes" in V8, SpiderMonkey, and PyPy borrowed later; it decouples
  "what shape is this object" from "what class is it nominally," which
  matters for a Lisp-like system with plists/maps as first-class values
  (Agel's `map` type) where reflection should not force a slow path.
- **Polymorphic inline caches (PICs)** — Hölzle, Chambers & Ungar, ECOOP
  1991, extend a monomorphic inline cache to a small table of
  (type → method) pairs per call site, so call sites that see 2–4 shapes
  (very common in real programs) still get inline-cache speed instead of
  falling back to a full lookup. https://research.google/pubs/optimizing-dynamically-typed-object-oriented-languages-with-polymorphic-inline-caches/,
  Springer record: https://link.springer.com/chapter/10.1007/BFb0057013.
- **Adaptive optimization / deoptimization** — Self's insight (Hölzle's 1994
  Stanford PhD thesis, "Adaptive Optimization for SELF: Reconciling High
  Performance with Exploratory Programming," http://i.stanford.edu/pub/cstr/reports/cs/tr/94/1520/CS-TR-94-1520.pdf)
  is that you cannot afford to fully optimize everything up front in an
  interactive, live-editable system — so start with a cheap non-optimizing
  compile, count hot call sites at runtime, and only invest heavy
  optimization (inlining across polymorphic sends, speculative type
  assumptions) into the parts that are actually hot. The matching
  requirement is **deoptimization**: when a speculative assumption breaks
  (a new receiver type shows up, or the user attaches a debugger), you must
  be able to reconstruct an unoptimized-equivalent stack frame from an
  optimized one and resume — "Debugging Optimized Code with Dynamic
  Deoptimization," Hölzle/Chambers/Ungar, PLDI 1992,
  https://dl.acm.org/doi/10.1145/143103.143114. This pairing —
  cheap-baseline + hot-path-only optimizer + guaranteed deopt path — is the
  single most important lesson for any live, transactional system that also
  wants native-code speed, because it is the only known way to reconcile
  "you can always drop into a debugger/inspector on any live frame" with
  "hot code runs at native speed."
- **The Self VM philosophy**: "simple runtime, smart compiler." The
  interpreter/runtime object model is meant to be small and predictable;
  essentially all cleverness lives in the (replaceable, restartable)
  optimizing compiler, not smeared through the runtime's object
  representation. This is a direct argument for keeping Agel's evaluator's
  *object and world model* boring and moving cleverness into a
  separately-versioned optimizing pass.
- **Klein**, the Self VM written in Self itself (a metacircular VM), is the
  extreme end of this: "Constructing a Metacircular Virtual Machine in an
  Exploratory Programming Environment," Ungar & Spitz, OOPSLA companion
  2005, https://dl.acm.org/doi/10.1145/1094855.1094865. The finding that
  matters for Agel: because Klein is metacircular, the *same code* that
  builds objects in the bootstrap image also builds objects at VM runtime
  and implements a remote debugger — one implementation serves three roles.
  That is the same shape as Agel's "the compiler and reader running in the
  guest" milestone (`docs/roadmap.md`): once code written in the target
  language can build/inspect the representations the runtime itself uses,
  you get debugger/compiler/bootstrapper reuse for free instead of writing
  each one three times in three languages.

## Strongtalk

Strongtalk (Bracha & Griswold, 1993 paper, https://en.wikipedia.org/wiki/Strongtalk)
added an optional, sound static type system on top of Smalltalk semantics
without breaking dynamic-typing escape hatches, and its VM (built by the
Animorphic team, later bought by Sun) is the direct ancestor of the Java
HotSpot VM — the "mixed-mode interpreter + adaptive optimizing compiler +
deoptimization" architecture in HotSpot came from people who built it first
for Strongtalk/Self. The lesson: **the JIT architecture is language-agnostic
and portable across a full type-system rewrite** — Strongtalk kept the
Self-style VM machinery and grafted a type checker in front of it. For
Agel, this suggests the native backend and any future JIT/AOT pipeline
should be decoupled from whatever gradual/optional type discipline gets
added to the frontend later; the backend's contract should be "here is IR,"
not "here is dynamically-typed Agel source."

## Squeak: "Back to the Future" and Slang

Squeak (Ingalls, Kaehler, Maloney, Wallace, Kay — OOPSLA 1996,
https://dl.acm.org/doi/10.1145/263700.263754, PDF via
https://scispace.com/pdf/back-to-the-future-the-story-of-squeak-a-practical-smalltalk-59r29lu1to.pdf)
is the paper Agel's own bootstrapping story should study closely, because it
answers "how do you self-host a VM without writing a second, unrelated
implementation in C?" Squeak's VM is written in **Slang**, a restricted
subset of Smalltalk syntax that maps directly onto C constructs (no real
blocks except a few control idioms, no dynamic message sends where a direct
call is meant, no true object headers) — code you can *execute directly in
the Smalltalk image* (as an interpreter, for debugging/development) or
*mechanically translate to C* (for a fast, standalone VM). The same source
text is simultaneously "runnable Smalltalk for interactive development" and
"compilable systems code." This is close to what Agel's IR/native backend
already is (Agel code that compiles to x86-64), but the Squeak lesson
sharpens the payoff: writing the VM/runtime bootstrap *in the restricted
subset of the host language itself*, rather than in a wholly separate
systems language, keeps one syntax and one set of tools for the whole
tower, and lets the same code be interpreted (for correctness/debugging) or
compiled (for speed) without divergence.

## Cog, Spur, and Sista (Eliot Miranda, Clément Béra)

Eliot Miranda's Cog VM (blog: http://www.mirandabanda.org/cogblog/,
"Two Decades of Smalltalk VM Development," https://www.researchgate.net/publication/328509577)
replaced Squeak's classic interpreter with a **context-to-stack mapping**
JIT: method/block activations run on a real native stack by default (fast),
but any activation can be transparently "reified" into a first-class
`MethodContext` heap object on demand (for the debugger, `thisContext`,
non-local returns, continuations) — the same lazy-materialization idea
Deutsch/Schiffman had, done for a modern JIT. **Spur**, the object-memory
redesign layered under Cog, tackled Smalltalk's `become:` primitive (swap
the identities of two objects everywhere in the heap — used for schema
migration and for atomically publishing a new class/method set) which
historically required either an extra indirection on every object (slow
slot access, always) or a full heap sweep (slow become:, rarely). Spur uses
direct pointers plus **lazy forwarding pointers** with a partial read
barrier baked into send-cache and primitive-argument checks, so `become:`
is cheap without taxing every object access — see
https://clementbera.wordpress.com/2014/01/16/spurs-new-object-format/ and
the SPLASH-I talk "Spur: Efficient Support for Live Programming in Dynamic
Languages" (2015). **Sista** (Béra & Miranda, "Sista: Saving Optimized Code
in Snapshots for Fast Start-Up," ManLang 2017,
https://rmod-files.lille.inria.fr/Team/Texts/Papers/Bera17b-ManLang-SistaArchitecture.pdf)
pushes Self's adaptive-optimization idea *into the image*: the optimizer is
itself written in Smalltalk, optimizes bytecode-to-bytecode (not to machine
code) so the optimized form is portable and inspectable, and — crucially
for an image-based/world-file system — the optimized, speculative code can
be **persisted across snapshots** with deoptimization guards intact, so a
saved image restarts already warm instead of re-learning hot paths from
cold. That is a direct, load-bearing precedent for Agel's world files: if
Agel ever wants JIT'd or specialized code to survive a world-file save/
restore, Sista is the existing design to copy (optimize in a bytecode/IR
the persisted format already understands; encode deopt guards as data next
to the code; never persist raw machine-code addresses).

## Pharo and Newspeak

Pharo (a Squeak/Cog-descendant, cleaned up for modularity and reduced
legacy surface) mainly demonstrates that a Smalltalk image can be
*continuously modernized in place* over 15+ years without a rewrite,
because the image-and-changes model plus a strong package/dependency system
(Metacello) lets subsystems be replaced live. Newspeak (Bracha et al.,
"Modules as Objects in Newspeak," https://bracha.org/newspeak-modules.pdf,
platform paper https://bracha.org/newspeak.pdf) goes further than
Smalltalk on modularity: **top-level classes are the module system** — a
module is just an instantiable, parameterized class with no global
namespace and no static/ambient state, and the only way one module gets
access to another (or to any capability, including "the filesystem" or "the
network") is by being handed a reference at construction time — i.e., an
**object-capability** discipline is the module system, not a bolt-on
security layer. This is unusually relevant to Agel, which already has
"capability-scoped effects with deterministic budgets" and agents with
isolated heaps: Newspeak is the existing precedent that a class/module
system and a capability-security system can be *the same mechanism*
instead of two overlapping ones, which argues for wiring Agel's module
system (`module`/`import`) to hand out capabilities as ordinary
constructor arguments rather than as a separate ambient-capability table.

## Image persistence, `become:`, and the debugger as the main tool

Across Smalltalk, the recurring lived experience (well documented anecdotally,
e.g. https://medium.com/smalltalk-talk/improving-smalltalk-s-image-f078b6f806d8)
is: the image (a full heap snapshot of every live object, including open
windows and stack frames mid-computation) plus a "changes" log of source
edits since the last snapshot is not just a save format, it *is* the
development model — there is no edit/compile/run cycle distinct from
"the system," and the debugger is not a diagnostic bolted onto a batch
compiler but the primary way code gets written (you hit an error, get a
live stack in the debugger, patch the offending method, and resume the
same computation from where it broke). The recurring **failure mode** is
image fragility: a bad `become:`, a corrupted class hierarchy edit, or an
incompatible version of a base class can leave an image unopenable, and
"my image won't boot and my only backup is three weeks old" is a
running joke in the Smalltalk community for a reason. Two mitigations
that did work: (1) treat the changes-file/source log as the durable
ground truth and the image as a derived, disposable cache (Smalltalk
projects that did this recovered from corruption far more easily than
those that trusted the image alone); (2) make transformations like
`become:` and class redefinition atomic and narrowly scoped rather than
sweeping. Agel's transactional worlds (each form commits or rolls back,
world files as canonical encodings) are already a stronger answer to
exactly this failure mode than classic Smalltalk had — the lesson is to
keep leaning on that rather than ever introducing an in-place,
non-transactional mutation path into the live image for performance
reasons, however tempting.

## Simplicity-first Smalltalks: Little Smalltalk, Smalltalk/X

Little Smalltalk (Timothy Budd, Oregon State, 1984–1987; book at
https://archive.org/details/ALittleSmalltalkBook) is the counter-example to
the Blue-Book-and-beyond arms race: a deliberately small, non-optimizing,
bytecode-interpreted Smalltalk meant to be understood end-to-end by a
single reader, and it was — per Wikipedia — the first Smalltalk
implementation outside Xerox PARC, valuable precisely because its
simplicity made it *portable and legible* rather than fast
(https://en.wikipedia.org/wiki/Little_Smalltalk). Smalltalk/X similarly
prioritized straightforward, well-understood implementation techniques
(a fairly conventional threaded/bytecode VM, ahead-of-time compiled base
classes) over VM cleverness, and survived commercially for decades on that
basis. The lesson for Agel, still bootstrapping: a small, boring,
easy-to-audit evaluator that gets the semantics and the transactional/
capability model exactly right is worth more right now than an aggressively
optimizing one — optimization can be layered on later (as Self/Cog/Sista
show) without touching the semantics, but semantics bugs baked into a
"smart" VM are expensive to excavate.

## Lisp machines: Genera and Interlisp-D

Symbolics Genera and Xerox Interlisp-D are the "whole OS is Lisp" end of
the spectrum Agel is explicitly aiming at. Genera's hardware (the 3600
series) tagged every word so the object system's dynamic typing, garbage
collection, and even debugging (e.g., catching a type error as a hardware
trap) were supported directly by the CPU's microcode —
https://jrdelaney.substack.com/p/deep-dive-lisp-machines-the-rise and Moon's
"Garbage Collection in a Large Lisp System," https://dl.acm.org/doi/10.1145/800055.802040,
which documents converting Genera's collector to run in a software emulator
by moving parts of it into new emulated "microcode" instructions — i.e.,
even the historically hardware-tag-dependent GC was eventually made to run
emulated, which matters because it shows the tagging discipline (not the
silicon) was the actual load-bearing idea; Agel, running on stock x86-64,
can and should keep the discipline (tagged/typed values, GC-aware layout)
without needing special hardware. Interlisp-D
(https://interlisp.org/history/, timeline https://interlisp.org/history/timeline/)
is notable for **DWIM** (Do What I Mean, Warren Teitelman 1968) — an error
correction/completion layer baked into the environment on the theory that
the system should be forgiving of small mistakes — and for having the
*entire environment* (structure editor, cross-referencer, windowing system)
be Lisp data structures inspectable and modifiable the same way user
programs are. The recurring lesson from both: when "the whole OS is
[language]," the payoff is that every tool (debugger, editor, inspector)
is automatically extensible in the same language users write in — but the
1980s hardware-specific implementations died commercially when general
workstations got fast enough to run Lisp adequately in software, which is
itself a caution: don't over-invest in bespoke low-level machinery
(hardware tags, a custom kernel) beyond what the software discipline
already buys you, because commodity hardware/software eventually catches
up and outlasts the bespoke path. (Agel's choice to target stock x86-64/
AArch64/RISC-V under seL4/its own kernels rather than bespoke silicon is
the right side of that history.)

## MIT Scheme, the Lambda Papers

Scheme (Sussman & Steele, MIT AI Memos 1975–1980, "the Lambda Papers,"
https://research.scheme.org/lambda-papers/) began as an accident of
understanding Hewitt's actor model: they wrote a toy actor-message
interpreter in MacLisp and discovered function application and actor
message-send were the same operation, so "message send" collapsed into
"procedure call with lexical closures," giving Scheme's minimalism (one
namespace, proper tail calls, first-class continuations) almost as a side
effect of trying to explain something else cleanly. The bootstrapping
lesson: **radical simplicity can come from unifying two things that turn
out to be one thing**, not from a deliberate minimalism campaign — worth
remembering when Agel's own agent-mailbox-send model and function-call
model start to look structurally similar.

## SBCL/CMUCL's "Python" compiler

CMUCL's "Python" compiler (no relation to the language; Rob MacLachlan,
1985 on, https://www.sbcl.org/history.html, "Python compiler for CMU Common
Lisp" https://www.researchgate.net/publication/221252239) is the
demonstration that a dynamically-typed Lisp can get C-like performance from
a compiler written in the language itself, via aggressive local **type
inference** (propagating `declare`/inferred types through a program to
eliminate generic-arithmetic dispatch and runtime type checks) plus a
native-code backend, all self-hosted (the compiler compiles itself). SBCL
forked from CMUCL in 1999 specifically to get a reproducible,
from-C-and-a-frozen-Lisp-core bootstrap and became compiler-only (no
separate byte-code interpreter to maintain) — a useful precedent for
Agel's plan to keep pushing the compiler frontend and native backend into
the guest: a "compiler-first" implementation is viable and can outlive a
more baroque implementation with two/three internal execution engines.

## Chez Scheme: nanopass, self-hosting, simplicity

R. Kent Dybvig's Chez Scheme (1985, open-sourced 2016) is arguably the best
existing model for "how do you keep a serious optimizing compiler simple
enough that you can trust and rewrite it." The **nanopass framework**
(Sarkar/Waddell/Dybvig, ICFP 2004; Keep & Dybvig, "A Nanopass Framework for
Commercial Compiler Development," ICFP 2013,
https://www.cs.tufts.edu/comp/150FP/archive/icfp13.pdf) replaced a
handful of large, complex compiler passes with 50+ tiny passes, each doing
one small, independently-verifiable rewrite of a precisely-typed
intermediate language, and found compile times stayed within 2x of the
original monolithic-pass compiler despite 5x as many passes — because each
pass is trivial to write, test and reason about in isolation, even though
there are more of them. Directly transferable to Agel's compiler frontend
and native backend: prefer many small, separately testable IR-to-IR passes
with explicit typed IRs between them over a few large ones, especially
since Agel's IR/backend now itself runs inside the guest and needs to be
auditable by the same discipline (fuel budgets, transactional commits) as
the rest of the system.

## Gambit, Guile, Racket-on-Chez

Gambit (Marc Feeley, 1988; C backend "Gambit-C" 1994,
https://en.wikipedia.org/wiki/Gambit_(Scheme_implementation)) shows a
"compile to portable C" backend strategy can deliver both performance and
portability across any platform with a C compiler — relevant if Agel ever
wants a backend target beyond hand-written x86-64/AArch64/RISC-V codegen.
Guile (GNU's extension-language Scheme,
https://www.gnu.org/software/guile/manual/html_node/Why-a-VM_003f.html,
JIT: https://www.gnu.org/software/guile/manual/html_node/Just_002dIn_002dTime-Native-Code.html)
took the deliberately staged path — bytecode interpreter first, add a
simple template JIT later (triggered by per-function call/loop counters) —
without ever needing Self-grade adaptive optimization, because as an
embedding/extension language most of its hot loops are short. Racket's
2019+ move to a Chez Scheme backend ("Racket CS": experience report,
https://dl.acm.org/doi/10.1145/3341642; status posts at
https://blog.racket-lang.org/2019/01/racket-on-chez-status.html) is the
most important recent data point of all: the Racket team explicitly
*replaced 200k lines of hand-written C runtime with ~150k lines of Scheme/
Racket riding on an existing, independently-maintained, self-hosting Scheme
compiler (Chez)*, and reported the goal was never "make Racket faster" but
"make Racket's implementation trustworthy and small enough for the Racket
team itself to maintain" — performance parity followed, sometimes at the
cost of memory. The lesson: **when a mature, simpler self-hosting sibling
language exists, riding on top of it and deleting your own C runtime can be
a bigger win than continuing to hand-optimize the C.** For Agel this maps
onto: once Agel's own guest-hosted compiler/backend are solid, the honest
long-term target is deleting Rust-side evaluator code paths that duplicate
what the guest compiler can now do, rather than maintaining both forever.

## Clojure: persistent data structures, refs/STM, agents

Clojure's core technical bet (Rich Hickey; overview slides
https://qconlondon.com/london-2009/qconlondon.com/dl/qcon-london-2009/slides/RichHickey_PersistentDataStructuresAndManagedReferences.pdf)
is that **immutable, structurally-shared persistent collections are not a
performance tax** — a persistent vector/map built as a shallow tree (32-way
branching, "HAMT"-like) gives O(log₃₂ n) update while sharing almost all
structure with the previous version, so "no destructive update" does not
mean "copy everything." On top of pure values, Clojure separates *identity*
(a Ref/Atom/Agent naming "the current value of this changing thing") from
*value* (immutable, comparable by `=`, safe to share across threads without
locks) and gives each identity type its own concurrency semantics: `ref`
+ STM with multiversion concurrency control for coordinated synchronous
updates across multiple refs, `atom` for uncoordinated synchronous compare-
and-swap, and `agent` for asynchronous, ordered, single-threaded-per-agent
updates delivered via a queue — notably, Clojure's "agent" is deliberately
close to Agel's own agent model (isolated mutable identity, messages
processed one at a time, no shared-memory races). This is direct precedent
that "immutable persistent values as the substrate + a small number of
named, well-typed identity/change constructs on top" scales to real
concurrent programs, and it argues Agel's world/agent model is already on
the right side of this lesson (each agent's heap is its own identity;
worlds commit-or-rollback like an STM transaction) — the risk to avoid is
letting any future performance work quietly introduce shared *mutable*
structure between agents "just this once."

## Common Lisp conditions and restarts

The condition system (formalized in ANSI Common Lisp via CLtL2,
background in Kent Pitman's "Condition Handling in the Lisp Language
Family," 2001, https://www.nhplace.com/kent/Papers/Condition-Handling-2001.html)
is the feature Agel explicitly already adopts, so the historical lesson is
mainly a warning about scope creep: CL's condition system took years and
several committee rounds (X3J13 votes in 1988–1989) to settle, partly
because it tried to unify error handling *and* generic "interesting event"
signaling *and* integration with CLOS, and partly because getting
`handler-case` vs `handler-bind` vs restart semantics (resuming from the
exact point of failure, vs. unwinding) right is genuinely subtle. The part
that most differentiates CL from ordinary exceptions — and that Agel should
make sure survives intact — is that **signaling a condition does not by
itself unwind the stack**; handlers run *in the dynamic context of the
signal*, so a restart can resume the failing computation in place (e.g.
supply a replacement value and continue) rather than only being able to
abort outward. That property is what makes conditions/restarts useful for
an agentic system: an agent's failed turn can offer restarts ("retry with
smaller budget," "use cached value," "ask supervisor"), and the caller
picks one without the failing frame having already been discarded — which
lines up with Agel's "a failed agent turn rolls back" model, provided
restart selection happens before the rollback destroys the state a restart
would need to inspect.

## Emacs Lisp native compilation

Emacs's native-compilation work (Andrea Corallo et al., "Bringing GNU Emacs
to Native Code," ELS 2020, https://arxiv.org/abs/2004.02504) is a recent,
well-documented case of bolting an AOT native backend onto a decades-old,
deployed, byte-compiled dynamic language without breaking its ecosystem:
the native compiler consumes the *existing* byte-compiler's IR (so all
existing tooling/semantics stay authoritative), most of the compiler's
optimization passes are written in Elisp itself, and only the final
codegen step calls out to a C library (`libgccjit`) — reported 2.3x–42x
speedups on micro-benchmarks. The transferable lesson for Agel: when adding
a native backend to an existing dynamic evaluator, reuse the existing
frontend/IR as the *sole* source of truth rather than building a parallel
type-checked path, and keep the actual machine-code-emitting layer as thin
and swappable as possible (Emacs uses `libgccjit`; Agel's backend already
emits x86-64 directly, which is more work but avoids an external
compiler-as-a-service dependency at runtime — reasonable given Agel's goal
of running the backend *inside* the guest, where shelling out to GCC is not
an option).

## Lessons for Agel, prioritized

1. **Keep the evaluator boring; put cleverness in a separately-versioned
   optimizer.** (Self's "simple runtime, smart compiler"; Little
   Smalltalk's "small enough to audit.") Concretely: do not let the
   guest-hosted native backend's optimizations leak assumptions into
   `agel-core`'s semantics — the backend should be one replaceable
   consumer of a stable IR, exactly as Sista's optimizer is a replaceable
   consumer of Squeak/Pharo bytecode.

2. **If/when Agel gets a JIT or hot-path optimizer for the guest-hosted
   evaluator, adopt the Self/Hölzle pairing: cheap baseline + counters +
   optimize-only-what's-hot + mandatory deoptimization back to an
   unoptimized frame.** This is the only known way to keep "you can always
   inspect/resume a live agent turn" (which Agel's transactional-worlds
   design already promises) compatible with native-code speed. Design the
   deopt path *before* the optimizer, not after — Self's team learned this
   the hard way (deoptimization was itself a follow-up paper).

3. **When you persist optimized/specialized code in a world file, follow
   Sista: persist it as bytecode/IR plus explicit deopt guards, never as
   raw machine addresses.** This is the direct answer to "can a world file
   survive across a version of the JIT/backend that changes register
   allocation or code layout" — the Sista paper is literally about solving
   this for image snapshots.

4. **Use maps/shapes, not per-object metadata, for Agel's map/record
   values.** Self's maps and their descendants (V8 hidden classes) are the
   standard answer to "objects with dynamic-looking fields still need fast,
   uniform field access" — relevant the moment Agel's map type or agent
   state records become a hot path.

5. **Treat `become:`-like operations (redefining a function/class/agent
   behavior in a live world) as Spur treats `become:`: cheap, forwarding-
   pointer-based, and scoped — never a full-heap sweep.** If Agel ever adds
   live in-place redefinition of running agent code (as opposed to
   spawning a new agent version), budget for this now rather than
   discovering it needs a heap walk later.

6. **Bootstrap the native backend/runtime the Squeak way: write it in a
   restricted, directly-compilable subset of Agel itself, not in a
   separate systems language, wherever possible.** Agel is already doing
   this (`agel/native-x86`, the compiler running in the guest per
   `docs/roadmap.md`) — the lesson is to keep pushing this line rather than
   letting Rust reabsorb responsibilities the guest compiler can now carry,
   mirroring how Slang stayed the single source for both the interpreted
   and translated VM.

7. **Follow the nanopass discipline for the compiler/backend pipeline:**
   many small, independently testable IR-to-IR passes over one or two
   typed intermediate languages, rather than a few large passes. Chez's
   experience (5x more passes, compile time within 2x) shows this doesn't
   cost as much as intuition suggests, and it is much easier to keep each
   pass provably fuel-bounded and world-transaction-safe, which Agel's
   determinism story needs anyway.

8. **Wire capabilities through the module system the Newspeak way**:
   modules/classes as parameterized, capability-receiving constructors
   rather than a capability table layered separately on top of a
   conventional module system. Agel already has capability-scoped effects
   and a module system (`module`/`import`); the Newspeak precedent argues
   for making capability-passing the *only* way a module gets authority,
   with no ambient/global fallback, which keeps the module system and the
   security model from drifting apart over time.

9. **Guard the condition/restart implementation against scope creep, but
   keep the one property that makes it worth having**: handlers run before
   the stack unwinds, so a restart can resume in place. Make sure "a failed
   agent turn rolls back" doesn't happen *before* restart selection — the
   rollback boundary should sit at "restart chosen and applied," not at
   "condition signaled," or Agel loses the CL property that justified
   adopting conditions in the first place.

10. **Treat the world file as the durable ground truth, the way the
    changes-file (not the image) should have been treated in classic
    Smalltalk.** Agel's transactional, canonically-encoded world files
    already do this better than image-based Smalltalk did (atomic commit/
    rollback per form vs. Smalltalk's advisory changes-log), so the
    concrete action is negative: never add a fast path that mutates live
    world state outside a transaction, however tempting for JIT
    bookkeeping or agent scheduling metadata — that is exactly the seam
    where Smalltalk images became unrecoverable.

11. **Next self-hosting step, given where Agel is today** (compiler
    frontend + integer x86-64 backend run in the guest; evaluator/runtime
    still Rust, per `docs/roadmap.md`'s "The compiler and reader running in
    the guest" line): the SBCL/Chez/Racket-CS trajectory suggests the next
    milestone is not "rewrite the evaluator in Agel for its own sake" but
    **extend the guest-hosted backend's IR coverage (lists, texts, maps —
    named explicitly as current gaps) until the guest compiler can compile
    the metacircular evaluator (`agel/meta` in `stdlib.agel`) itself to
    native code**, at which point Agel can run its own evaluator
    guest-compiled rather than Rust-interpreted for at least a subset of
    programs — mirroring CMUCL/SBCL's move from "Lisp compiler hosted by
    another Lisp" to "Lisp compiler that compiles itself and needs
    increasingly little from the host," and Racket's later move of
    deleting the C runtime once the Chez-backed implementation was proven.
    The guest evaluator does not need to become the *only* evaluator
    immediately (Racket kept Racket BC around during the transition); it
    needs to become *good enough to trust for real workloads*, exactly as
    Racket CS was allowed to become the default only once benchmarks
    showed parity, not superiority, was enough justification.

## Sources

- Deutsch & Schiffman, "Efficient Implementation of the Smalltalk-80
  System," POPL 1984 — https://dl.acm.org/doi/10.1145/800017.800542
- Rice University lecture summary of Deutsch–Schiffman — https://www.clear.rice.edu/comp512/Lectures/22ST80.pdf
- Hölzle, Chambers, Ungar, "Optimizing Dynamically-Typed Object-Oriented
  Languages with Polymorphic Inline Caches," ECOOP 1991 — https://research.google/pubs/optimizing-dynamically-typed-object-oriented-languages-with-polymorphic-inline-caches/
- Hölzle, "Adaptive Optimization for SELF," PhD thesis, Stanford 1994 — http://i.stanford.edu/pub/cstr/reports/cs/tr/94/1520/CS-TR-94-1520.pdf
- Hölzle, Chambers, Ungar, "Debugging Optimized Code with Dynamic
  Deoptimization," PLDI 1992 — https://dl.acm.org/doi/10.1145/143103.143114
- Ungar & Spitz, "Constructing a Metacircular Virtual Machine in an
  Exploratory Programming Environment" (Klein), OOPSLA 2005 companion — https://dl.acm.org/doi/10.1145/1094855.1094865
- Strongtalk overview — https://en.wikipedia.org/wiki/Strongtalk
- Ingalls, Kaehler, Maloney, Wallace, Kay, "Back to the Future: The Story
  of Squeak," OOPSLA 1996 — https://dl.acm.org/doi/10.1145/263700.263754
  (PDF: https://scispace.com/pdf/back-to-the-future-the-story-of-squeak-a-practical-smalltalk-59r29lu1to.pdf)
- Miranda, Cog Blog — http://www.mirandabanda.org/cogblog/
- "Two Decades of Smalltalk VM Development" — https://www.researchgate.net/publication/328509577
- Béra, "Spur's new object format" — https://clementbera.wordpress.com/2014/01/16/spurs-new-object-format/
- Béra & Miranda, "Sista: Saving Optimized Code in Snapshots for Fast
  Start-Up," ManLang 2017 — https://rmod-files.lille.inria.fr/Team/Texts/Papers/Bera17b-ManLang-SistaArchitecture.pdf
- Bracha et al., "Modules as Objects in Newspeak" — https://bracha.org/newspeak-modules.pdf
- Bracha et al., "The Newspeak Programming Platform" — https://bracha.org/newspeak.pdf
- Little Smalltalk — https://en.wikipedia.org/wiki/Little_Smalltalk ; book: https://archive.org/details/ALittleSmalltalkBook
- "Deep Dive: Lisp Machines" — https://jrdelaney.substack.com/p/deep-dive-lisp-machines-the-rise
- Moon, "Garbage Collection in a Large Lisp System," 1984 — https://dl.acm.org/doi/10.1145/800055.802040
- The Medley Interlisp Project, history — https://interlisp.org/history/ and timeline https://interlisp.org/history/timeline/
- "The Lambda Papers" (Scheme) — https://research.scheme.org/lambda-papers/
- SBCL history — https://www.sbcl.org/history.html
- "Python compiler for CMU Common Lisp" — https://www.researchgate.net/publication/221252239_Python_compiler_for_CMU_common_Lisp
- Keep & Dybvig, "A Nanopass Framework for Commercial Compiler
  Development," ICFP 2013 — https://www.cs.tufts.edu/comp/150FP/archive/icfp13.pdf
- Chez Scheme — https://en.wikipedia.org/wiki/Chez_Scheme ; publications: https://www.scheme.com/pubs/index.html
- Gambit Scheme — https://en.wikipedia.org/wiki/Gambit_(Scheme_implementation)
- Guile VM/JIT manual — https://www.gnu.org/software/guile/manual/html_node/Why-a-VM_003f.html , https://www.gnu.org/software/guile/manual/html_node/Just_002dIn_002dTime-Native-Code.html
- "Rebuilding Racket on Chez Scheme" (experience report), OOPSLA 2019 — https://dl.acm.org/doi/10.1145/3341642
- Racket-on-Chez status posts — https://blog.racket-lang.org/2019/01/racket-on-chez-status.html
- Hickey, "Persistent Data Structures and Managed References," QCon London
  2009 — https://qconlondon.com/london-2009/qconlondon.com/dl/qcon-london-2009/slides/RichHickey_PersistentDataStructuresAndManagedReferences.pdf
- Pitman, "Condition Handling in the Lisp Language Family," 2001 — https://www.nhplace.com/kent/Papers/Condition-Handling-2001.html
- Corallo, Nassi, Manca, "Bringing GNU Emacs to Native Code," ELS 2020 — https://arxiv.org/abs/2004.02504
- Agel `docs/roadmap.md` (in-repo, for current self-hosting status cited in
  the final lesson)

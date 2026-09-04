# Design lineage

Agel combines old language ideas with newer agent-runtime constraints. These
projects are inputs, not dependencies, and Agel keeps the mechanisms separable:

- Common Lisp supplies homoiconicity, macros, conditions/restarts, and the
  bootstrap culture. Smalltalk supplies the live-image ideal; Objective-C
  supplies a pragmatic small runtime and message-oriented object model.
- [pi](https://pi.dev/) and Unix motivate a minimal core with small, replaceable
  agents and adapters. Agel keeps orchestration in ordinary language libraries
  (`agel/swarm`) instead of baking particular model workflows into syntax.
- [AgentFS](https://github.com/tursodatabase/agentfs) and its
  [disaggregated design](https://penberg.org/blog/disaggregated-agentfs.html)
  motivate inspectable, per-agent state and a stable storage interface. Agel's
  current steps are the copy-on-write workspace and canonical event-sourced
  image; object-backed disaggregation is future library work.
- [Chimera](https://github.com/penberg/chimera), its
  [design essay](https://penberg.org/blog/chimera.html), and the paper
  [*Towards Sandboxing Untrusted Agents in Userspace*](https://penberg.org/papers/penberg-chimera.pdf)
  motivate syscall-level mediation beneath language capabilities. Agel v0.6
  interposes one constrained process path, but does not claim DBT-strength
  containment; that remains a native boundary.
- Thinking Machines' [Interaction Models](https://thinkingmachines.ai/blog/interaction-models/)
  motivates splitting responsive foreground interaction from longer-running
  background thought. `agel-interaction` makes that split explicit and adds a
  separate verified-human authority class.

- [seL4](https://sel4.systems/) supplies the capability model Agel treats as
  authoritative: authority is a slot in a capability space with rights, never a
  name, string, or unguessable token. [Microkit](https://docs.sel4.systems/projects/microkit/)
  supplies statically composed protection domains, channels, memory regions and
  budget/period scheduling attributes. [Hubris](https://github.com/oxidecomputer/hubris)
  supplies restart-as-an-architectural-property: isolated tasks, generations, and
  introspection that does not require each task to implement a debug protocol.
  [Zircon](https://fuchsia.dev/fuchsia-src/concepts/kernel) supplies rights-carrying
  handles transferred over channels, [Genode](https://www.genode.org/) supplies a
  component contract separable from one kernel, [Tock](https://tockos.org/) supplies
  grant-style per-process resource ownership, and
  [Barrelfish](https://barrelfish.org/) supplies multicore-as-a-distributed-system.
  [CHERI](https://www.cl.cam.ac.uk/research/security/ctsrd/cheri/) is the future
  fine-grained hardware target. The evaluation that selected these, and the
  ideas Agel deliberately declines to adopt, are recorded in
  [`docs/microkernel-research.md`](microkernel-research.md).

- [Redox](https://www.redox-os.org/) supplies the shape of Agel's Linux
  application compatibility. Its [`relibc`](https://gitlab.redox-os.org/redox-os/relibc)
  demonstrates a C library written in Rust, and its schemes demonstrate a
  uniform service namespace — both as an unprivileged *personality* above the
  kernel rather than as the conceptual centre of the system. Agel takes that
  arrangement and changes one thing: a scheme-like name is not authority. A
  POSIX path resolves through a namespace capability a process was granted, and
  a name outside that capability is unreachable however it is spelled. Agel does
  not fork Redox; it writes its own personality against its own kernel contract.

The synthesis specific to Agel is the invariant stack: code remains ordinary
data, but privileged change crosses content-bound proposals, zero-authority
canaries, typed effects, event-sourced images, A/B promotion, and an independent
native recovery monitor. No individual inspiration provides that whole chain.

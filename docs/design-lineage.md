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

The synthesis specific to Agel is the invariant stack: code remains ordinary
data, but privileged change crosses content-bound proposals, zero-authority
canaries, typed effects, event-sourced images, A/B promotion, and an independent
native recovery monitor. No individual inspiration provides that whole chain.

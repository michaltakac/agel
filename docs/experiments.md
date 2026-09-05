# Agentic OS experiments

These are motivating experiments, not claims about the current release. Each
must preserve Agel's proposal, bounded preview, validation, capability, commit,
and rollback boundary.

## A browser that can understand and reshape itself

Treat the browser as a supervised society of agents: navigation, DOM,
JavaScript, layout, networking, storage, accessibility, and rendering remain
separate capability holders. For a difficult web application, a temporary
analysis swarm can receive read-only trace capabilities, observe DOM mutations,
network timing, layout invalidation, and user-visible behavior, then propose a
site-specific browser specialization. The proposal runs against recorded traces
in a disposable world before promotion; webpage content cannot grant the swarm
new authority or rewrite the trusted admission policy.

An eventual “inject an agent” action should therefore inject observation and
message endpoints, not unrestricted native code. The page, browser agents, and
analysis agents remain mutually suspicious even though all can inspect their
own Agel definitions.

## Application fixed points and composition

Explore a practical application-composition operator inspired by fixed-point
combinators. Given two application agents, an `app-fix` agent can discover their
protocols, construct an adapter graph, and repeatedly refine a combined
application until its declared behavioral contract stabilizes. Examples:

- calendar + mail → a correspondence-aware scheduling workspace;
- editor + debugger → a live program microscope;
- browser + research notebook → a provenance-preserving investigation tool;
- file manager + model swarm → a semantic project environment.

The fixed point is over immutable application descriptions and executable
tests, never over unchecked privileges. Every iteration is inspectable, has a
resource budget, and can be discarded; the last promoted application remains
available for instant rollback.

## Agents all the way down

Longer-term demonstrations should make each layer explain and safely propose
changes to itself: widget, application, shell, driver, service, language
library, and eventually kernel configuration. Self-knowledge does not imply
self-authorization. An agent can describe and patch its implementation, while a
different minimal authority validates and promotes the patch under explicit
invariants.

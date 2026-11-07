# v0.3.104

External repair actors require a module-private driver seal on completion
and activation-probe messages. The seal is bound to the actor, request,
and stage. A mailbox sender that knows the pending ID cannot approve a
proposal or probe by supplying an ordinary result message.

Tests cover direct message injection, a look-alike seal, and reuse of a
completion seal from the event log as a probe seal. This protects the
external repair actor's message path. The controller that supplies its
judge and probe callbacks remains trusted.

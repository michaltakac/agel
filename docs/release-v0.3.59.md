# v0.3.59 · Drive repair from the actor's state

`repair-agent-drive` advances an external-dispatch repair actor while
it is proposing, judging, or awaiting an activation probe. It returns
the final state and reports exhaustion if active work remains after
the transition budget. The native service and tool examples use this
instead of listing individual exchanges.

Native combat comparisons now include the verified policy decision
in the Agel result. An arm that continued after refusal carries
`refused-retained-fixed` separately from task correctness, time, and
charges.

The repair suite checks exhaustion and resumption through an activation
probe. A scorer fixture checks that a refused policy, incorrect outcome,
and recorded judge charges all survive in the result.

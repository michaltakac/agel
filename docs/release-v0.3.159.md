# v0.3.159

A candidate that raises a condition during replay leaves a receipt with the
number of cases that passed before the failure and the condition kind.
Repair agents use that count to identify the failed stage. Retry prompts
report a held-out condition without including its raw message.

Hosted evolution, live-tool and repair tests passed, including a condition
raised after successful replay steps and a retry after a held-out condition.

# v0.3.171

Native combat, crafting, economy, and quest failures report how many
live packets used the adaptive policy. When an older failed run has a
verified commit but no live-use receipt, the scorer reports
`committed-use-unknown`.

A scoring fixture distinguishes missing, zero, and positive use counts.
Rescoring a retained quest pair changed only the policy-use classification.

# v0.3.73 · Record native writer token usage

Live native runs ask the Codex CLI for structured events. The bridge
returns the final writer answer to Agel and records the completed turn's
input, cached input, and output token counts beside the inference job ID.
Missing usage is marked explicitly. The benchmark still needs a verified
price for the billed model to produce a writer dollar total.

The hosted Cook's Assistant run with the v0.3.72 dialogue fix saved all
three hand-ins, one quest point, and 300 Cooking XP at normal rights.

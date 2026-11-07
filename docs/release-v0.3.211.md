# v0.3.211

Melee scores show when a missing stance update led the trainer to seek
confirmation from XP. Each fallback is marked confirmed only when the
trace records XP earned in the requested skill during that phase.
The saved stance and overall correctness verdict remain separate.

Trace fixtures distinguished confirmed and unconfirmed fallbacks,
including XP earned in the wrong skill.

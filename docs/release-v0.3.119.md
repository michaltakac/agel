# v0.3.119

The native desktop reads optional application registrations from the image’s
`/data/apps.cfg`. DOOM supplies its program names, source paths, cell prefixes,
and ready messages in its example package. The example also supplies the
startup line to wait for and the companion agent to join during handover.

The handover command accepts `wait=PREFIX` and `join=NAME`. Existing commands
using a trailing `agents` argument should name the companion with `join=NAME`.

RuneScape’s client, protocol, prompts, and scoring remain in the examples.
The architecture guide records where application code and reusable facilities
belong.

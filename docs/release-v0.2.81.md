# Agel v0.2.81 — Three partial rows closed

Three rows of the roadmap said *partial* for reasons that were tests or
encodings the project had not written. This release writes them.

## What changed

- **A canonical encoding for the evidence digest.** The digest that binds
  a verified proposal to the world state it was verified against was
  SHA-256 over a debug rendering of the state; a change in that rendering
  would have invalidated stored proposals. It is now SHA-256 over
  `crates/agel-core/src/canon.rs`: every value, closure environment,
  agent, event, macro, module and model record written as tagged,
  length-delimited bytes in a fixed order, versioned by the prefix
  `agel-world-canonical-v1`, the same on any build. The replay checksum
  (`state_digest`) runs over the same bytes. A fresh world's digest is
  pinned by a test vector, so an accidental change to the encoding fails
  the test rather than the stored evidence.
- **`ESTALE`, exercised.** `boot/posix/stale` opens `notes`, reads it,
  sleeps six seconds and reads again through the same descriptor. The
  desktop hands the prompt back while it sleeps (`PROCESS SLEEPING`), the
  operator's `:fs-restart` replaces the filesystem service with a new
  generation, and the second read answers 116: the descriptor's authority
  came from a service that no longer exists. The process exits 116 and
  the file is read again by a fresh descriptor from the desktop's
  evaluator.
- **A model-driven episode through the bridge, in CI.** `agel-play
  --policy echo` boots the desktop, loads `doom-agent-model.agel` and
  answers every `model-request` the program makes — four steps, four
  requests, four replies, each recorded in `steps.jsonl` — on the same
  path the Claude and Codex providers take. For that, `model-result` now
  spends the answer it reads: before, the last reply stayed readable and
  the program asked once and decided from it for the rest of the run;
  now a program that asks each step asks anew, and its decision is for
  the observation that step made. The native evaluator's world state
  carries the change, so a failed form leaves the answer unread.

## Proof

`crates/agel-core/tests/language_core.rs` (`canonical_digest`): the
pinned vector `44a2357b…9ebe` for a fresh world; two worlds evaluating the
same source (a definition, an agent, a message) digest equal, one more
definition digests different, and a snapshot restores the same digest.
`scripts/test-stale.sh` and `scripts/test-play-bridge.sh` as above. The
kernel changes by one assignment and stays inside its slot. The full
regression passes.

## Not claimed

Rows the hardware decides stay as they are: signing the trusted slot, the
selector, workspace generations and the recovery record would put a key
in the kernel that verifies them, and there is no root of trust before
the BIOS stage to hold it — that row stays partial on purpose; the
Raspberry Pi 5 image has never run because no emulator models the board.
A real model deciding an episode is the same bridge path with credentials
and is run by hand. The canonical encoding is a digest input, not a
serialization: nothing decodes it, and images keep their own entry-chain
format.

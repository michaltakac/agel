# Agel v0.2.98: a goal for the game

The judged DOOM agent at v0.2.97 walked forward into a wall and along it,
because nothing it was asked or told knew where the level's exit was. A
person watching said so, and [`parallel-agents.md`](parallel-agents.md)
is the plan that answers it. This release is its first milestone: the
engine reports its own map, the program steers by arithmetic, and Jev is
asked only what the picture holds. Its honest label is a better sensor
and a hand-tuned policy: every change in these notes was made from
outside, in C and Rust, by a person reading the agent's metrics, which
an OS on a device could never do. The plan's next milestone is the agent
making such changes itself, from inside, in Agel.

## What changed

- **The engine reports the way to the exit, and what is ahead.** Each
  `doom: state` line now ends with `goal H dist D path P seen S free A L
  R door K`. The engine walks its own map over a grid of 32-unit cells
  (twice the player's radius): two neighbouring cells are joined when a
  walk between their centres crosses no line a player cannot cross (a
  one-sided wall, a blocking line, a ledge more than a step up, a gap
  shorter than a player; a closed door is crossed, since a door opens),
  tested with the engine's own line traversal and kept once tested, so
  the map is learned as it is walked. A breadth-first search from the
  player's cell to the cell in front of the first exit line is the
  route. `goal` is the heading to the furthest cell along it that a
  straight walk from the player reaches, in the same 256ths of a turn as
  the player's angle, `dist` the distance to it in map units, `path` the
  route's remaining length, `seen` the share of the level's lines the
  automap has drawn, `free` how far three rays reach before a line that
  cannot be crossed (ahead, a quarter turn left, a quarter turn right,
  at most 512 units), and `door` whether a closed door stops the ray
  ahead. Four first cuts are in the notes because each taught the
  design: the exit as the crow flies had a live run circle the start
  room, the exit being south-east behind its walls while the route
  leaves north; a path over sectors flipped between routes as the
  player crossed strips of floor, and left the start room through a
  line 800 units away in another region the same sector number tags,
  since a sector is a number, not a place; a path over rooms (a sector's
  boundary lines joined at their vertices) was right about the rooms and
  walked the player into the nukage pool, an island inside the start
  room that a straight walk to the room's exit crosses. The grid knows
  about islands. The rays and the route are read from the engine's own
  tables with its own traversal; nothing is decided in C. The look line
  grew from 128 to 192 bytes to carry the line.
- **`doom-agent-judge` steers.** The program's questions are what the
  judge can see in the picture: `foe` (an enemy in view) and `risk`
  (safe, wary, lethal), and, when the automap is up, `way` (which
  quadrant is unexplored). Which way to turn is arithmetic on fields 3
  and 8 of the state line: within 12/256 of a turn of the waypoint
  it goes forward, otherwise it turns the shorter way. On the heading, a
  closed door within eighty units of the ray ahead is used while
  walking, before anything else, since a door is not a wall to go
  round; a forward step that did not move or a wall within forty units
  starts a follow of eight steps: four turning toward the side whose
  ray reaches further, four walking; an enemy in view is fired at while
  walking; a lethal risk while hurt backs off. Three forwards in a row
  that did not move press Tab; the judge names the unexplored quadrant,
  which is the heading for six steps, and Tab closes the map. Sixteen
  cells of at most 256 bytes, all the world holds. Two first cuts,
  again: one turned by the picture's brightness whenever stuck and
  checked the map every twentieth step, and in a live run wobbled in
  place at a wall for thirty steps and spent sixty of its 120 steps on
  detours away from the route; the next asked the judge what was
  directly ahead (open, door, wall, enemy, drop) and followed walls on
  its answer, and the judge, given an 80×25 picture, called the far end
  of the start room a wall at 848 units, so the agent turned away from
  an open route. The engine's rays know a near wall from a far one; the
  judge is asked what the rays cannot see. And the first run on the
  grid route reached the level's first door and spent seventy steps
  against it, because the program checked "blocked" before "door" and a
  closed door sixteen units ahead is both.
- **Metrics on the OS.** The program appends each step to `metrics` in
  the filesystem region: the state line and the answer line that decided
  it. `file-append` existed; the file is what the next milestone's window
  will draw.
- **Tab is a key the play loop can hold** (`tab`, scan code 0x0f).
- **The program loader reads 8 KiB, and refuses more.** The judged
  agent's source with its comments passed 4 KiB, and the loader read the
  first 4 KiB and replayed a program cut mid-cell, which failed at the
  cut with nothing said of why. A file that fills the buffer is now
  refused as `THE PROGRAM IS TOO LONG FOR THE LOADER`.
- **A short answer no longer fails a step.** `number` in both agent
  programs checks that a field exists before reading it as an integer:
  a judge's error reply has fewer fields than a judgment, and reading a
  missing one raised an error that ended the form, which is the hang the
  v0.2.97 notes listed as open. The bridge also stops on a loop's
  `DRIVE-STEP FAILED`, `PLAY-STEP FAILED`, `NO PROGRAM TO` and
  `NO WINDOW TO` statuses instead of waiting on. A global named `keys`
  is refused by the native evaluator, since it is a builtin's name; the
  program's is `chosen`.

## Measured

One live run of 120 judged steps, E1M1, against the endpoint
(`agel-play --policy jev --steps 120`), reported as it went:

| 120 steps, `doom-agent-judge` | start | end |
|---|---|---|
| route remaining, map units | 4416 | 2304 |
| position | 1056, −3616 (the start) | 2479, −2662 (the corridor east of the first door) |
| health | 100 | 82 |
| ammo | 50 | 42 |
| seen, share of the level's lines | 6 % | 22 % |
| level | 1 | 1 |

Keys held: forward 57, right 29, left 18, forward-and-fire 15, use 1.
Forward steps that did not move: 2. The one `use` opened the level's
first door at step 58; the fifteen shots were at what the judge called
an enemy in view (`foe` above 500 fifteen times); nothing was killed.
The map check was not needed (no three stalled forwards). Half the
route in 120 steps, with the exit's door still ahead; one run, no claim
of a rate.

The earlier cuts of the same milestone, each one run: the exit as the
crow flies did not leave the start room (route unknown; distance to the
exit 2432 → 2363); the sector path reached the second room and stuck at
a wall for thirty steps (rooms to cross 20 → 18); the path over rooms,
with the judge's "wall" as the block signal, turned away from the open
route (route unchanged); the grid route reached the first door and spent
seventy steps against it (route 4416 → 3264) until "door" was checked
before "blocked".

## Validation

- `scripts/test-play-bridge.sh`: the judged episode's stand-in answers
  the new questions; four steps go forward firing at the enemy it
  reports, and every state line carries `goal`, `dist`, `path`, `seen`,
  `free` and `door`.
- `scripts/test-drive.sh`: a judge's error reply is waited on and the
  next step asks again.
- The full local regression and CI.
- By hand: the live run above, against the endpoint.

## Not claimed

- Finishing the level. The route is the engine's shortest walk over a
  grid, not a walkthrough's: it knows nothing of keys, switches,
  monsters, lifts or what lies in a room, and a cell is a point to walk
  at, not a way around what stands before it. The wall-follow and the
  map detour are bounded reflexes.
- Any success rate: one live run, reported as it went.
- The agent changing itself. Every change in these notes was made by a
  person reading the agent's own metrics after a run; the plan's next
  rows are the mechanism by which the agent does that, by typed
  questions, with a model only for what a typed question cannot answer.
- The metrics window, agents side by side, a fetch through the host: the
  later milestones in [`parallel-agents.md`](parallel-agents.md).

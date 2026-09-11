# Agel v0.2.50 — A desktop that responds

The pointer does things now. Surfaces say when they are hovered, the
panel's "Applications" opens a launcher over the program region, a launcher
entry runs its program, the dock's tiles act, and the panel keeps the time
from a clock driver domain.

![The desktop at v0.2.50](images/native-desktop-v0.2.50.png)

## What changed

- **Hover states.** A dock tile lightens under the pointer, "Applications"
  gets a pill, a launcher entry a row; only the surfaces whose hover
  changed are repainted, with the pointer's path.
- **The launcher.** Lists the program table's names (a `Region::list`
  joins the shared reader); a click runs the name as a typed `:exec`, so
  the serial console shows it and the terminal panel receives the output.
- **Dock actions.** Agel and store open the launcher; terminal clears the
  panel; files lists the root; settings cycles the accent; help prints the
  help. Every action is a typed command underneath.
- **A clock.** `agel_clock_main` is a driver domain granted the two CMOS
  ports; it decodes the real-time clock from BCD and answers six numbers.
  The supervisor reads it at boot (`clock: YYYY-MM-DD HH:MM` on the serial
  console) and while idle, repainting the label when the minute turns.
- **Pointer packets are coalesced** into one repaint per burst, and a
  press ends the burst where it landed.

## Proof

`scripts/test-desktop-process.sh` now also requires the clock line at
boot, moves the pointer to "Applications" in packet-sized steps, clicks,
clicks the first launcher entry and reads the program's output on the
serial console, clicks the files tile and reads the listing, and checks
the launcher closed. The graphics self-test's digest is refrozen.

## Not claimed

The editor tile does nothing. The launcher shows the table's first eight
names. A large pointer move sent as one event loses packets to the 8042's
queue, as a real mouse never sends; the test steps.

# Agel v0.2.77 — Programs from files

The rung after room: the desktop loads Agel programs from its own
filesystem. Until now every program the OS ran came from the kernel image
(`:load workbench`, `:load doom-agent`) or was typed a form at a time. Now
an Agel form can write a program to a file with `file-write` and the
desktop can load it, so the language extends the system from inside it,
and a program left on the disk runs at the next boot.

## What changed

- **`:load-file PATH`** evaluates every top-level form of an Agel file in
  order, each its own transaction with the desktop answering its effects.
  The first form that fails stops the load, and the status says how many
  forms came before it; they stay. A form is still one shared-page payload
  of 256 bytes, and a longer one is refused by size before it is parsed.
  The file is at most 16 KiB.
- **`:load NAME`** for a name the desktop does not carry reads
  `/NAME.agel` the same way, so `:load tool` runs the tool an agent wrote.
- **`/init.agel` runs at boot,** after the workspace is restored and before
  the first prompt, with its report on the console; a failing form in it
  stops it without stopping the desktop.
- **A splitter in the supervisor** finds the forms: a bracket counter that
  respects strings, their escapes and `;` comments, handing each form to
  the evaluator to judge.

## Proof

`scripts/test-programs.sh`: an Agel form writes `tool.agel` with two
definitions, `:load-file` runs it and the definitions answer; `:load more`
reads `/more.agel`; a file whose second form divides by zero loads one form
and stops, the first kept and the third never run; then a form writes
`/init.agel`, the machine is booted again from the same disk, and the
console reports the load before the first prompt with the definition in
place. The full regression passes; the kernel stays inside its slot.

## Not claimed

A program file is not signed or checked against anything; it runs with the
operator's authority as any typed form does, and there is still no
capability a program must hold to write a file or to be loaded. The
splitter is not a reader. A loaded program is not persisted as workspace
cells: the file is its persistence, and `/init.agel` its way to run at
boot. The runtime that holds arbitrary programs is still the rung after.

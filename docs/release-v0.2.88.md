# Agel v0.2.88 — Real tail calls in the backend written in Agel

v0.2.87 gave the hosted evaluator proper tail calls. This gives them to
the backend written in Agel: `agel/native-x86` now compiles a call in
tail position through a parameter as reuse of the current frame, and a
`let` as slots of the frame it appears in, so a loop written as a
function calling itself last runs in constant stack — the OS runs a
million-iteration loop the backend compiled without the stack growing.
A tail call was a call and a return before; a loop was bounded by the
process's stack.

## What changed

- **A callee-pops calling convention.** Every compiled function ends
  `ret 8(arity+1)`, popping the block it was given — its closure and
  arguments. This is the precondition for safe tail calls: the caller no
  longer cleans up after a call, so a tail call can replace the block
  and jump without the eventual return popping the wrong amount.
- **Frame reuse for a tail call through a parameter.** `(tail-call f
  args)` where `f` is a parameter (the self convention, or a function
  passed in) evaluates the closure and arguments onto the stack, copies
  the block over the current frame's from the top down (safe for any
  arity the frame can hold), restores the caller's frame pointer and
  return address, and jumps through the closure. A tail call whose callee
  takes *more* arguments than the frame holds stays a plain call —
  correct, bounded by the stack, and never writing past the frame.
- **`let` is inlined into its frame.** A function literal applied at once
  (which is how the frontend lowers `let`) becomes slots of the enclosing
  frame rather than a call, so a tail call inside a `let` body reuses the
  enclosing frame too. Lexical addressing tracks both frame parameters
  (reached through the static link) and inline `let` slots (below the
  link in the same frame).

## Proof

`crates/agel-stdlib/tests/backend.rs`: a tail-recursive loop compiles to
a jump through the closure (`jmp [r11]`) with the callee popping its own
four-word block (`ret 32`), while fib — whose self-calls are operands of
`+`, not tail calls — keeps an ordinary call. `scripts/test-agel-process.sh`:
the loaded runtime compiles `(fn (self n acc) (if (= n 0) acc (let ((m (-
n 1))) (self self m (+ acc n)))))`, the OS installs and runs it on
`(1000000 0)`, and it answers 500000500000 in one frame — a million tail
calls through a `let` with the stack not growing. The earlier fib, `let`
and tail-sum programs still compile, install and run. The full
regression passes; the kernel is unchanged.

## Not claimed

The backend compiles the integer subset still: no lists, texts or maps —
that is the next rung, and it needs a heap and a collector (a per-agent
Cheney semispace with a shadow stack, per the research decisions). A tail
call whose callee takes more arguments than the frame holds is a plain
call, so a loop that grows its argument count each turn is still bounded
by the stack; the common self-recursive-with-accumulator loop is not.
The hosted JIT (`agel-jit`, Cranelift) remains the full backend on the
host.

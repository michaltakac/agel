#!/bin/sh
# Boot the Agel kernel contract on an unmodified seL4 kernel and require the
# same frozen transcript every other backend produces.
#
# The system has no debug-exit device and Microkit protection domains have no
# way to power the machine off, so the harness reads the serial transcript until
# the run reports itself finished and then stops the emulator. A run that never
# reports is a failure, not a pass.
set -eu

deadline=${AGEL_SEL4_TIMEOUT:-120}
image=$(./scripts/build-sel4.sh "$@" | tail -n 1)

output_file=$(mktemp "${TMPDIR:-/tmp}/agel-sel4.XXXXXX")
transcript_file=$(mktemp "${TMPDIR:-/tmp}/agel-sel4-transcript.XXXXXX")
trap 'rm -f "$output_file" "$transcript_file"' EXIT HUP INT TERM

# The board's own documented invocation: the Microkit loader is placed by the
# `loader` device rather than by `-kernel`.
qemu-system-aarch64 \
  -machine virt,virtualization=on -cpu cortex-a53 -m 2G \
  -display none -monitor none -serial stdio -no-reboot \
  -device "loader,file=$image,addr=0x70000000,cpu-num=0" \
  < /dev/null > "$output_file" 2>&1 &
emulator=$!

elapsed=0
while true; do
  if grep -q 'AGEL_SEL4_OK\|AGEL_SEL4_FAILED' "$output_file" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$emulator" 2>/dev/null; then
    break
  fi
  if test "$elapsed" -ge "$deadline"; then
    kill "$emulator" 2>/dev/null || true
    wait "$emulator" 2>/dev/null || true
    printf '%s\n' "seL4 system produced no verdict within ${deadline}s" >&2
    cat "$output_file" >&2
    exit 1
  fi
  sleep 1
  elapsed=$((elapsed + 1))
done
kill "$emulator" 2>/dev/null || true
wait "$emulator" 2>/dev/null || true

if grep -q 'AGEL_SEL4_FAILED' "$output_file"; then
  printf '%s\n' "the seL4 system reported a failure" >&2
  cat "$output_file" >&2
  exit 1
fi
if ! grep -q 'AGEL_SEL4_OK' "$output_file"; then
  printf '%s\n' "the seL4 system stopped without a verdict" >&2
  cat "$output_file" >&2
  exit 1
fi

# The transcript an unprivileged protection domain produced, by asking a server
# protection domain across a real seL4 protected procedure, must equal the same
# frozen bytes the hosted model and the three research backends produce.
tr -d '\r' < "$output_file" \
  | sed -n '/^---BEGIN AGEL CONTRACT TRANSCRIPT---$/,/^---END AGEL CONTRACT TRANSCRIPT---$/p' \
  | sed '1d;$d' > "$transcript_file"
diff -u bootstrap/kernel-contract.trace "$transcript_file"

grep -q 'world: 81 invocations answered by the broker' "$output_file"
grep -q 'world: contract invariants hold across the boundary' "$output_file"
grep -q 'recovery: contained it without replying; the world is not resumed' "$output_file"

printf '%s\n' \
  "Agel on seL4: 81 contract steps from an unprivileged protection domain, fault contained by its parent [ok]"

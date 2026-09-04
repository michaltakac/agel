#!/bin/sh
set -eu

kernel=$(./scripts/build-boot.sh --features isolation-selftest | tail -n 1)
output_file=$(mktemp "${TMPDIR:-/tmp}/agel-isolation.XXXXXX")
transcript_file=$(mktemp "${TMPDIR:-/tmp}/agel-ring3.XXXXXX")
disassembly_file=$(mktemp "${TMPDIR:-/tmp}/agel-usertext.XXXXXX")
trap 'rm -f "$output_file" "$transcript_file" "$disassembly_file"' EXIT HUP INT TERM

# The ring-3 program may only reach `.user_text`, the one range of the image the
# page tables mark user-executable. A call or an indirect jump through a table
# would land in supervisor-only memory and fault, so reject them at build time
# rather than discovering it as a mysterious page fault at run time.
if command -v gobjdump >/dev/null 2>&1; then
  objdump_bin=$(command -v gobjdump)
elif test -x /opt/homebrew/opt/binutils/bin/gobjdump; then
  objdump_bin=/opt/homebrew/opt/binutils/bin/gobjdump
elif command -v objdump >/dev/null 2>&1; then
  objdump_bin=$(command -v objdump)
else
  objdump_bin=
fi
if test -n "$objdump_bin"; then
  "$objdump_bin" -d --section=.user_text \
    boot/kernel/target/x86_64-unknown-none/release/agel-boot > "$disassembly_file"
  if grep -Eq '\b(call|jmp)[[:space:]]+\*' "$disassembly_file"; then
    printf '%s\n' "ring-3 text contains an indirect branch; it cannot stay self-contained" >&2
    exit 1
  fi
  if grep -Eq '\bcall\b' "$disassembly_file"; then
    printf '%s\n' "ring-3 text calls out of .user_text into supervisor-only code" >&2
    exit 1
  fi
fi

set +e
perl -e 'alarm shift; exec @ARGV' 60 qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -display none -monitor none -serial stdio -no-reboot \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  -boot order=c,strict=on \
  -drive format=raw,file="$kernel",snapshot=on > "$output_file" 2>&1
status=$?
set -e

test "$status" -eq 33
grep -q 'AGEL_ISOLATION_OK' "$output_file"

# The transcript the unprivileged world produced must equal the frozen contract
# transcript byte for byte. One corpus, one reference model, two backends: a
# hosted process and a ring-3 protection domain talking through a trap gate.
tr -d '\r' < "$output_file" \
  | sed -n '/^---BEGIN AGEL RING3 CONTRACT TRANSCRIPT---$/,/^---END AGEL RING3 CONTRACT TRANSCRIPT---$/p' \
  | sed '1d;$d' > "$transcript_file"
diff -u bootstrap/kernel-contract.trace "$transcript_file"

grep -q 'contained a world writing to kernel memory: page-fault' "$output_file"
grep -q 'contained a world dividing by zero: divide-error' "$output_file"
grep -q 'contained a world masking interrupts: general-protection' "$output_file"
grep -q 'preempted a world that never yields' "$output_file"
grep -q 'watchdog fault: rolled back to slot A' "$output_file"

printf '%s\n' \
  "Agel ring-3 isolation: contract from user mode -> fault, privilege and loop containment [ok]"

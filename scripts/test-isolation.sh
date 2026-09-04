#!/bin/sh
# One contract, one corpus, one frozen transcript, three machines.
#
# For each architecture this boots the research kernel under QEMU and requires
# that an unprivileged world answers all 81 kernel-contract steps with a
# transcript byte-identical to bootstrap/kernel-contract.trace, that every way
# that architecture lets a world misbehave is contained, and that the recovery
# monitor still works afterwards.
#
# With no argument every architecture runs. Naming one runs only that one.
set -eu

run_x86_64() {
  image=$(./scripts/build-kernel.sh x86_64 | tail -n 1)
  qemu-system-x86_64 \
    -machine pc,accel=tcg -m 64M -display none -monitor none -serial stdio -no-reboot \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -boot order=c,strict=on \
    -drive format=raw,file="$image",snapshot=on
}

run_aarch64() {
  image=$(./scripts/build-kernel.sh aarch64 | tail -n 1)
  # There is no debug-exit device on `virt`; the kernel leaves through PSCI, so
  # a clean exit is status 0 and the success token carries the verdict.
  qemu-system-aarch64 \
    -machine virt -cpu cortex-a72 -m 128M -display none -monitor none -serial stdio -no-reboot \
    -kernel "$image"
}

run_riscv64() {
  image=$(./scripts/build-kernel.sh riscv64 | tail -n 1)
  qemu-system-riscv64 \
    -machine virt -m 128M -display none -monitor none -serial stdio -no-reboot -bios default \
    -kernel "$image"
}

# The x86-64 debug-exit device maps the guest's clean value 0x10 to host status
# 33. The other two platforms exit cleanly.
expected_status_x86_64=33
expected_status_aarch64=0
expected_status_riscv64=0

image_path() {
  case $1 in
    x86_64) printf '%s\n' boot/kernel/target/x86_64-unknown-none/release/agel-boot ;;
    aarch64) printf '%s\n' boot/kernel/target/aarch64-unknown-none-softfloat/release/agel-boot ;;
    riscv64) printf '%s\n' boot/kernel/target/riscv64imac-unknown-none-elf/release/agel-boot ;;
  esac
}

find_objdump() {
  if command -v gobjdump >/dev/null 2>&1; then
    command -v gobjdump
  elif test -x /opt/homebrew/opt/binutils/bin/gobjdump; then
    printf '%s\n' /opt/homebrew/opt/binutils/bin/gobjdump
  elif command -v llvm-objdump >/dev/null 2>&1; then
    command -v llvm-objdump
  elif command -v objdump >/dev/null 2>&1; then
    command -v objdump
  fi
}

# Prove the check itself works before trusting it: `.text` is full of calls, so
# a checker that passes on `.text` is a checker that would pass on anything.
self_check_user_text() {
  objdump_bin=$(find_objdump)
  test -n "$objdump_bin" || return 0
  probe=$(mktemp "${TMPDIR:-/tmp}/agel-selfcheck.XXXXXX")
  "$objdump_bin" -d --section=.text "$(image_path "$1")" > "$probe" 2>/dev/null || {
    rm -f "$probe"
    return 0
  }
  if ! grep -Eq '[[:space:]](call|callq|bl|blr|br|jal|jalr)[[:space:]]' "$probe"; then
    printf '%s\n' "$1: the .user_text self-containment check finds no calls even in .text" >&2
    rm -f "$probe"
    return 1
  fi
  rm -f "$probe"
}

check_user_text() {
  # The unprivileged program may only reach `.user_text`, the one range of the
  # image the page tables mark user-executable. A call or an indirect branch
  # would land in supervisor-only memory and fault, so reject it at build time
  # rather than discovering it as a mysterious fault at run time.
  objdump_bin=$(find_objdump)
  test -n "$objdump_bin" || return 0
  disassembly=$(mktemp "${TMPDIR:-/tmp}/agel-usertext.XXXXXX")
  if ! "$objdump_bin" -d --section=.user_text "$(image_path "$1")" > "$disassembly" 2>/dev/null; then
    rm -f "$disassembly"
    return 0
  fi
  outcome=0
  if grep -Eq '[[:space:]](call|callq|bl|blr|br|jal|jalr)[[:space:]]' "$disassembly"; then
    printf '%s\n' "$1: .user_text calls or branches out of itself" >&2
    outcome=1
  fi
  if grep -Eq '(jmp|jmpq)[[:space:]]+\*' "$disassembly"; then
    printf '%s\n' "$1: .user_text contains an indirect jump through a table" >&2
    outcome=1
  fi
  rm -f "$disassembly"
  return "$outcome"
}

run_architecture() {
  architecture=$1
  output_file=$(mktemp "${TMPDIR:-/tmp}/agel-isolation.XXXXXX")
  transcript_file=$(mktemp "${TMPDIR:-/tmp}/agel-transcript.XXXXXX")

  eval "expected=\$expected_status_$architecture"
  set +e
  "run_$architecture" < /dev/null > "$output_file" 2>&1
  status=$?
  set -e

  self_check_user_text "$architecture"
  check_user_text "$architecture"

  if test "$status" -ne "$expected"; then
    printf '%s\n' "$architecture: QEMU exited with $status, expected $expected" >&2
    cat "$output_file" >&2
    exit 1
  fi

  if ! grep -q 'AGEL_ISOLATION_OK' "$output_file"; then
    printf '%s\n' "$architecture: the isolation self-test did not report success" >&2
    cat "$output_file" >&2
    exit 1
  fi

  # The transcript the unprivileged world produced must equal the frozen
  # contract transcript byte for byte. One corpus, one reference model, and a
  # protection domain on each machine talking through a trap gate.
  tr -d '\r' < "$output_file" \
    | sed -n '/^---BEGIN AGEL CONTRACT TRANSCRIPT---$/,/^---END AGEL CONTRACT TRANSCRIPT---$/p' \
    | sed '1d;$d' > "$transcript_file"
  diff -u bootstrap/kernel-contract.trace "$transcript_file"

  # Every architecture must contain a world that writes to kernel memory, a
  # world that executes something it is not allowed to, and a world that never
  # yields. The exact fault names differ, and the report says which.
  grep -q "isolation\[$architecture\]: unprivileged corpus matches the reference model" "$output_file"
  grep -q "isolation\[$architecture\]: contained a world writing to kernel memory: page-fault" "$output_file"
  grep -q "isolation\[$architecture\]: contained a world executing an undefined instruction" "$output_file"
  grep -q "isolation\[$architecture\]: preempted a world that never yields" "$output_file"
  grep -q "isolation\[$architecture\]: contained a world touching a device it was not granted" \
    "$output_file"
  grep -q 'watchdog fault: rolled back to slot A' "$output_file"

  # Phase 3: the console driver is an unprivileged domain the supervisor can
  # lose and replace. The transcript diffed above was printed by it, so the
  # driver working is already load-bearing; these check the rest of the claim.
  grep -q "isolation\[$architecture\]: console driver in an unprivileged domain, generation 1" \
    "$output_file"
  grep -q "isolation\[$architecture\]: the console driver faulted" "$output_file"
  grep -q "isolation\[$architecture\]: replaced it; generation 2 after 1 restart" "$output_file"
  grep -q "isolation\[$architecture\]: a handle from generation 1 was refused: stale-generation" \
    "$output_file"
  grep -q "isolation\[$architecture\]: the replacement console driver is printing this line" \
    "$output_file"
  if grep -q 'this line must never appear' "$output_file"; then
    printf '%s\n' "$architecture: a stale handle printed anyway" >&2
    exit 1
  fi

  contained=$(grep -c "isolation\[$architecture\]: contained a world" "$output_file")
  if test "$contained" -lt 3; then
    printf '%s\n' "$architecture: only $contained containments reported" >&2
    exit 1
  fi

  printf '%s\n' "  $architecture: 81 contract steps printed by an unprivileged driver, $contained faults contained, 1 preemption, 1 driver restart [ok]"
  rm -f "$output_file" "$transcript_file"
}

if test "$#" -ge 1; then
  architectures=$*
else
  architectures="x86_64 aarch64 riscv64"
fi

for architecture in $architectures; do
  run_architecture "$architecture"
done

printf '%s\n' "Agel kernel-contract isolation: identical transcripts across $architectures [ok]"

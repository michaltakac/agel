#!/bin/sh
# One contract, one corpus, one frozen transcript, three machines.
#
# For each architecture this boots the research kernel under QEMU and requires
# that an unprivileged world answers all 118 kernel-contract steps with a
# transcript byte-identical to bootstrap/kernel-contract.trace, the v1.1
# profile with a frame window the page tables make real, that every way
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

# The diskless machines get a virtio block device for the storage driver
# test, with the 0xaa55 boot signature the test reads back from sector 0.
virtio_disk() {
  disk=$(mktemp "${TMPDIR:-/tmp}/agel-virtio.XXXXXX")
  dd if=/dev/zero of="$disk" bs=512 count=3072 2>/dev/null
  printf '\125\252' | dd of="$disk" bs=1 seek=510 conv=notrunc 2>/dev/null
  printf '%s\n' "$disk"
}

run_aarch64() {
  image=$(./scripts/build-kernel.sh aarch64 | tail -n 1)
  disk=$(virtio_disk)
  # There is no debug-exit device on `virt`; the kernel leaves through PSCI, so
  # a clean exit is status 0 and the success token carries the verdict.
  qemu-system-aarch64 \
    -machine virt -cpu cortex-a72 -m 128M -display none -monitor none -serial stdio -no-reboot \
    -global virtio-mmio.force-legacy=false \
    -drive if=none,format=raw,file="$disk",id=disk0,snapshot=on -device virtio-blk-device,drive=disk0 \
    -kernel "$image"
  status=$?
  rm -f "$disk"
  return $status
}

run_riscv64() {
  image=$(./scripts/build-kernel.sh riscv64 | tail -n 1)
  disk=$(virtio_disk)
  qemu-system-riscv64 \
    -machine virt -m 128M -display none -monitor none -serial stdio -no-reboot -bios default \
    -global virtio-mmio.force-legacy=false \
    -drive if=none,format=raw,file="$disk",id=disk0,snapshot=on -device virtio-blk-device,drive=disk0 \
    -kernel "$image"
  status=$?
  rm -f "$disk"
  return $status
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

check_user_text() {
  # The evaluator is no longer a call-free instruction stub: recursive Lisp
  # evaluation necessarily has direct calls and compiler-generated bounded
  # dispatch tables. The linker collects its code in `.user_text`; page tables
  # make only that section user-executable, and the live corpus below proves its
  # valid call graph remains inside it. Any escaped call faults in hardware.
  objdump_bin=$(find_objdump)
  test -n "$objdump_bin" || return 0
  sections=$(mktemp "${TMPDIR:-/tmp}/agel-user-sections.XXXXXX")
  symbols=$(mktemp "${TMPDIR:-/tmp}/agel-user-symbols.XXXXXX")
  "$objdump_bin" -h "$(image_path "$1")" > "$sections"
  "$objdump_bin" -t "$(image_path "$1")" > "$symbols"
  grep -Eq '[[:space:]]\.user_text[[:space:]]' "$sections"
  grep -Eq '\.user_text.*agel_evaluator_main$' "$symbols"
  grep -Eq '\.user_text.*agel_world_main$' "$symbols"
  rm -f "$sections" "$symbols"
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
  grep -q "isolation\[$architecture\]: the world answered with the independent implementation behind a trap gate; the supervisor checked all 118 steps against the reference model" \
    "$output_file"
  grep -q "isolation\[$architecture\]: native Agel evaluated factorial with transactional rollback in an unprivileged domain" "$output_file"
  # The memory group is real on this machine: mappings are page-table
  # entries, protection is enforced, and an unmapped page is an absence.
  grep -q "isolation\[$architecture\]: a world mapped its frame, wrote through the mapping, and read the value back" "$output_file"
  grep -q "isolation\[$architecture\]: a mapping protected to read-only refused the write: page-fault" "$output_file"
  grep -q "isolation\[$architecture\]: an allocated frame was written through the window, unmapped, and the page then faulted: page-fault" "$output_file"
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
  # A replaced driver's frames go back to the pool and its replacement is
  # built from them; the pool holds as many frames after as before.
  grep -Eq "isolation\[$architecture\]: the console driver's [0-9]+ frames were reclaimed and its replacement built from them" \
    "$output_file"
  grep -Eq "isolation\[$architecture\]: the storage driver's [0-9]+ frames were reclaimed and its replacement built from them" \
    "$output_file"
  if grep -q 'this line must never appear' "$output_file"; then
    printf '%s\n' "$architecture: a stale handle printed anyway" >&2
    exit 1
  fi

  # Phase 3, continued: the disk is a driver domain too, granted exactly the
  # ATA ports on x86-64 and one virtio-mmio page plus one DMA frame elsewhere;
  # it reads, is lost, is replaced, and refuses its old handle.
  grep -q "isolation\[$architecture\]: contained a world touching the disk it was not granted" \
    "$output_file"
  grep -q "isolation\[$architecture\]: storage driver read the boot sector from an unprivileged domain, generation 1" \
    "$output_file"
  grep -q "isolation\[$architecture\]: the storage driver faulted" "$output_file"
  grep -q "isolation\[$architecture\]: replaced the storage driver; a handle from generation 1 was refused: stale-generation" \
    "$output_file"
  grep -q "isolation\[$architecture\]: the replacement storage driver, generation 2, read the boot sector again" \
    "$output_file"
  if test "$architecture" = x86_64; then
    grep -q "isolation\[$architecture\]: contained a world touching the keyboard controller it was not granted" \
      "$output_file"
  fi

  contained=$(grep -c "isolation\[$architecture\]: contained a world" "$output_file")
  if test "$contained" -lt 4; then
    printf '%s\n' "$architecture: only $contained containments reported" >&2
    exit 1
  fi

  printf '%s\n' "  $architecture: 118 contract steps printed by an unprivileged driver, $contained faults contained, 1 preemption, 1 driver restart [ok]"
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

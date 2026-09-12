# Shared by the QEMU suites; each one sources it with `. ./scripts/lib.sh`
# from the repository root.

# prepare_machine ARCH TAG BLANK
#   Builds the `isolated-repl` kernel for ARCH and a temporary disk that is
#   removed at exit, and sets `$kernel` and `$disk`. x86-64 boots its BIOS
#   image, which is also the disk: a copy is taken so a test never touches
#   the developer's workshop, and BLANK sectors from sector 1024 (the
#   workspace slots, the records, and with 1024 the filesystem region) are
#   zeroed in the copy. AArch64 and RISC-V boot the ELF with a blank
#   1.5 MiB virtio disk attached.
prepare_machine() {
  disk=$(mktemp "${TMPDIR:-/tmp}/agel-$2.XXXXXX")
  trap 'rm -f "$disk"' EXIT HUP INT TERM
  case "$1" in
    x86_64)
      image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
      cp "$image" "$disk"
      dd if=/dev/zero of="$disk" bs=512 seek=1024 count="$3" conv=notrunc 2>/dev/null
      kernel=$disk
      ;;
    aarch64 | riscv64)
      kernel=$(./scripts/build-kernel.sh "$1" --features isolated-repl | tail -n 1)
      dd if=/dev/zero of="$disk" bs=512 count=3072 2>/dev/null
      ;;
    *) printf 'unknown architecture: %s\n' "$1" >&2; exit 2 ;;
  esac
}

# boot_x86_headless IMAGE OUTPUT [SECONDS]
#   Boots IMAGE on the headless x86-64 machine with the serial console in
#   OUTPUT, killed after SECONDS (default 15), and returns 0 when the guest
#   left cleanly: the debug-exit device maps its 0x10 to host status 33.
boot_x86_headless() {
  set +e
  perl -e 'alarm shift; exec @ARGV' "${3:-15}" qemu-system-x86_64 \
    -machine pc,accel=tcg -m 64M -display none -monitor none -serial stdio -no-reboot \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -boot order=c,strict=on \
    -drive format=raw,file="$1",snapshot=on < /dev/null > "$2" 2>&1
  status=$?
  set -e
  test "$status" -eq 33
}

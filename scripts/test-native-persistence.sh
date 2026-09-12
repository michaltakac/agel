#!/bin/sh
# Edit -> save -> reboot -> recover, on a temporary disk, on any of the three
# machines. x86-64 boots its BIOS image, which is also the disk; AArch64 and
# RISC-V boot the ELF with a virtio block device attached.
set -eu

architecture=${1:-x86_64}
. ./scripts/lib.sh
# The temporary copy starts with both workspace slots and the recovery
# record blank even if the developer's image already holds a workspace.
prepare_machine "$architecture" persistent 34
python3 ./scripts/test-native-repl.py "$kernel" --persistence --arch "$architecture" --disk "$disk"

#!/bin/sh
# Build the Agel kernel contract as a Microkit system on an unmodified seL4
# kernel, and print the path to the bootable image.
#
# The Microkit SDK is a 66 MiB third-party download and is deliberately not
# vendored. Point MICROKIT_SDK at an unpacked SDK, or let this script fetch and
# verify the pinned release into ./target/sel4.
#
# Nothing here builds seL4 itself. That is the point: the kernel is the one the
# seL4 Foundation published, byte for byte, and ./scripts/sel4-manifest.sh
# records which one.
set -eu

sdk_version=2.3.0
# SHA-256 of the published release archives, pinned so an unverified kernel
# cannot be substituted for the verified one.
sdk_sha256_macos_aarch64=f688ca8cc3545ee95681c509efc00cf212f79b75d919a9639371fff8fa51dc20
sdk_sha256_macos_x86_64=e301ec3ff2d86c754b7966ccb8585339b118fe9382059300bcf8e80addf92102
sdk_sha256_linux_x86_64=e12e507f72c87cbf5c514df9e9b0c66103b42298852f44c045d5729ea3de4f89
sdk_sha256_linux_aarch64=98a18bd5d90386c7a72a541083a8e1a3129ae83bf27d71c65759b36344718841

board=${MICROKIT_BOARD:-qemu_virt_aarch64}
config=${MICROKIT_CONFIG:-release}

project_dir=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
crate_dir="$project_dir/boot/microkit"
build_dir="$project_dir/target/sel4"
mkdir -p "$build_dir"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) platform=macos-aarch64; expected=$sdk_sha256_macos_aarch64 ;;
  Darwin-x86_64) platform=macos-x86-64; expected=$sdk_sha256_macos_x86_64 ;;
  Linux-x86_64) platform=linux-x86-64; expected=$sdk_sha256_linux_x86_64 ;;
  Linux-aarch64) platform=linux-aarch64; expected=$sdk_sha256_linux_aarch64 ;;
  *)
    printf '%s\n' "no Microkit SDK is published for $(uname -s)-$(uname -m)" >&2
    exit 2
    ;;
esac

if test -n "${MICROKIT_SDK:-}"; then
  sdk=$MICROKIT_SDK
else
  sdk="$build_dir/microkit-sdk-$sdk_version"
  if ! test -d "$sdk"; then
    archive="$build_dir/microkit-sdk-$sdk_version-$platform.tar.gz"
    url="https://github.com/seL4/microkit/releases/download/$sdk_version/microkit-sdk-$sdk_version-$platform.tar.gz"
    printf '%s\n' "fetching the Microkit $sdk_version SDK for $platform" >&2
    curl -sSL --fail -o "$archive" "$url"
    if command -v shasum >/dev/null 2>&1; then
      actual=$(shasum -a 256 "$archive" | cut -d' ' -f1)
    else
      actual=$(sha256sum "$archive" | cut -d' ' -f1)
    fi
    if test "$actual" != "$expected"; then
      printf '%s\n' "Microkit SDK checksum mismatch; refusing to use it" >&2
      printf '  expected %s\n  actual   %s\n' "$expected" "$actual" >&2
      rm -f "$archive"
      exit 1
    fi
    tar xzf "$archive" -C "$build_dir"
  fi
fi

board_dir="$sdk/board/$board/$config"
if ! test -d "$board_dir"; then
  printf '%s\n' "no such board configuration: $board_dir" >&2
  exit 1
fi

rustup target add aarch64-unknown-none-softfloat >/dev/null

# Each protection domain is linked by the SDK's own script against its prebuilt
# `libmicrokit`. The link arguments come last, so the archive resolves `_start`
# and `main` after the domain's own objects have already defined `init` and
# `notified`.
RUSTFLAGS="-C relocation-model=static \
 -C link-arg=-L$board_dir/lib \
 -C link-arg=-lmicrokit \
 -C link-arg=-T$board_dir/lib/microkit.ld"
export RUSTFLAGS

(cd "$crate_dir" && cargo build --release "$@")

# The system description names each program image with a `.elf` suffix, which
# is what the Microkit tool searches for; cargo does not add one.
domains="$build_dir/domains"
mkdir -p "$domains"
for domain in agel-serial agel-broker agel-recovery agel-world; do
  cp "$crate_dir/target/aarch64-unknown-none-softfloat/release/$domain" \
     "$domains/$domain.elf"
done

image="$build_dir/agel-sel4.img"
report="$build_dir/agel-sel4-report.txt"

"$sdk/bin/microkit" "$crate_dir/agel.system" \
  --search-path "$domains" \
  --board "$board" \
  --config "$config" \
  -o "$image" \
  -r "$report" >&2

printf '%s\n' "$image"

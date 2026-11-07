#!/bin/sh
# Boot Agel in QEMU's own graphical window.
# --workbench boots a separate persistent demo disk; --web adds the optional
# host-layout text bridge; --native (the default) uses QEMU's direct window.
# Named source cells remain on the same persistent disk across launches.
set -eu

usage() {
  cat <<'USAGE'
Usage: ./scripts/run-graphics.sh [--workbench] [--doom-demo] [--native | --web | --agent] [-- QEMU-ARGS...]
  --workbench  boot target/boot/agel-workbench.img, a separate persistent demo disk
  --doom-demo  include the optional DOOM programs and assets
  --native     QEMU's direct window and serial input (default; US physical layout)
  --web        also open the loopback browser console for host-layout text entry
  --agent      include the legacy demo and attach its host judge
               (TYPESAFEAI_API_KEY in the environment)
  --help       show this message
USAGE
}

workbench=false
web=false
agent=false
features=native-graphics
while test $# -gt 0; do
  case "$1" in
    --workbench) workbench=true ;;
    --doom-demo) features=doom-demo ;;
    --native) web=false ;;
    --web) web=true ;;
    --agent) agent=true; features=doom-demo ;;
    --help | -h) usage; exit 0 ;;
    --) shift; break ;;
    *) printf 'unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

image=$(./scripts/build-boot.sh --features "$features" | tail -n 1)
test -n "$image" && test -f "$image"
# The desktop's own persistent disk: the seed's boot sectors and assets
# refreshed on every run, the source workspace kept, and the programs a
# person wants at hand installed — the game, its data, the hosted runtime
# with the browser's site — so the agent can start them when asked.
desktop_image="$(dirname "$image")/agel-desktop.img"
test ! -L "$desktop_image"
if test ! -e "$desktop_image"; then
  cp "$image" "$desktop_image"
fi
dd if="$image" of="$desktop_image" bs=512 count=1 conv=notrunc 2>/dev/null
dd if="$image" of="$desktop_image" bs=512 skip=65536 seek=65536 count=4224 conv=notrunc 2>/dev/null
dd if="$image" of="$desktop_image" bs=512 skip=10240 seek=10240 count=3072 conv=notrunc 2>/dev/null
for program in boot/desktop/workbench:wb boot/desktop/builder:bd \
  examples/doom/doom-agent:da examples/doom/doom-agent-model:dm examples/doom/doom-agent-judge:dj \
  examples/doom/desktop-agent:dk examples/doom/review:rv examples/doom/lookup:lk; do
  case "$features:$program" in
    native-graphics:examples/doom/*)
      python3 ./scripts/install-program.py --region data "$desktop_image" --remove "${program##*:}.agel" >/dev/null ;;
    *)
      python3 ./scripts/install-program.py --region data "$desktop_image" "${program##*:}.agel" "${program%%:*}.agel" >/dev/null ;;
  esac
done
if test "$features" = doom-demo; then
  if doom=$(./scripts/build-c-program.sh doom x86_64 2>/dev/null | tail -n 1) && test -f "$doom" \
     && wad=$(./scripts/fetch-doom-wad.sh 2>/dev/null) && test -f "$wad"; then
    python3 ./scripts/install-program.py "$desktop_image" c-doom "$doom" >/dev/null
    python3 ./scripts/install-program.py --region data "$desktop_image" doom1.wad "$wad" >/dev/null
  else
    printf '%s\n' 'DOOM is not installed: build it with scripts/build-c-program.sh doom and fetch the WAD with scripts/fetch-doom-wad.sh' >&2
  fi
fi
if agel=$(./scripts/build-program.sh agel x86_64 2>/dev/null | tail -n 1) && test -f "$agel"; then
  python3 ./scripts/install-program.py "$desktop_image" agel "$agel" >/dev/null
  for page in examples/pages/*.html; do
    python3 ./scripts/install-program.py --region data "$desktop_image" "$(basename "$page")" "$page" >/dev/null
  done
  python3 ./scripts/install-program.py --region data "$desktop_image" browse.agel examples/browse-agent.agel >/dev/null
fi
image="$desktop_image"
if $workbench; then
  # Keep the user's existing workshop disk and this demo's source cells separate.
  workbench_image="$(dirname "$image")/agel-workbench.img"
  test ! -L "$workbench_image"
  if test ! -e "$workbench_image"; then
    dd if=/dev/zero of="$workbench_image" bs=512 count=73984 2>/dev/null
  fi
  test -f "$workbench_image"
  dd if="$image" of="$workbench_image" bs=512 count=1 conv=notrunc 2>/dev/null
  dd if="$image" of="$workbench_image" bs=512 skip=65536 seek=65536 count=4224 conv=notrunc 2>/dev/null
  # The asset region travels with the seed: the fonts the desktop is set in.
  dd if="$image" of="$workbench_image" bs=512 skip=10240 seek=10240 count=3072 conv=notrunc 2>/dev/null
  image="$workbench_image"
fi
if $web; then
  cargo build --release -q -p agel-jit --example module_workshop
  exec python3 ./scripts/graphical_console.py "$image" "$@"
fi
printf '%s\n' 'Direct QEMU input uses a US physical layout. Use --web for Slovak/macOS text composition.'
printf '%s\n' 'Click the desktop to open the workbench. The pointer is the host'"'"'s own: nothing is captured.'
if $agent; then
  # The judge beside the person: the console goes to a socket the bridge
  # attaches to, answering what the desktop asks and printing what it
  # says; the window stays the person's. Needs TYPESAFEAI_API_KEY.
  test -n "${TYPESAFEAI_API_KEY:-}" || { printf '%s\n' '--agent needs TYPESAFEAI_API_KEY in the environment (set -a; . ./.env; set +a)' >&2; exit 2; }
  cargo build --release -q -p agel-play
  sockets=$(mktemp -d "${TMPDIR:-/tmp}/agel-agent.XXXXXX")
  trap 'rm -rf "$sockets"' EXIT HUP INT TERM
  qemu-system-x86_64 \
    -machine pc,accel=tcg -m 64M -monitor none -no-reboot \
    -chardev socket,id=serial0,path="$sockets/serial",server=on,wait=off -serial chardev:serial0 \
    -vga std -boot order=c,strict=on \
    -drive format=raw,file="$image",if=ide,index=0,media=disk "$@" &
  qemu=$!
  ./target/release/agel-play --attach "$sockets/serial" --scene desktop --policy jev --out target/doom-runs/attached || true
  kill "$qemu" 2>/dev/null || true
  wait "$qemu" 2>/dev/null || true
  exit 0
fi
exec qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -monitor none -serial stdio -no-reboot \
  -vga std -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk "$@"

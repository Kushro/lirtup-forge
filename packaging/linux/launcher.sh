#!/bin/sh
# Launcher of LirtUp Forge, installed as both <prefix>/bin/lirtup and
# <prefix>/bin/lirtup-gui (the name picks the binary). The binaries live in
# <prefix>/lib/lirtup/, next to the GUI's assets/ folder (where dx's asset
# resolver looks: the folder of the executable) and next to each other (the
# GUI starts `lirtup serve` from its own folder). Paths are relative to this
# script, so the same file works in /usr and inside the AppImage.
set -eu

self=$(readlink -f -- "$0")
prefix=$(dirname -- "$(dirname -- "$self")")
name=$(basename -- "$0")
case "$name" in
    lirtup-gui*) bin=lirtup-gui ;;
    *) bin=lirtup ;;
esac

# The MagpieFX effects shipped with the package (catalog::builtin_dir).
# Without this, a binary built from a source tree that still exists would
# read the effects of that tree.
if [ -z "${LIRTUP_EFFECTS_DIR:-}" ] && [ -d "$prefix/share/lirtup/effects" ]; then
    LIRTUP_EFFECTS_DIR="$prefix/share/lirtup/effects"
    export LIRTUP_EFFECTS_DIR
fi

if [ "$bin" = lirtup-gui ]; then
    # WebKitGTK draws a blank window with its DMA-BUF renderer on some
    # drivers (NVIDIA, some Wayland compositors). Set it to 0 to try it.
    : "${WEBKIT_DISABLE_DMABUF_RENDERER:=1}"
    export WEBKIT_DISABLE_DMABUF_RENDERER
fi

exec "$prefix/lib/lirtup/$bin" "$@"

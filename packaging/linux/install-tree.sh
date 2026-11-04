#!/bin/sh
# Installs a built LirtUp Forge into DESTDIR/PREFIX: the layout shared by the
# AUR package (DESTDIR=$pkgdir, PREFIX=/usr) and the AppImage
# (DESTDIR=AppDir, PREFIX=/usr).
#
#   PREFIX/bin/lirtup, lirtup-gui      the launcher (packaging/linux/launcher.sh)
#   PREFIX/lib/lirtup/lirtup           the CLI and scaler (cargo build -p lirtup-cli)
#   PREFIX/lib/lirtup/lirtup-gui       the GUI (dx build -p lirtup-ui)
#   PREFIX/lib/lirtup/assets/          the GUI's assets, from the dx bundle
#   PREFIX/share/lirtup/effects/       the MagpieFX effects (catalog::builtin_dir)
#   PREFIX/share/lirtup/magpiefx-seed/ their precompiled SPIR-V, when built
#                                      (paths::magpiefx_seed_dir)
#   PREFIX/share/applications, metainfo, icons/hicolor
#
# The shaders (.spv) and the CuNNy models are compiled into the binaries
# (include_bytes! in lirtup-effects and lirtup-gpu), so only the MagpieFX
# effects, which are read at run time, are installed as data.
#
# Usage: install-tree.sh DESTDIR PREFIX
# Environment (defaults from CARGO_TARGET_DIR, else ./target):
#   LIRTUP_CLI_BIN   the lirtup binary
#   LIRTUP_GUI_APP   the dx bundle folder (with lirtup-gui and assets/)
#   LIRTUP_EFFECTS   the effects folder of the source tree
#   LIRTUP_SEED      the MagpieFX seed written by
#                    `cargo run --release -p lirtup-magpiefx --bin magpiefx-seed -- --out DIR`
#                    (default build/magpiefx-seed; skipped with a warning when
#                    missing, without one when set to the empty string)
set -eu

if [ $# -ne 2 ]; then
    echo "usage: $0 DESTDIR PREFIX" >&2
    exit 2
fi
destdir=$1
prefix=$2

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target}

cli=${LIRTUP_CLI_BIN:-$target/release/lirtup}
gui_app=${LIRTUP_GUI_APP:-$target/dx/lirtup-gui/release/linux/app}

# The built-in MagpieFX effects (LIRTUP_EFFECTS overrides the folder).
LIRTUP_EFFECTS=${LIRTUP_EFFECTS:-$root/crates/lirtup-magpiefx/effects}
effects=${LIRTUP_EFFECTS:-}

# The precompiled MagpieFX seed (optional: without it effects compile on
# first use). Unset: the default folder; empty: none.
seed=${LIRTUP_SEED-$root/build/magpiefx-seed}

fail() {
    echo "install-tree: $*" >&2
    exit 1
}
[ -x "$cli" ] || fail "no lirtup binary at $cli (cargo build --release -p lirtup-cli)"
[ -x "$gui_app/lirtup-gui" ] || fail "no dx bundle at $gui_app (dx build --release -p lirtup-ui)"
[ -d "$gui_app/assets" ] || fail "the dx bundle at $gui_app has no assets/"
[ -n "$effects" ] && [ -d "$effects" ] || fail "no MagpieFX effects folder (set LIRTUP_EFFECTS)"

app_id=io.github.baronsmv.linux-rt-upscaler
dest=$destdir$prefix

install -Dm755 "$cli" "$dest/lib/lirtup/lirtup"
install -Dm755 "$gui_app/lirtup-gui" "$dest/lib/lirtup/lirtup-gui"
mkdir -p "$dest/lib/lirtup/assets"
cp -R "$gui_app/assets/." "$dest/lib/lirtup/assets/"

install -Dm755 "$here/launcher.sh" "$dest/bin/lirtup"
install -Dm755 "$here/launcher.sh" "$dest/bin/lirtup-gui"

mkdir -p "$dest/share/lirtup/effects"
cp -R "$effects/." "$dest/share/lirtup/effects/"

if [ -z "$seed" ]; then
    :
elif [ -d "$seed" ] && [ -n "$(ls -A "$seed")" ]; then
    mkdir -p "$dest/share/lirtup/magpiefx-seed"
    cp -R "$seed/." "$dest/share/lirtup/magpiefx-seed/"
else
    echo "install-tree: warning: no MagpieFX seed at $seed; effects will compile on first use" >&2
fi

install -Dm644 "$here/$app_id.desktop" "$dest/share/applications/$app_id.desktop"
install -Dm644 "$here/$app_id.metainfo.xml" "$dest/share/metainfo/$app_id.metainfo.xml"
(
    cd "$root/data/icons"
    find hicolor -type f -name "$app_id.*" | while read -r icon; do
        install -Dm644 "$icon" "$dest/share/icons/$icon"
    done
)

# Files and folders readable by everyone, whatever the umask of the build.
find "$dest/lib/lirtup/assets" "$dest/share/lirtup" -type d -exec chmod 755 {} +
find "$dest/lib/lirtup/assets" "$dest/share/lirtup" -type f -exec chmod 644 {} +

#!/usr/bin/env bash
# Builds the Linux release of LirtUp Forge:
#   - lirtup (cargo) and lirtup-gui (dx, with the Tailwind of package.json);
#   - lirtup-forge-<version>-linux-x86_64.tar.gz: the /usr tree of
#     packaging/linux/install-tree.sh (bin/, lib/lirtup/, share/);
#   - LirtUp_Forge-<version>-x86_64.AppImage: the same tree in an AppDir.
#
# Usage: scripts/build-release-linux.sh [options]
#   --out DIR        where the artifacts go (default: $CARGO_TARGET_DIR/release-linux,
#                    or target/release-linux)
#   --skip-build     package what is already built
#   --no-appimage    only the tarball
#   --fetch-tools    download appimagetool 1.9.1 (checked against its sha256)
#                    when it is not installed
#
# Needs: cargo, dx (the version of dioxus in Cargo.lock), npm, and for the
# AppImage appimagetool (or --fetch-tools) and patchelf (to bundle libxdo).
# Environment: CARGO_TARGET_DIR, APPIMAGETOOL (path to appimagetool).
# The source tree is not modified: node_modules/ is ignored by git, and
# packaging/tools/tailwindcss makes dx write the committed tailwind.css.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

out=
skip_build=0
appimage=1
fetch_tools=0
while [[ $# -gt 0 ]]; do
    case $1 in
        --out) out=$2; shift 2 ;;
        --skip-build) skip_build=1; shift ;;
        --no-appimage) appimage=0; shift ;;
        --fetch-tools) fetch_tools=1; shift ;;
        -h|--help) sed -n '2,/^set -euo/{/^set -euo/d;s/^# \{0,1\}//;p}' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/target}
out=${out:-$CARGO_TARGET_DIR/release-linux}
work=$CARGO_TARGET_DIR/release-linux-work
mkdir -p "$out" "$work"

version=$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"$/\1/p}' Cargo.toml)
dioxus=$(sed -n '/^name = "dioxus"$/{n;s/^version = "\(.*\)"$/\1/p;q}' Cargo.lock)
arch=$(uname -m)

step() { printf '\n==> %s\n' "$*"; }
die() { echo "error: $*" >&2; exit 1; }

if [[ $skip_build == 0 ]]; then
    have=$(dx --version 2>/dev/null | awk '{print $2}') || true
    [[ $have == "$dioxus" ]] ||
        die "dx ${have:-not found}; install dioxus-cli $dioxus (cargo install dioxus-cli --version $dioxus --locked)"

    step "Tailwind of package.json (npm ci)"
    npm ci --prefix crates/lirtup-ui --no-audit --no-fund

    step "lirtup $version (cargo)"
    cargo build --locked --release -p lirtup-cli

    step "lirtup-gui (dx $dioxus)"
    NO_DOWNLOADS=1 PATH="$root/packaging/tools:$PATH" dx build --release -p lirtup-ui --desktop
fi

gui_app=$CARGO_TARGET_DIR/dx/lirtup-gui/release/linux/app

step "Tarball"
tree=$work/tree
rm -rf "$tree"
LIRTUP_GUI_APP=$gui_app sh packaging/linux/install-tree.sh "$tree" /usr
install -Dm644 LICENSE "$tree/usr/share/licenses/lirtup-forge/LICENSE"
tarball=$out/lirtup-forge-$version-linux-$arch.tar.gz
tar -C "$tree" -czf "$tarball" --owner=0 --group=0 usr
echo "$tarball"

[[ $appimage == 1 ]] || exit 0

step "AppImage"
appdir=$work/AppDir
rm -rf "$appdir"
cp -a "$tree" "$appdir"
app_id=io.github.baronsmv.linux-rt-upscaler
install -m755 packaging/appimage/AppRun "$appdir/AppRun"
cp "packaging/linux/$app_id.desktop" "$appdir/$app_id.desktop"
cp "data/icons/hicolor/256x256/apps/$app_id.png" "$appdir/$app_id.png"
ln -sf "$app_id.png" "$appdir/.DirIcon"

# libxdo is libxdo.so.3 on Debian and Ubuntu and libxdo.so.4 on Arch: bundle
# the one the GUI links, found through $ORIGIN.
xdo=$(ldd "$appdir/usr/lib/lirtup/lirtup-gui" | awk '/libxdo\.so/ {print $3}')
if [[ -n $xdo ]]; then
    if command -v patchelf >/dev/null; then
        install -m644 "$xdo" "$appdir/usr/lib/lirtup/"
        patchelf --set-rpath '$ORIGIN' "$appdir/usr/lib/lirtup/lirtup-gui"
    else
        echo "warning: patchelf not found; $(basename "$xdo") is not bundled" >&2
    fi
fi

tool=${APPIMAGETOOL:-$(command -v appimagetool || true)}
if [[ -z $tool ]]; then
    [[ $fetch_tools == 1 ]] || die "appimagetool not found (install it, set APPIMAGETOOL or pass --fetch-tools)"
    tool=$work/appimagetool-1.9.1-x86_64.AppImage
    if [[ ! -x $tool ]]; then
        curl -fL -o "$tool.part" \
            https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
        echo "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  $tool.part" | sha256sum -c -
        chmod +x "$tool.part"
        mv "$tool.part" "$tool"
    fi
fi

image=$out/LirtUp_Forge-$version-$arch.AppImage
# Without FUSE (containers, CI) appimagetool runs extracted.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$arch "$tool" --no-appstream "$appdir" "$image"
echo "$image"

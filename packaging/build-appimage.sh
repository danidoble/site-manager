#!/usr/bin/env bash
# Build a portable AppImage for Local Site Manager (GUI entry point).
#
# Downloads linuxdeploy + appimagetool on first run (needs network).
# Output:  packaging/dist/local-site-manager-<version>-x86_64.AppImage
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([0-9.]+)".*/\1/')"
DIST="$ROOT/packaging/dist"
TOOLS="$ROOT/packaging/.tools"
APPDIR="$ROOT/packaging/.AppDir"

mkdir -p "$DIST" "$TOOLS"

# ---- tools ---------------------------------------------------------------
fetch() { # url dest
    if [ ! -x "$2" ]; then
        echo "==> Downloading $1"
        curl -fsSL "$1" -o "$2"
        chmod +x "$2"
    fi
}
LINUXDEPLOY="$TOOLS/linuxdeploy-x86_64.AppImage"
APPIMAGETOOL="$TOOLS/appimagetool-x86_64.AppImage"
fetch "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage" "$LINUXDEPLOY"
fetch "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage" "$APPIMAGETOOL"

# Allow AppImages to run where FUSE is unavailable.
export APPIMAGE_EXTRACT_AND_RUN=1
export NO_STRIP=true

echo "==> Building release binaries"
cargo build --release --workspace

echo "==> Preparing AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/metainfo" "$APPDIR/usr/share/polkit-1/actions"

install -m 0755 target/release/local-site-manager-gui      "$APPDIR/usr/bin/"
install -m 0755 target/release/site-manager                "$APPDIR/usr/bin/"
install -m 0755 target/release/local-site-manager-api      "$APPDIR/usr/bin/"
install -m 0755 target/release/local-site-manager-privileged "$APPDIR/usr/bin/"
install -m 0644 packaging/local-site-manager.desktop       "$APPDIR/usr/share/applications/"
install -m 0644 packaging/local-site-manager.metainfo.xml  "$APPDIR/usr/share/metainfo/"
install -m 0644 assets/polkit/local.lsm.policy             "$APPDIR/usr/share/polkit-1/actions/"

echo "==> Running linuxdeploy"
"$LINUXDEPLOY" \
    --appdir "$APPDIR" \
    --desktop-file packaging/local-site-manager.desktop \
    --icon-file packaging/icons/512/local-site-manager.png \
    --executable target/release/local-site-manager-gui \
    --executable target/release/site-manager \
    --executable target/release/local-site-manager-api \
    --executable target/release/local-site-manager-privileged

OUT="$DIST/local-site-manager-${VERSION}-x86_64.AppImage"
echo "==> Packing AppImage -> $OUT"
rm -f "$OUT"
"$APPIMAGETOOL" "$APPDIR" "$OUT"

echo "==> Result:"
ls -lh "$OUT"
echo
echo "Run with:  ./$OUT"

#!/usr/bin/env bash
# Build the .deb package for Local Site Manager.
#
# Prereq:  cargo install cargo-deb
# Output:  target/debian/local-site-manager_<version>-1_amd64.deb
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"

if ! command -v cargo-deb >/dev/null 2>&1; then
    echo "cargo-deb not found. Install with:  cargo install cargo-deb"
    exit 1
fi

echo "==> Building release binaries"
cargo build --release --workspace

echo "==> Building .deb"
cargo deb -p lsm-gui --no-build

echo "==> Result:"
ls -lh target/debian/*.deb
echo
echo "Install with:"
echo "  sudo apt install ./target/debian/local-site-manager_*.deb"

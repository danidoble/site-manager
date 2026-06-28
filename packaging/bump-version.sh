#!/usr/bin/env bash
# Update release version numbers before committing and tagging.
#
# Usage:
#   ./packaging/bump-version.sh 2.0.1
#   ./packaging/bump-version.sh 2.0.1 2
#
# The first argument is the upstream app version.
# The optional second argument is the Debian package revision.
set -euo pipefail

cd "$(dirname "$0")/.."

VERSION="${1:-}"
DEB_REVISION="${2:-1}"
TODAY="$(date +%F)"

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z]+)?$ ]]; then
    echo "usage: $0 <version> [debian-revision]" >&2
    echo "example: $0 2.0.1" >&2
    exit 2
fi

if [[ ! "$DEB_REVISION" =~ ^[0-9]+([.+~][0-9A-Za-z]+)*$ ]]; then
    echo "invalid Debian revision: $DEB_REVISION" >&2
    exit 2
fi

echo "==> Updating Cargo workspace version to $VERSION"
sed -i -E "0,/^version = \".*\"/s//version = \"$VERSION\"/" Cargo.toml

echo "==> Updating embedded app info"
if [[ -f assets/app-info.json ]]; then
    sed -i -E "s/\"version\": \"[^\"]+\"/\"version\": \"$VERSION\"/" assets/app-info.json
fi

echo "==> Updating Debian revision to $DEB_REVISION"
if grep -q '^revision = ' crates/lsm-gui/Cargo.toml; then
    sed -i -E "s/^revision = \".*\"/revision = \"$DEB_REVISION\"/" crates/lsm-gui/Cargo.toml
else
    sed -i '/^depends = /a revision = "'"$DEB_REVISION"'"' crates/lsm-gui/Cargo.toml
fi

echo "==> Updating AppStream metainfo release"
if grep -q '<release version="' packaging/local-site-manager.metainfo.xml; then
    sed -i -E \
        "0,/<release version=\"[^\"]+\" date=\"[^\"]+\">/s//<release version=\"$VERSION\" date=\"$TODAY\">/" \
        packaging/local-site-manager.metainfo.xml
fi

echo "==> Updating README package examples"
sed -i -E \
    "s/local-site-manager_[0-9][0-9A-Za-z.+:~.-]*-[0-9A-Za-z.+~]+_amd64\\.deb/local-site-manager_${VERSION}-${DEB_REVISION}_amd64.deb/g" \
    README.md
sed -i -E \
    "s/local-site-manager-[0-9][0-9A-Za-z.+:~.-]*-x86_64\\.AppImage/local-site-manager-${VERSION}-x86_64.AppImage/g" \
    README.md

echo "==> Refreshing Cargo.lock"
cargo metadata --no-deps --format-version 1 >/dev/null

cat <<EOF

Version updated.

Next steps:
  git diff
  git add Cargo.toml Cargo.lock assets/app-info.json crates/lsm-gui/Cargo.toml packaging/local-site-manager.metainfo.xml README.md
  git commit -m "Release v$VERSION"
  git tag v$VERSION
  git push && git push origin v$VERSION

Expected Debian package:
  target/debian/local-site-manager_${VERSION}-${DEB_REVISION}_amd64.deb
EOF

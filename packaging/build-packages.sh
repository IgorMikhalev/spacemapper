#!/usr/bin/env bash
# Builds the release files into dist/:
#   spacemap-VERSION-x86_64.tar.gz           any distribution
#   spacemap_VERSION-1_amd64.deb             Debian, Ubuntu, Mint, Pop!_OS
#   spacemap-VERSION-1.x86_64.rpm            Fedora, openSUSE, RHEL
#   spacemap-VERSION-1-x86_64.pkg.tar.zst    Arch, CachyOS, EndeavourOS, Manjaro
#
# Requirements: cargo, cargo-zigbuild + zig (for a binary that runs on
# glibc 2.28 and newer), cargo-generate-rpm, dpkg-deb, python3 + zstandard.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
TARGET=x86_64-unknown-linux-gnu
BIN=target/$TARGET/release/spacemap
DIST=dist
rm -rf "$DIST"
mkdir -p "$DIST"

echo "==> Building spacemap $VERSION (glibc 2.28+)"
cargo zigbuild --release --locked --target "$TARGET.2.28"

# ---------------------------------------------------------------- file tree
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
install -Dm755 "$BIN" "$STAGE/usr/bin/spacemap"
install -Dm644 packaging/spacemap.desktop "$STAGE/usr/share/applications/spacemap.desktop"
install -Dm644 packaging/io.github.martin_ii.spacemap.metainfo.xml \
  "$STAGE/usr/share/metainfo/io.github.martin_ii.spacemap.metainfo.xml"
for s in 32 48 64 128 256 512; do
  install -Dm644 "assets/icon/spacemap-$s.png" "$STAGE/usr/share/icons/hicolor/${s}x${s}/apps/spacemap.png"
done
install -Dm644 README.md "$STAGE/usr/share/doc/spacemap/README.md"
install -Dm644 LICENSE "$STAGE/usr/share/licenses/spacemap/LICENSE"
install -Dm644 assets/Inter-LICENSE.txt "$STAGE/usr/share/licenses/spacemap/Inter-LICENSE.txt"

# ---------------------------------------------------------------- tar.gz
echo "==> tar.gz"
T="$STAGE/tar/spacemap-$VERSION"
mkdir -p "$T"
cp "$BIN" "$T/spacemap"
cp packaging/spacemap.desktop README.md LICENSE "$T/"
mkdir -p "$T/icons"
cp assets/icon/spacemap-*.png "$T/icons/"
cp packaging/install.sh "$T/install.sh"
chmod 755 "$T/install.sh" "$T/spacemap"
tar -C "$STAGE/tar" --owner=0 --group=0 -czf "$DIST/spacemap-$VERSION-x86_64.tar.gz" "spacemap-$VERSION"

# ---------------------------------------------------------------- deb
echo "==> deb"
D="$STAGE/deb"
mkdir -p "$D/DEBIAN"
cp -a "$STAGE/usr" "$D/"
mkdir -p "$D/usr/share/doc/spacemap"
cat > "$D/usr/share/doc/spacemap/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: spacemap
Source: https://github.com/Martin-ii/spacemap

Files: *
Copyright: $(date +%Y) Martin Fleisch
License: GPL-3.0-or-later
 On Debian systems, the full text of the GNU General Public License
 version 3 can be found in /usr/share/common-licenses/GPL-3.
EOF
SIZE=$(du -sk "$D/usr" | cut -f1)
cat > "$D/DEBIAN/control" <<EOF
Package: spacemap
Version: $VERSION-1
Section: utils
Priority: optional
Architecture: amd64
Maintainer: Martin Fleisch <https://github.com/Martin-ii>
Installed-Size: $SIZE
Depends: libc6 (>= 2.28), libgl1, libegl1, libxkbcommon0, libwayland-client0
Recommends: libxkbcommon-x11-0, libxcursor1, libxi6, xdg-utils, pkexec | policykit-1
Homepage: https://github.com/Martin-ii/spacemap
Description: Treemap disk usage viewer
 Spacemap shows the contents of a drive or folder as a treemap. Each file
 and folder is drawn as a box sized by the space it uses. It includes a
 sortable list of the largest files and file types, filters, and an option
 to scan system folders as administrator. Spacemap does not change or
 delete files.
EOF
find "$D" -type d -exec chmod 755 {} +
dpkg-deb --root-owner-group -Zxz --build "$D" "$DIST/spacemap_${VERSION}-1_amd64.deb" >/dev/null

# ---------------------------------------------------------------- rpm
echo "==> rpm"
cargo generate-rpm --target "$TARGET" -o "$DIST/spacemap-$VERSION-1.x86_64.rpm"

# ---------------------------------------------------------------- arch
echo "==> pkg.tar.zst"
python3 packaging/make-arch-pkg.py "$STAGE" "$VERSION" "$DIST/spacemap-$VERSION-1-x86_64.pkg.tar.zst"

# ---------------------------------------------------------------- checksums
(cd "$DIST" && sha256sum -- * > SHA256SUMS)
echo "==> Done"
ls -l "$DIST"

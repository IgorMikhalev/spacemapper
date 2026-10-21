#!/bin/sh
# Installs Spacemap from the extracted tar.gz.
#
#   ./install.sh            install for the current user (~/.local)
#   sudo ./install.sh       install for all users (/usr/local)
#   ./install.sh --remove   uninstall (use sudo if it was installed with sudo)
set -e
cd "$(dirname "$0")"

if [ "$(id -u)" -eq 0 ]; then
    PREFIX=/usr/local
else
    PREFIX="$HOME/.local"
fi
BIN="$PREFIX/bin"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor"

if [ "$1" = "--remove" ]; then
    rm -f "$BIN/spacemap" "$APPS/spacemap.desktop"
    for s in 32 48 64 128 256 512; do
        rm -f "$ICONS/${s}x${s}/apps/spacemap.png"
    done
    echo "Spacemap removed from $PREFIX"
    exit 0
fi

install -Dm755 spacemap "$BIN/spacemap"
sed "s|^Exec=spacemap|Exec=$BIN/spacemap|" spacemap.desktop > spacemap.desktop.tmp
install -Dm644 spacemap.desktop.tmp "$APPS/spacemap.desktop"
rm -f spacemap.desktop.tmp
for s in 32 48 64 128 256 512; do
    install -Dm644 "icons/spacemap-$s.png" "$ICONS/${s}x${s}/apps/spacemap.png"
done
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" >/dev/null 2>&1 || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -qf "$ICONS" >/dev/null 2>&1 || true

echo "Spacemap installed to $PREFIX"
echo "Start it from your application menu or run: $BIN/spacemap"

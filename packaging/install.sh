#!/bin/sh
# Installs Spacemapper from the extracted tar.gz.
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
    rm -f "$BIN/spacemapper" "$APPS/spacemapper.desktop"
    for s in 32 48 64 128 256 512; do
        rm -f "$ICONS/${s}x${s}/apps/spacemapper.png"
    done
    echo "Spacemapper removed from $PREFIX"
    exit 0
fi

install -Dm755 spacemapper "$BIN/spacemapper"
sed "s|^Exec=spacemapper|Exec=$BIN/spacemapper|" spacemapper.desktop > spacemapper.desktop.tmp
install -Dm644 spacemapper.desktop.tmp "$APPS/spacemapper.desktop"
rm -f spacemapper.desktop.tmp
for s in 32 48 64 128 256 512; do
    install -Dm644 "icons/spacemapper-$s.png" "$ICONS/${s}x${s}/apps/spacemapper.png"
done
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" >/dev/null 2>&1 || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -qf "$ICONS" >/dev/null 2>&1 || true

echo "Spacemapper installed to $PREFIX"
echo "Start it from your application menu or run: $BIN/spacemapper"

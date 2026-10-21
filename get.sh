#!/bin/sh
# Installs or updates Spacemapper from the latest GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/Martin-ii/spacemapper/main/get.sh | sh
#
# Picks the package for your system (Arch, Debian/Ubuntu, Fedora, openSUSE),
# or installs the portable version to ~/.local on other distributions.
# Running it again updates to the newest version. To install a specific
# version instead:  curl -fsSL .../get.sh | SPACEMAPPER_VERSION=0.5.0 sh
set -eu

REPO="Martin-ii/spacemapper"

say() { printf '%s\n' "$*"; }
die() { printf 'Error: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl is needed. Install it with your package manager and run this again."
[ "$(uname -m)" = "x86_64" ] || die "Spacemapper is only built for x86_64 (64-bit Intel/AMD) at the moment."

# run as root: directly if we are root, else through sudo or doas.
# Package managers read their [Y/n] question from the terminal, not from the
# pipe this script arrives through.
as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@" </dev/tty
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@" </dev/tty
    elif command -v doas >/dev/null 2>&1; then
        doas "$@" </dev/tty
    else
        die "sudo or doas is needed to install system packages."
    fi
}

if [ -n "${SPACEMAPPER_VERSION:-}" ]; then
    # a specific version, e.g. SPACEMAPPER_VERSION=0.5.0
    tag="v${SPACEMAPPER_VERSION#v}"
else
    say "Looking up the latest Spacemapper release..."
    # github.com/<repo>/releases/latest redirects to .../releases/tag/vX.Y.Z
    latest=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest") \
        || die "could not reach GitHub."
    tag=${latest##*/}
fi
case "$tag" in
    v[0-9]*) ;;
    *) die "could not find a release (got: $tag)." ;;
esac
ver=${tag#v}
base="https://github.com/$REPO/releases/download/$tag"

if command -v pacman >/dev/null 2>&1; then
    kind="pacman"; name="spacemapper-$ver-1-x86_64.pkg.tar.zst"
elif command -v apt-get >/dev/null 2>&1; then
    kind="apt"; name="spacemapper_${ver}-1_amd64.deb"
elif command -v dnf >/dev/null 2>&1; then
    kind="dnf"; name="spacemapper-$ver-1.x86_64.rpm"
elif command -v zypper >/dev/null 2>&1; then
    kind="zypper"; name="spacemapper-$ver-1.x86_64.rpm"
else
    kind="portable"; name="spacemapper-$ver-x86_64.tar.gz"
fi
url="$base/$name"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
chmod 755 "$tmp"
file="$tmp/$name"
say "Downloading Spacemapper $ver ($name)..."
curl -fL --progress-bar -o "$file" "$url" || die "download failed."

case "$kind" in
    pacman) as_root pacman -U "$file" ;;
    apt)    as_root apt-get install "$file" ;;
    dnf)    as_root dnf install "$file" ;;
    zypper) as_root zypper install --allow-unsigned-rpm "$file" ;;
    portable)
        tar xzf "$file" -C "$tmp"
        sh "$tmp"/spacemapper-*/install.sh
        ;;
esac

say ""
say "Spacemapper $ver is installed. Start it from your application menu, or run: spacemapper"

<p align="center">
  <img src="assets/icon/spacemapper-128.png" width="96" alt="">
</p>

<h1 align="center">Spacemapper</h1>

<p align="center">Disk usage viewer for Linux that shows your files as a treemap.</p>

<p align="center">
  <a href="https://github.com/Martin-ii/spacemapper/releases/latest">Download</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#building-from-source">Building</a>
</p>

![Spacemapper showing a home folder](docs/screenshot-main.png)

Spacemapper scans a drive or folder and draws every file and folder as a box. The size of a box matches the space it takes up, so large files and folders stand out right away. Folders contain their subfolders and files, and you can zoom in and out to look at any part in more detail.

It works like SpaceSniffer on Windows. Spacemapper only reads file information: it has no option to delete, move or rename files, so you can look around without risk.

## Features

- Treemap of a whole drive or a single folder, zoomable with mouse or keyboard
- Side list with three views: folder contents, largest files, and space per file type
- Sorting by size, number of files, name or modification date
- Filters by name, path, size and age, with ready-made filters for large files, old files, videos, images, audio, archives, caches, build folders, package caches and trash
- Colors by top-level folder, file type or age, or plain grayscale
- Free space and unreadable space shown when scanning a whole drive
- Full access option to include system folders (asks for your administrator password)
- Open files or show them in your file manager from the right-click menu
- Fast multithreaded scanning; sizes match `du`
- Runs on Wayland and X11

| File types | Filter | Colored by age |
|---|---|---|
| ![Space per file type](docs/screenshot-types.png) | ![Filter for video files](docs/screenshot-filter.png) | ![Files colored by modification date](docs/screenshot-age.png) |

## Installation

Packages for the most common distributions are on the [Releases](https://github.com/Martin-ii/spacemapper/releases/latest) page. They work on 64-bit (x86_64) systems with glibc 2.28 or newer, which covers every distribution released since 2019.

### AppImage (any distribution, no installation)

Download `Spacemapper-VERSION-x86_64.AppImage`, make it executable and run it:

```sh
chmod +x Spacemapper-*-x86_64.AppImage
./Spacemapper-*-x86_64.AppImage
```

You can also right-click the file in your file manager, open Properties, allow it to run as a program, and double-click it. Nothing is installed; to remove Spacemapper, delete the file.

If it does not start and mentions FUSE, install FUSE 2 (`sudo apt install libfuse2` on Debian and Ubuntu, `sudo dnf install fuse-libs` on Fedora, `sudo pacman -S fuse2` on Arch), or start it with `--appimage-extract-and-run` added after the file name.

### Debian, Ubuntu, Linux Mint, Pop!_OS, elementary OS, Zorin OS

Download `spacemapper_VERSION-1_amd64.deb`, then run:

```sh
sudo apt install ./spacemapper_*_amd64.deb
```

You can also double-click the file to open it in your software center.

To uninstall: `sudo apt remove spacemapper`

### Fedora, RHEL, Rocky Linux, AlmaLinux

Download `spacemapper-VERSION-1.x86_64.rpm`, then run:

```sh
sudo dnf install ./spacemapper-*.x86_64.rpm
```

To uninstall: `sudo dnf remove spacemapper`

### openSUSE

Download `spacemapper-VERSION-1.x86_64.rpm`, then run:

```sh
sudo zypper install --allow-unsigned-rpm ./spacemapper-*.x86_64.rpm
```

To uninstall: `sudo zypper remove spacemapper`

### Arch Linux, CachyOS, EndeavourOS, Manjaro

Download `spacemapper-VERSION-1-x86_64.pkg.tar.zst`, then run:

```sh
sudo pacman -U spacemapper-*-x86_64.pkg.tar.zst
```

To uninstall: `sudo pacman -R spacemapper`

To build the package yourself instead, see [Arch package from source](#arch-package-from-source).

### Any other distribution

Download `spacemapper-VERSION-x86_64.tar.gz` and extract it:

```sh
tar xzf spacemapper-*-x86_64.tar.gz
cd spacemapper-*/
```

You can run it straight from this folder with `./spacemapper`. To add it to your application menu:

```sh
./install.sh          # for your user only, installs to ~/.local
sudo ./install.sh     # for all users, installs to /usr/local
```

To uninstall, run the same command with `--remove`, for example `./install.sh --remove`.

### Requirements

The packages above install these automatically. With the tar.gz you need:

- OpenGL (Mesa or your graphics driver) and EGL
- libxkbcommon and libwayland-client
- For X11 sessions: libxkbcommon-x11, libXcursor and libXi
- Optional: `xdg-utils` to open files, `polkit` (pkexec) for full access scans

These are already installed on almost every desktop system.

## Usage

Start Spacemapper from your application menu, or from a terminal:

```sh
spacemapper                    # opens the drive list
spacemapper ~/Downloads        # scans a folder directly
```

On the start screen, click **Scan** next to a drive, or enter a folder path. You can also drag a folder onto the window.

### Navigating

| Action | Mouse | Keyboard |
|---|---|---|
| Zoom into a folder | Double-click, or scroll up over it | Enter (on a selected item) |
| Zoom out one level | Scroll down | U or Alt+Up |
| Back / forward | Mouse back / forward buttons | Backspace, Alt+Left / Alt+Right |
| Go to the top level | House button | Home |
| Jump to a parent folder | Click it in the path bar | |
| Select an item | Click | |
| Context menu | Right-click | |
| Show in file manager | Middle-click | |
| Rescan | Rescan button | F5 |
| Show or hide the side list | List button | L |
| Focus the filter box | | Ctrl+F |

The right-click menu has: Show in file manager, Open, Zoom in, Zoom out, Copy path, Copy name and Rescan this folder.

### Toolbar

From left to right:

- **Navigation**: back, forward, up one level, top level
- **Detail** (− and +, or keys `[` and `]`): how small a box can get before small items are combined into one box labelled "N small items". More detail shows more individual files.
- **Levels** (− and +, or keys `-` and `+`): how many folder levels are drawn inside each other
- **Colors**: by top-level folder, grayscale, by file type, or by age (green for recently changed files, red for files not changed in years)
- **Disk / Length**: show the space used on disk, or the file sizes. They differ for compressed, sparse and very small files.
- **Free / Unknown**: show or hide free space and unknown space (only when scanning a whole drive)
- **List**, **Rescan** and **Drives** (back to the start screen)

Folders that only contain a single subfolder are drawn as one box, for example `lib / python3.13 / site-packages`.

### Side list

- **Contents**: the files and folders in the current view
- **Largest**: the 500 largest files in the current view and below it
- **Types**: space used per file extension. Click a type to filter the treemap to it.

Click a column name (Size, Files, Name, Modified) to sort by it, and click it again to reverse the order. Hovering over a row highlights the item in the treemap, and double-clicking opens it.

### Filters

Type into the filter box, or pick one of the ready-made filters from the **Sort** menu next to it. Separate several terms with `;`.

| Term | Shows |
|---|---|
| `*.mkv;*.mp4` | files matching any of these patterns |
| `cache` | files whose name contains "cache" |
| `/node_modules/` | files whose path contains this (a term with `/` is matched against the full path) |
| `!*.log` | everything except matching files |
| `>100mb` | files larger than 100 MB (units: kb, mb, gb, tb) |
| `<2gb` | files smaller than 2 GB |
| `>1y` | files not modified for more than a year (units: h, d, w, mo, y) |
| `<7d` | files modified in the last 7 days |

Terms can be combined. For example, `*.mkv;*.mp4;>1gb;>6mo` shows video files over 1 GB that were not changed in the last six months.

### Free and unknown space

When you scan a whole drive, two extra boxes appear:

- **Free space** is the space still available on the drive.
- **Unknown** is used space that the scan could not read. This is mostly system folders that your user has no permission to open, plus btrfs snapshots and filesystem metadata.

### Full access

Some folders, such as `/root` and parts of `/var`, can only be read by the administrator. To include them, turn on **Full access** on the start screen before scanning, or click **Rescan with full access** in the status bar after a scan.

Spacemapper then asks for your password through polkit (`pkexec`). Only the scanning part runs with administrator rights; the window itself keeps running as your user.

The password prompt needs a polkit agent. GNOME, KDE Plasma, Cinnamon, Xfce and most other desktops include one. On window managers such as Hyprland, Sway or i3 you may need to install and start one yourself, for example:

```sh
# Hyprland (Arch)
sudo pacman -S hyprpolkitagent
# then add to ~/.config/hypr/hyprland.conf:
exec-once = systemctl --user start hyprpolkitagent

# Sway, i3 and others: install polkit-gnome or lxqt-policykit
# and start it from your config.
```

## How sizes are counted

- By default, sizes are the space used on disk, the same as `du`.
- Hard-linked files are counted once. Symbolic links are not followed.
- The scan stays on one filesystem. Other drives mounted inside the scanned folder (USB drives, network shares) are skipped unless you turn on **Other drives** on the start screen.
- Btrfs subvolumes on the same drive, such as `@` and `@home`, are treated as one filesystem, so scanning `/` includes `/home`.
- Btrfs snapshot folders (`.snapshots` and Timeshift) are skipped, because snapshots share most of their data with the live system and would otherwise be counted several times.
- Virtual filesystems such as `/proc` and `/sys` are never scanned.

## Building from source

You need [Rust](https://rustup.rs) (stable) and a C compiler. On most distributions:

```sh
# Debian / Ubuntu
sudo apt install build-essential curl
# Fedora
sudo dnf install gcc
# Arch
sudo pacman -S base-devel rust
```

Install Rust with [rustup](https://rustup.rs) if your distribution does not package a recent version. Then:

```sh
git clone https://github.com/Martin-ii/spacemapper.git
cd spacemapper
cargo build --release
./target/release/spacemapper
```

### Arch package from source

```sh
git clone https://github.com/Martin-ii/spacemapper.git
cd spacemapper/packaging/arch
makepkg -si
```

### Release packages

`packaging/build-packages.sh` builds the AppImage, .deb, .rpm, Arch package and tar.gz into `dist/`. It needs `cargo-zigbuild` with `zig` (used to build a binary that runs on older glibc versions), `cargo-generate-rpm`, `dpkg-deb`, `appimagetool` and Python with the `zstandard` module. The GitHub Actions workflow in `.github/workflows/release.yml` runs this script and tests the packages on Debian 12 and 13, Ubuntu 22.04 and 24.04, Fedora, openSUSE and Arch.

## License

Spacemapper is free software, licensed under the [GNU General Public License v3.0 or later](LICENSE).

The interface uses the [Inter](https://rsms.me/inter/) font (SIL Open Font License 1.1, see `assets/Inter-LICENSE.txt`) and [Phosphor](https://phosphoricons.com) icons (MIT License).

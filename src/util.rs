// SPDX-License-Identifier: GPL-3.0-or-later
//! Helpers: formatting, drives and mounts, free space, file metadata,
//! opening files, and per-user folders. Everything that differs between
//! Linux and Windows lives here, in the `imp` modules at the bottom.
//!
//! The only external programs started from here are `gio`, `xdg-open` and
//! `dbus-send` on Linux, and `explorer` on Windows (to show or open a file).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use imp::*;

// ---------------------------------------------------------------- formatting

pub fn fmt_size(b: u64) -> String {
    const U: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if b < 1024 {
        return format!("{b} B");
    }
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 {
        format!("{v:.0} {}", U[i])
    } else if v >= 10.0 {
        format!("{v:.1} {}", U[i])
    } else {
        format!("{v:.2} {}", U[i])
    }
}

pub fn fmt_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{202F}'); // narrow no-break space
        }
        out.push(c);
    }
    out
}

/// "1 file", "12 files"
pub fn count_of(n: u64, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{} {word}s", fmt_count(n)) }
}

pub fn fmt_duration(d: std::time::Duration) -> String {
    let s = d.as_secs_f64();
    if s < 60.0 {
        format!("{s:.1} s")
    } else {
        format!("{}m {:02}s", (s / 60.0) as u64, (s % 60.0) as u64)
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- mounts

#[derive(Clone, Debug)]
pub struct Mount {
    pub point: PathBuf,
    pub source: String,
    pub fstype: String,
    /// per-mount options such as "rw,noatime"
    pub options: String,
}

/// Mount table keyed by mount point (topmost mount wins).
pub fn mount_map(mounts: &[Mount]) -> HashMap<PathBuf, Mount> {
    let mut map = HashMap::new();
    for m in mounts {
        map.insert(m.point.clone(), m.clone());
    }
    map
}

/// The mount that contains `p` (longest matching mount point).
pub fn mount_of<'a>(mounts: &'a [Mount], p: &Path) -> Option<&'a Mount> {
    let mut best: Option<&Mount> = None;
    for m in mounts.iter() {
        if p.starts_with(&m.point) {
            let better = match best {
                None => true,
                // ">=" so a later (stacked) mount on the same point wins
                Some(b) => m.point.as_os_str().len() >= b.point.as_os_str().len(),
            };
            if better {
                best = Some(m);
            }
        }
    }
    best
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Space {
    pub total: u64,
    pub avail: u64,
    pub used: u64,
}

#[cfg_attr(unix, allow(dead_code))]
fn run_detached(mut cmd: Command) -> bool {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.spawn() {
        Ok(mut child) => {
            // reap in the background so no zombie processes are left behind
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

/// Absolute form of a path the user typed, without resolving symlinks on
/// Windows (where canonicalize returns `\\?\` paths).
pub fn absolute(p: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
    }
    #[cfg(not(windows))]
    {
        std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    }
}

/// Sizes and dates of a directory entry, the same on every platform.
pub struct Meta {
    /// bytes used on disk
    pub alloc: u64,
    /// file length
    pub apparent: u64,
    pub mtime: i64,
    /// Linux: inode change time. Windows: creation time on this disk.
    pub ctime: i64,
    pub atime: i64,
    /// device and inode, to count hard links once and to notice mount points
    pub dev: u64,
    pub ino: u64,
    pub nlink: u64,
}

// ======================================================================
// Linux and other Unix systems
// ======================================================================
#[cfg(unix)]
mod imp {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;

    pub fn meta(md: &std::fs::Metadata) -> Meta {
        Meta {
            alloc: md.blocks() * 512,
            apparent: md.size(),
            mtime: md.mtime(),
            ctime: md.ctime(),
            atime: md.atime(),
            dev: md.dev(),
            ino: md.ino(),
            nlink: md.nlink(),
        }
    }

    pub fn fmt_date(secs: i64) -> String {
        let t: libc::time_t = secs as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
        if !ok {
            return "?".into();
        }
        format!("{:04}-{:02}-{:02} {:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
    }

    fn unescape_mount(s: &str) -> String {
        // /proc/self/mountinfo escapes space, tab, newline and backslash as \ooo
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'\\' && i + 3 < b.len() {
                let oct = &b[i + 1..i + 4];
                if oct.iter().all(|c| (b'0'..=b'7').contains(c)) {
                    let v = (oct[0] - b'0') as u32 * 64 + (oct[1] - b'0') as u32 * 8 + (oct[2] - b'0') as u32;
                    out.push(v as u8);
                    i += 4;
                    continue;
                }
            }
            out.push(b[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    pub fn read_mounts() -> Vec<Mount> {
        let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
            return Vec::new();
        };
        let mut v = Vec::new();
        for line in text.lines() {
            let Some((left, right)) = line.split_once(" - ") else { continue };
            let lf: Vec<&str> = left.split(' ').collect();
            let rf: Vec<&str> = right.split(' ').collect();
            if lf.len() < 5 || rf.len() < 2 {
                continue;
            }
            v.push(Mount {
                point: PathBuf::from(unescape_mount(lf[4])),
                fstype: rf[0].to_string(),
                source: unescape_mount(rf[1]),
                options: lf.get(5).unwrap_or(&"").to_string(),
            });
        }
        v
    }

    /// Pseudo filesystems that must never be scanned (they are not on any disk).
    pub fn is_virtual_fs(fstype: &str) -> bool {
        matches!(
            fstype,
            "proc" | "sysfs" | "devtmpfs" | "devpts" | "cgroup" | "cgroup2" | "securityfs"
                | "debugfs" | "tracefs" | "pstore" | "bpf" | "mqueue" | "hugetlbfs"
                | "configfs" | "fusectl" | "autofs" | "binfmt_misc" | "efivarfs"
                | "rpc_pipefs" | "nsfs" | "selinuxfs" | "overlay" | "fuse.portal"
                | "fuse.gvfsd-fuse" | "ramfs" | "nfsd"
        )
    }

    /// Filesystems worth listing on the start screen.
    pub fn is_listable_fs(m: &Mount) -> bool {
        if is_virtual_fs(&m.fstype) || m.fstype == "tmpfs" || m.fstype == "squashfs" {
            return false;
        }
        m.source.starts_with('/')
            || matches!(m.fstype.as_str(), "nfs" | "nfs4" | "cifs" | "smb3" | "9p" | "virtiofs")
            || m.fstype.starts_with("fuse.")
    }

    pub fn fs_space(p: &Path) -> Option<Space> {
        let c = CString::new(p.as_os_str().as_bytes()).ok()?;
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
            return None;
        }
        let fr = s.f_frsize as u64;
        Some(Space {
            total: s.f_blocks as u64 * fr,
            avail: s.f_bavail as u64 * fr,
            used: (s.f_blocks as u64).saturating_sub(s.f_bfree as u64) * fr,
        })
    }

    fn file_uri(p: &Path) -> String {
        let mut s = String::from("file://");
        for &b in p.as_os_str().as_bytes() {
            let keep = b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~');
            if keep {
                s.push(b as char);
            } else {
                s.push_str(&format!("%{b:02X}"));
            }
        }
        s
    }

    /// Open a file or folder with the default application. `gio open` is
    /// tried first: outside GNOME and KDE (Hyprland, Sway, ...) `xdg-open`
    /// searches all .desktop files with a shell script, which can take
    /// seconds, while gio finds the application right away.
    fn open_default(p: &Path) -> bool {
        let quiet = |mut c: Command| c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        let mut gio = Command::new("gio");
        gio.arg("open").arg(p);
        if quiet(gio) {
            return true;
        }
        let mut xdg = Command::new("xdg-open");
        xdg.arg(p);
        quiet(xdg)
    }

    pub fn open_path(p: &Path) {
        let p = p.to_path_buf();
        std::thread::spawn(move || {
            open_default(&p);
        });
    }

    /// Open the file manager with the item selected (FreeDesktop FileManager1
    /// interface: Dolphin, Nautilus, Nemo, Thunar, ...). Falls back to opening
    /// the containing folder.
    pub fn show_in_file_manager(p: &Path) {
        let uri = file_uri(p);
        let parent = p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| p.to_path_buf());
        std::thread::spawn(move || {
            let ok = Command::new("dbus-send")
                .args([
                    "--session",
                    "--print-reply",
                    "--reply-timeout=4000",
                    "--dest=org.freedesktop.FileManager1",
                    "--type=method_call",
                    "/org/freedesktop/FileManager1",
                    "org.freedesktop.FileManager1.ShowItems",
                    &format!("array:string:{uri}"),
                    "string:",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                open_default(&parent);
            }
        });
    }

    pub fn home_dir() -> PathBuf {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
    }

    fn xdg(var: &str, fallback: &str) -> PathBuf {
        std::env::var_os(var)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home_dir().join(fallback))
    }

    /// ~/.config/spacemapper
    pub fn config_dir() -> PathBuf {
        xdg("XDG_CONFIG_HOME", ".config").join("spacemapper")
    }

    /// ~/.local/state/spacemapper
    pub fn state_dir() -> PathBuf {
        xdg("XDG_STATE_HOME", ".local/state").join("spacemapper")
    }

    /// Never called on Unix: full access works through pkexec instead.
    pub fn relaunch_as_admin(_path: Option<&Path>) -> bool {
        false
    }

    pub fn is_admin() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    pub fn attach_console() {}
}

// ======================================================================
// Windows
// ======================================================================
#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::Storage::FileSystem::{
        FileTimeToLocalFileTime, GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows_sys::Win32::System::Time::FileTimeToSystemTime;

    /// FILETIME (100 ns since 1601) to Unix seconds
    fn ft_to_unix(t: u64) -> i64 {
        (t / 10_000_000) as i64 - 11_644_473_600
    }

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x40000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x400000;
    /// NTFS default cluster size; files take whole clusters on disk
    const CLUSTER: u64 = 4096;

    pub fn meta(md: &std::fs::Metadata) -> Meta {
        let len = md.len();
        let attrs = md.file_attributes();
        // OneDrive and other cloud placeholders are listed but not stored on
        // this disk
        let online_only = attrs & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0;
        let alloc = if online_only || md.is_dir() { 0 } else { len.div_ceil(CLUSTER) * CLUSTER };
        let mtime = ft_to_unix(md.last_write_time());
        Meta {
            alloc,
            apparent: if md.is_dir() { 0 } else { len },
            mtime,
            // the creation time is set when a file is copied or downloaded
            // to this disk, which is what "added" means
            ctime: ft_to_unix(md.creation_time()),
            atime: ft_to_unix(md.last_access_time()),
            dev: 0,
            ino: 0,
            nlink: 1,
        }
    }

    pub fn fmt_date(secs: i64) -> String {
        let t = ((secs + 11_644_473_600).max(0) as u64) * 10_000_000;
        let ft = FILETIME { dwLowDateTime: t as u32, dwHighDateTime: (t >> 32) as u32 };
        let mut local = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
        unsafe {
            if FileTimeToLocalFileTime(&ft, &mut local) == 0 || FileTimeToSystemTime(&local, &mut st) == 0 {
                return "?".into();
            }
        }
        format!("{:04}-{:02}-{:02} {:02}:{:02}", st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute)
    }

    /// Drive letters with a filesystem (C:\, D:\, network drives, USB sticks).
    pub fn read_mounts() -> Vec<Mount> {
        let mask = unsafe { GetLogicalDrives() };
        let mut v = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = PathBuf::from(format!("{letter}:\\"));
            let w = wide(&root);
            let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
            if kind <= 1 {
                continue; // unknown or no root directory
            }
            let mut label = [0u16; 261];
            let mut fsname = [0u16; 261];
            let ok = unsafe {
                GetVolumeInformationW(
                    w.as_ptr(),
                    label.as_mut_ptr(),
                    label.len() as u32,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    fsname.as_mut_ptr(),
                    fsname.len() as u32,
                )
            };
            if ok == 0 {
                continue; // empty card reader or DVD drive
            }
            let s = |b: &[u16]| {
                let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                std::ffi::OsString::from_wide(&b[..n]).to_string_lossy().into_owned()
            };
            let label = s(&label);
            let source = match (label.is_empty(), kind) {
                (false, _) => label,
                (true, 2) => "Removable drive".into(),
                (true, 4) => "Network drive".into(),
                (true, 5) => "DVD drive".into(),
                _ => "Local disk".into(),
            };
            v.push(Mount { point: root, source, fstype: s(&fsname), options: String::new() });
        }
        v
    }

    pub fn is_virtual_fs(_fstype: &str) -> bool {
        false
    }

    pub fn is_listable_fs(_m: &Mount) -> bool {
        true
    }

    pub fn fs_space(p: &Path) -> Option<Space> {
        let w = wide(p);
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
        (ok != 0).then(|| Space { total, avail, used: total.saturating_sub(free) })
    }

    /// Open a file with its default program, or a folder in Explorer.
    pub fn open_path(p: &Path) {
        let mut c = Command::new("explorer");
        c.arg(p);
        run_detached(c);
    }

    /// Open Explorer with the item selected.
    pub fn show_in_file_manager(p: &Path) {
        let mut c = Command::new("explorer");
        // explorer needs exactly: /select,"C:\path with spaces"
        c.raw_arg(format!("/select,\"{}\"", p.display()));
        run_detached(c);
    }

    pub fn home_dir() -> PathBuf {
        std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\"))
    }

    fn known(var: &str, fallback: &str) -> PathBuf {
        std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| home_dir().join(fallback))
    }

    /// %APPDATA%\Spacemapper
    pub fn config_dir() -> PathBuf {
        known("APPDATA", "AppData\\Roaming").join("Spacemapper")
    }

    /// %LOCALAPPDATA%\Spacemapper
    pub fn state_dir() -> PathBuf {
        known("LOCALAPPDATA", "AppData\\Local").join("Spacemapper")
    }

    pub fn is_admin() -> bool {
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }

    /// Start Spacemapper again with administrator rights (the UAC prompt),
    /// optionally scanning `path`. Returns true if the new copy started.
    pub fn relaunch_as_admin(path: Option<&Path>) -> bool {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let Ok(exe) = std::env::current_exe() else { return false };
        let to_w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let verb = to_w("runas");
        let file = wide(&exe);
        // quote the path; a trailing backslash (as in C:\) must be doubled so
        // it does not escape the closing quote
        let params = to_w(&path
            .map(|p| {
                let p = p.display().to_string();
                let extra = "\\".repeat(p.len() - p.trim_end_matches('\\').len());
                format!("\"{p}{extra}\"")
            })
            .unwrap_or_default());
        let r = unsafe {
            ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), params.as_ptr(), std::ptr::null(), SW_SHOWNORMAL)
        };
        r as isize > 32
    }

    /// Spacemapper is a window program on Windows; when started from a
    /// terminal with --json, --help or --version, print to that terminal.
    pub fn attach_console() {
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
//! Helpers: formatting, mount table, filesystem space, opening files.
//!
//! The only external programs started from here are `xdg-open` and
//! `dbus-send` (to show a file in the file manager).

use std::collections::HashMap;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

pub fn fmt_date(secs: i64) -> String {
    let t: libc::time_t = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if !ok {
        return "?".into();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
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

// ---------------------------------------------------------------- opening

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

/// Open a file or folder with the default application (xdg-open).
pub fn open_path(p: &Path) {
    let mut c = Command::new("xdg-open");
    c.arg(p);
    run_detached(c);
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
            let _ = Command::new("xdg-open")
                .arg(&parent)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    });
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

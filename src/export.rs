// SPDX-License-Identifier: GPL-3.0-or-later
//! `spacemapper --json PATH`: scan a folder and print the result as JSON on
//! standard output, for use from other programs and scripts.
//!
//! Every entry has:
//!   name, type ("dir" | "file" | "link" | "other"),
//!   size (bytes on disk), apparent (file length in bytes),
//!   modified, added, opened (Unix time in seconds)
//! Folders also have `files` (number of files below) and `children`; folders
//! that could not be read have `"unreadable": true`. The top level adds
//! `path`, `scanned` (Unix time) and, when it is a whole filesystem,
//! `filesystem` with `total`, `used` and `free` bytes.

use crate::scan::{self, Kind, Progress, TNode, F_DENIED};
use crate::util;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

fn esc(w: &mut impl Write, s: &str) -> io::Result<()> {
    w.write_all(b"\"")?;
    for c in s.chars() {
        match c {
            '"' => w.write_all(b"\\\"")?,
            '\\' => w.write_all(b"\\\\")?,
            '\n' => w.write_all(b"\\n")?,
            '\r' => w.write_all(b"\\r")?,
            '\t' => w.write_all(b"\\t")?,
            c if (c as u32) < 0x20 => write!(w, "\\u{:04x}", c as u32)?,
            c => write!(w, "{c}")?,
        }
    }
    w.write_all(b"\"")
}

/// (bytes on disk, apparent bytes, files) of a subtree
fn totals(n: &TNode) -> (u64, u64, u64) {
    let mut t = (n.alloc, n.apparent, if n.kind == Kind::Dir { 0 } else { 1 });
    for c in &n.children {
        let s = totals(c);
        t = (t.0 + s.0, t.1 + s.1, t.2 + s.2);
    }
    t
}

fn node(w: &mut impl Write, n: &TNode, depth: u32, max_depth: u32, top: Option<&str>) -> io::Result<(u64, u64, u64)> {
    let kind = match n.kind {
        Kind::Dir => "dir",
        Kind::File => "file",
        Kind::Link => "link",
        _ => "other",
    };
    w.write_all(b"{")?;
    if let Some(path) = top {
        w.write_all(b"\"path\":")?;
        esc(w, path)?;
        write!(w, ",\"scanned\":{},", util::now_secs())?;
        if let Some(s) = util::fs_space(Path::new(path)) {
            write!(w, "\"filesystem\":{{\"total\":{},\"used\":{},\"free\":{}}},", s.total, s.used, s.avail)?;
        }
    }
    w.write_all(b"\"name\":")?;
    esc(w, &n.name)?;
    write!(
        w,
        ",\"type\":\"{kind}\",\"modified\":{},\"added\":{},\"opened\":{}",
        n.mtime,
        n.mtime.max(n.ctime),
        n.atime.max(n.mtime)
    )?;
    if n.flags & F_DENIED != 0 {
        w.write_all(b",\"unreadable\":true")?;
    }
    let mut t = (n.alloc, n.apparent, if n.kind == Kind::Dir { 0 } else { 1 });
    if n.kind == Kind::Dir {
        if depth < max_depth {
            w.write_all(b",\"children\":[")?;
            let mut kids: Vec<&TNode> = n.children.iter().collect();
            kids.sort_by_cached_key(|c| std::cmp::Reverse(totals(c).0));
            for (i, c) in kids.into_iter().enumerate() {
                if i > 0 {
                    w.write_all(b",")?;
                }
                let s = node(w, c, depth + 1, max_depth, None)?;
                t = (t.0 + s.0, t.1 + s.1, t.2 + s.2);
            }
            w.write_all(b"]")?;
        } else {
            let s = totals(n);
            t = s;
        }
        write!(w, ",\"files\":{}", t.2)?;
    }
    write!(w, ",\"size\":{},\"apparent\":{}}}", t.0, t.1)?;
    Ok(t)
}

pub fn run(path: &Path, max_depth: u32, cross: bool) -> i32 {
    let out = scan::scan(path, cross, Arc::new(Progress::default()));
    let Some(root) = out.root else {
        eprintln!("{}", out.error.unwrap_or_default());
        return 1;
    };
    let stdout = io::stdout();
    let mut w = BufWriter::with_capacity(1 << 20, stdout.lock());
    let shown = path.to_string_lossy().into_owned();
    if node(&mut w, &root, 0, max_depth, Some(&shown)).and_then(|_| w.write_all(b"\n")).and_then(|_| w.flush()).is_err() {
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escaping() {
        let mut v = Vec::new();
        esc(&mut v, "a\"b\\c\nd\u{1}é").unwrap();
        assert_eq!(String::from_utf8(v).unwrap(), "\"a\\\"b\\\\c\\nd\\u0001é\"");
    }

    #[test]
    fn json_tree() {
        let file = |name: &str, size: u64| TNode { name: name.into(), kind: Kind::File, alloc: size, apparent: size, mtime: 5, ctime: 7, atime: 6, flags: 0, children: vec![] };
        let root = TNode {
            name: "/x".into(),
            kind: Kind::Dir,
            alloc: 0,
            apparent: 0,
            mtime: 1,
            ctime: 1,
            atime: 1,
            flags: 0,
            children: vec![file("a", 10), TNode { name: "sub".into(), kind: Kind::Dir, alloc: 0, apparent: 0, mtime: 1, ctime: 1, atime: 1, flags: F_DENIED, children: vec![file("b", 30)] }],
        };
        let mut v = Vec::new();
        node(&mut v, &root, 0, 1, None).unwrap();
        let s = String::from_utf8(v).unwrap();
        // depth limit: "sub" is summarised without children
        assert!(s.contains("\"name\":\"sub\",\"type\":\"dir\",\"modified\":1,\"added\":1,\"opened\":1,\"unreadable\":true,\"files\":1,\"size\":30"));
        assert!(s.ends_with("\"files\":2,\"size\":40,\"apparent\":40}"));
        assert!(s.contains("\"added\":7"));
    }
}

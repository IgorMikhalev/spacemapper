// SPDX-License-Identifier: GPL-3.0-or-later
//! Flat arena holding the scanned tree, plus size aggregation with filters.

use crate::filter::{FileInfo, Filter};
use crate::scan::{Kind, TNode, F_DENIED, F_KEEP};
use std::collections::HashMap;
use crate::util::{self, Mount, Space};
use std::path::{Path, PathBuf};

pub const NONE: u32 = u32::MAX;

pub struct Node {
    pub name: Box<str>,
    /// lowercase name, for filtering
    pub lname: Box<str>,
    pub parent: u32,
    pub children: Vec<u32>,
    pub kind: Kind,
    pub flags: u8,
    pub alloc: u64,
    pub apparent: u64,
    pub mtime: i64,
    pub ctime: i64,
    pub atime: i64,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }
    /// When the file arrived on this disk or was last changed: the later of
    /// the modification and change times. A file installed today counts as
    /// new even if its contents are years old.
    pub fn added(&self) -> i64 {
        self.mtime.max(self.ctime)
    }

    pub fn date(&self, kind: DateKind) -> i64 {
        match kind {
            DateKind::Modified => self.mtime,
            DateKind::Added => self.added(),
            DateKind::Opened => self.atime.max(self.mtime),
        }
    }

    pub fn is_pseudo(&self) -> bool {
        matches!(self.kind, Kind::Free | Kind::Unknown)
    }
}

/// Which file date to use for age filters and colors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DateKind {
    Modified,
    Added,
    Opened,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub root: u32,
    pub root_path: PathBuf,
    pub free_id: u32,
    pub unknown_id: u32,
    pub mount: Option<Mount>,
    pub space: Option<Space>,
    pub is_mount_root: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SizeMode {
    OnDisk,
    #[allow(dead_code)]
    Apparent,
}

pub struct Values {
    pub size: Vec<u64>,
    pub files: Vec<u64>,
}

impl Tree {
    pub fn from_scan(t: TNode, root_path: &Path) -> Tree {
        let mounts = util::read_mounts();
        let mount = util::mount_of(&mounts, root_path).cloned();
        let is_mount_root = mount.as_ref().map(|m| m.point == root_path).unwrap_or(false);
        let mut tree = Tree {
            nodes: Vec::new(),
            root: 0,
            root_path: root_path.to_path_buf(),
            free_id: NONE,
            unknown_id: NONE,
            mount,
            space: util::fs_space(root_path),
            is_mount_root,
        };
        tree.root = tree.add(t, NONE);
        tree.add_pseudo();
        tree
    }

    fn add(&mut self, t: TNode, parent: u32) -> u32 {
        let id = self.nodes.len() as u32;
        let TNode { name, kind, alloc, apparent, mtime, ctime, atime, flags, children } = t;
        let lname = name.to_lowercase().into_boxed_str();
        self.nodes.push(Node { name, lname, parent, children: Vec::new(), kind, flags, alloc, apparent, mtime, ctime, atime });
        if !children.is_empty() {
            let mut ids = Vec::with_capacity(children.len());
            for c in children {
                ids.push(self.add(c, id));
            }
            self.nodes[id as usize].children = ids;
        }
        id
    }

    fn add_pseudo(&mut self) {
        let Some(space) = self.space else { return };
        // Like SpaceSniffer: free/unknown space only make sense for a whole drive.
        if !self.is_mount_root {
            return;
        }
        let root = self.root;
        let free = self.push_child(root, "<Free space>", Kind::Free, space.avail);
        self.free_id = free;
        // "Unknown" = used space on this filesystem that the scan could not see
        // (folders without permission, other subvolumes/snapshots, metadata...).
        // Only meaningful when scanning a whole filesystem from its mount point.
        if self.is_mount_root {
            let id = self.push_child(root, "<Unknown / not accessible>", Kind::Unknown, 0);
            self.unknown_id = id;
            self.update_unknown();
        }
    }

    fn push_child(&mut self, parent: u32, name: &str, kind: Kind, size: u64) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            name: name.into(),
            lname: name.to_lowercase().into(),
            parent,
            children: Vec::new(),
            kind,
            flags: 0,
            alloc: size,
            apparent: size,
            mtime: 0,
            ctime: 0,
            atime: 0,
        });
        self.nodes[parent as usize].children.push(id);
        id
    }

    fn update_unknown(&mut self) {
        if self.unknown_id == NONE {
            return;
        }
        let Some(space) = self.space else { return };
        let scanned = self.sum_alloc(self.root);
        let unknown = space.used.saturating_sub(scanned);
        let n = &mut self.nodes[self.unknown_id as usize];
        n.alloc = unknown;
        n.apparent = unknown;
    }

    fn sum_alloc(&self, id: u32) -> u64 {
        let mut total = 0u64;
        let mut stack = vec![id];
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i as usize];
            if n.is_pseudo() {
                continue;
            }
            total += n.alloc;
            stack.extend_from_slice(&n.children);
        }
        total
    }

    /// Replace a directory's subtree with a fresh scan. Returns the new id.
    pub fn replace_subtree(&mut self, id: u32, t: TNode) -> u32 {
        let parent = self.nodes[id as usize].parent;
        let name = self.nodes[id as usize].name.clone();
        let new = self.add(t, parent);
        self.nodes[new as usize].name = name;
        if parent != NONE {
            for c in self.nodes[parent as usize].children.iter_mut() {
                if *c == id {
                    *c = new;
                }
            }
        }
        // detached: no longer reachable from the root (see `is_live`)
        self.nodes[id as usize].parent = NONE;
        self.refresh_space();
        new
    }

    /// Read the filesystem's free and used space again.
    pub fn refresh_space(&mut self) {
        self.space = util::fs_space(&self.root_path);
        if self.free_id != NONE {
            if let Some(s) = self.space {
                let n = &mut self.nodes[self.free_id as usize];
                n.alloc = s.avail;
                n.apparent = s.avail;
            }
        }
        self.update_unknown();
    }

    /// Merge a fresh read of folder `id` (from `scan::refresh`). Entries that
    /// are still there keep their node id; `F_KEEP` folders keep their
    /// subtree. Removed entries are detached from the tree. Call
    /// `refresh_space` afterwards.
    pub fn merge_dir(&mut self, id: u32, t: TNode) {
        let old: HashMap<Box<str>, u32> = self.nodes[id as usize]
            .children
            .iter()
            .filter(|&&c| !self.nodes[c as usize].is_pseudo())
            .map(|&c| (self.nodes[c as usize].name.clone(), c))
            .collect();
        let pseudo: Vec<u32> =
            self.nodes[id as usize].children.iter().copied().filter(|&c| self.nodes[c as usize].is_pseudo()).collect();
        let TNode { alloc, apparent, mtime, ctime, atime, children, .. } = t;
        let mut ids = Vec::with_capacity(children.len() + pseudo.len());
        let mut kept = std::collections::HashSet::new();
        for c in children {
            let prev = old.get(&c.name).copied().filter(|&o| {
                let n = &self.nodes[o as usize];
                // a folder is kept as it is; a file is updated in place
                if c.flags & F_KEEP != 0 { n.is_dir() } else { !n.is_dir() && c.kind != Kind::Dir }
            });
            match prev {
                Some(o) => {
                    let n = &mut self.nodes[o as usize];
                    n.alloc = c.alloc;
                    n.apparent = c.apparent;
                    n.mtime = c.mtime;
                    n.ctime = c.ctime;
                    n.atime = c.atime;
                    if c.flags & F_KEEP == 0 {
                        n.kind = c.kind;
                        n.flags = c.flags;
                    }
                    kept.insert(o);
                    ids.push(o);
                }
                // F_KEEP without a folder to keep can only happen if the tree
                // changed in between; it shows as an empty folder until the
                // next change
                None => ids.push(self.add(TNode { flags: c.flags & !F_KEEP, ..c }, id)),
            }
        }
        for (_, o) in old {
            if !kept.contains(&o) {
                self.nodes[o as usize].parent = NONE;
            }
        }
        ids.extend(pseudo);
        let n = &mut self.nodes[id as usize];
        n.children = ids;
        n.alloc = alloc;
        n.apparent = apparent;
        n.mtime = mtime;
        n.ctime = ctime;
        n.atime = atime;
    }

    /// The node at `path`, or the deepest folder on the way to it that is in
    /// the tree. None if `path` is outside the scanned folder.
    pub fn nearest(&self, path: &Path) -> Option<(u32, bool)> {
        let rel = path.strip_prefix(&self.root_path).ok()?;
        let mut id = self.root;
        for part in rel.components() {
            let name = part.as_os_str().to_string_lossy();
            let next = self.nodes[id as usize]
                .children
                .iter()
                .copied()
                .find(|&c| !self.nodes[c as usize].is_pseudo() && *self.nodes[c as usize].name == *name);
            match next {
                Some(c) => id = c,
                None => return Some((id, false)),
            }
        }
        Some((id, true))
    }

    /// The biggest files in the tree with at least `min` bytes on disk,
    /// largest first, at most `max` of them.
    pub fn big_files(&self, min: u64, max: usize) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        let mut stack = vec![self.root];
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i as usize];
            if n.kind == Kind::File && n.alloc >= min {
                out.push(i);
            }
            stack.extend_from_slice(&n.children);
        }
        out.sort_unstable_by(|a, b| self.nodes[*b as usize].alloc.cmp(&self.nodes[*a as usize].alloc));
        out.truncate(max);
        out
    }

    /// Update a file's sizes and dates from a fresh stat. Returns whether
    /// anything changed.
    pub fn update_file(&mut self, id: u32, m: &util::Meta) -> bool {
        let n = &mut self.nodes[id as usize];
        let changed = (n.alloc, n.apparent, n.mtime, n.ctime) != (m.alloc, m.apparent, m.mtime, m.ctime);
        n.alloc = m.alloc;
        n.apparent = m.apparent;
        n.mtime = m.mtime;
        n.ctime = m.ctime;
        n.atime = m.atime;
        changed
    }

    /// False for nodes that were removed by a rescan or live update.
    pub fn is_live(&self, id: u32) -> bool {
        (id as usize) < self.nodes.len() && self.is_within(id, self.root)
    }

    /// True if `id` is `anc` or lies below it.
    pub fn is_within(&self, mut id: u32, anc: u32) -> bool {
        while id != NONE {
            if id == anc {
                return true;
            }
            id = self.nodes[id as usize].parent;
        }
        false
    }

    /// Chain from root to `id` (inclusive).
    pub fn chain(&self, mut id: u32) -> Vec<u32> {
        let mut v = Vec::new();
        while id != NONE {
            v.push(id);
            id = self.nodes[id as usize].parent;
        }
        v.reverse();
        v
    }

    pub fn path(&self, id: u32) -> PathBuf {
        let chain = self.chain(id);
        let mut p = self.root_path.clone();
        for &c in chain.iter().skip(1) {
            let n = &self.nodes[c as usize];
            if n.is_pseudo() {
                break;
            }
            p.push(&*n.name);
        }
        p
    }

    /// True when the scanned filesystem does not record access times, so
    /// "opened" dates are meaningless.
    pub fn no_atime(&self) -> bool {
        // Windows only updates the last-access time on some drives and
        // otherwise keeps the date a file was created or last written
        if cfg!(windows) {
            return true;
        }
        self.mount.as_ref().map(|m| m.options.split(',').any(|o| o == "noatime")).unwrap_or(false)
    }

    pub fn denied_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.flags & F_DENIED != 0).count()
    }

    /// Aggregate sizes bottom-up for the current size mode, filter and
    /// pseudo-item visibility.
    pub fn compute_values(&self, mode: SizeMode, filter: &Filter, show_free: bool, show_unknown: bool) -> Values {
        let n = self.nodes.len();
        let mut size = vec![0u64; n];
        let mut files = vec![0u64; n];
        // preorder from root (only reachable nodes; replaced subtrees are skipped)
        let mut order: Vec<u32> = Vec::with_capacity(n);
        let mut stack = vec![self.root];
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend_from_slice(&self.nodes[i as usize].children);
        }
        let active = filter.is_active();
        // hidden = inside a folder starting with "." (below the scan root) or
        // itself starting with "."; computed top-down only when needed
        let mut hidden = Vec::new();
        if filter.needs_hidden {
            hidden = vec![false; n];
            for &i in &order {
                let node = &self.nodes[i as usize];
                if i == self.root || node.parent == NONE {
                    continue;
                }
                hidden[i as usize] = hidden[node.parent as usize] || node.name.starts_with('.');
            }
        }
        // lowercase full paths of folders, built top-down once
        let mut dir_paths: Vec<String> = Vec::new();
        if filter.needs_path {
            dir_paths = vec![String::new(); n];
            // filters always use '/' so the same patterns work on Windows
            dir_paths[self.root as usize] = self.root_path.to_string_lossy().to_lowercase().replace('\\', "/");
            for &i in &order {
                let node = &self.nodes[i as usize];
                if i == self.root || !node.is_dir() {
                    continue;
                }
                let parent = &dir_paths[node.parent as usize];
                let mut p = String::with_capacity(parent.len() + 1 + node.lname.len());
                p.push_str(parent);
                if !p.ends_with('/') {
                    p.push('/');
                }
                p.push_str(&node.lname);
                dir_paths[i as usize] = p;
            }
        }
        // which files pass the filter, checked in parallel
        let mut pass: Vec<bool> = Vec::new();
        if active {
            use rayon::prelude::*;
            pass = vec![false; n];
            let file_ids: Vec<u32> = order
                .iter()
                .copied()
                .filter(|&i| {
                    let k = self.nodes[i as usize].kind;
                    !matches!(k, Kind::Dir | Kind::Free | Kind::Unknown)
                })
                .collect();
            let results: Vec<bool> = file_ids
                .par_iter()
                .with_min_len(4096)
                .map_init(String::new, |buf, &i| {
                    let node = &self.nodes[i as usize];
                    let path = if filter.needs_path {
                        buf.clear();
                        buf.push_str(&dir_paths[node.parent as usize]);
                        if !buf.ends_with('/') {
                            buf.push('/');
                        }
                        buf.push_str(&node.lname);
                        Some(buf.as_str())
                    } else {
                        None
                    };
                    let info = FileInfo {
                        name: &node.lname,
                        path,
                        size: match mode {
                            SizeMode::OnDisk => node.alloc,
                            SizeMode::Apparent => node.apparent,
                        },
                        mtime: node.mtime,
                        ctime: node.ctime,
                        atime: node.atime,
                        hidden: hidden.get(i as usize).copied().unwrap_or(false),
                    };
                    filter.matches(&info)
                })
                .collect();
            for (&i, ok) in file_ids.iter().zip(results) {
                pass[i as usize] = ok;
            }
        }
        for &i in order.iter().rev() {
            let node = &self.nodes[i as usize];
            let raw = match mode {
                SizeMode::OnDisk => node.alloc,
                SizeMode::Apparent => node.apparent,
            };
            let (own, own_files) = match node.kind {
                Kind::Dir => (if active { 0 } else { raw }, 0),
                Kind::Free => (if show_free && !active { node.alloc } else { 0 }, 0),
                Kind::Unknown => (if show_unknown && !active { node.alloc } else { 0 }, 0),
                _ => {
                    if !active || pass[i as usize] {
                        (raw, 1)
                    } else {
                        (0, 0)
                    }
                }
            };
            size[i as usize] += own;
            files[i as usize] += own_files;
            let p = node.parent;
            if p != NONE {
                size[p as usize] += size[i as usize];
                files[p as usize] += files[i as usize];
            }
        }
        Values { size, files }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{self, Progress, Refresh};
    use std::fs;
    use std::sync::Arc;

    #[test]
    fn live_refresh_keeps_ids_and_drops_removed() {
        let dir = std::env::temp_dir().join(format!("spacemapper-live-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("big/inner")).unwrap();
        fs::create_dir_all(dir.join("keep")).unwrap();
        fs::write(dir.join("big/inner/a"), vec![1u8; 100_000]).unwrap();
        fs::write(dir.join("keep/b"), vec![1u8; 50_000]).unwrap();
        fs::write(dir.join("c"), b"x").unwrap();

        let out = scan::scan(&dir, false, Arc::new(Progress::default()));
        let mut tree = Tree::from_scan(out.root.unwrap(), &dir);
        let keep = tree.nearest(&dir.join("keep")).unwrap().0;
        let big = tree.nearest(&dir.join("big")).unwrap().0;
        let c = tree.nearest(&dir.join("c")).unwrap().0;

        // delete a folder, grow a file, add a new folder
        fs::remove_dir_all(dir.join("big")).unwrap();
        fs::write(dir.join("c"), vec![1u8; 20_000]).unwrap();
        fs::create_dir_all(dir.join("new")).unwrap();
        fs::write(dir.join("new/d"), b"y").unwrap();

        let known = [Box::<str>::from("big"), "keep".into()].into_iter().collect();
        let res = scan::refresh(&dir, false, vec![Refresh { path: dir.clone(), keep: known }]);
        let (_, node) = res.into_iter().next().unwrap();
        tree.merge_dir(tree.root, node.unwrap());

        assert!(tree.is_live(keep));
        assert!(tree.is_live(c));
        assert!(!tree.is_live(big));
        assert_eq!(tree.nearest(&dir.join("keep")), Some((keep, true)));
        assert_eq!(tree.nodes[c as usize].apparent, 20_000);
        assert_eq!(tree.nearest(&dir.join("big/inner")), Some((tree.root, false)));
        let new = tree.nearest(&dir.join("new/d")).unwrap();
        assert!(new.1);
        // the kept folder still has its file
        assert_eq!(tree.nodes[keep as usize].children.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn big_files_are_checked_again() {
        let dir = std::env::temp_dir().join(format!("spacemapper-big-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/big"), vec![1u8; 300_000]).unwrap();
        fs::write(dir.join("small"), b"x").unwrap();
        let out = scan::scan(&dir, false, Arc::new(Progress::default()));
        let mut tree = Tree::from_scan(out.root.unwrap(), &dir);
        let big = tree.nearest(&dir.join("sub/big")).unwrap().0;

        assert_eq!(tree.big_files(100_000, 10), vec![big]);
        // shrink it without the tree knowing, as a sparse disk image does
        fs::OpenOptions::new().write(true).open(dir.join("sub/big")).unwrap().set_len(1000).unwrap();
        let md = fs::symlink_metadata(dir.join("sub/big")).unwrap();
        assert!(tree.update_file(big, &util::meta(&md)));
        assert_eq!(tree.nodes[big as usize].apparent, 1000);
        assert!(!tree.update_file(big, &util::meta(&md)));
        let _ = fs::remove_dir_all(&dir);
    }
}

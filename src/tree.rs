// SPDX-License-Identifier: GPL-3.0-or-later
//! Flat arena holding the scanned tree, plus size aggregation with filters.

use crate::filter::Filter;
use crate::scan::{Kind, TNode, F_DENIED};
use crate::util::{self, Mount, Space};
use std::path::{Path, PathBuf};

pub const NONE: u32 = u32::MAX;

pub struct Node {
    pub name: Box<str>,
    pub parent: u32,
    pub children: Vec<u32>,
    pub kind: Kind,
    pub flags: u8,
    pub alloc: u64,
    pub apparent: u64,
    pub mtime: i64,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }
    pub fn is_pseudo(&self) -> bool {
        matches!(self.kind, Kind::Free | Kind::Unknown)
    }
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub root: u32,
    pub root_path: PathBuf,
    pub free_id: u32,
    pub unknown_id: u32,
    #[allow(dead_code)]
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
        let TNode { name, kind, alloc, apparent, mtime, flags, children } = t;
        self.nodes.push(Node { name, parent, children: Vec::new(), kind, flags, alloc, apparent, mtime });
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
            parent,
            children: Vec::new(),
            kind,
            flags: 0,
            alloc: size,
            apparent: size,
            mtime: 0,
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
        self.space = util::fs_space(&self.root_path);
        if self.free_id != NONE {
            if let Some(s) = self.space {
                let n = &mut self.nodes[self.free_id as usize];
                n.alloc = s.avail;
                n.apparent = s.avail;
            }
        }
        self.update_unknown();
        new
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
        let mut name_buf = String::new();
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
                    if !active {
                        (raw, 1)
                    } else {
                        name_buf.clear();
                        for ch in node.name.chars() {
                            name_buf.extend(ch.to_lowercase());
                        }
                        let path = if filter.needs_path {
                            Some(self.path(i).to_string_lossy().to_lowercase())
                        } else {
                            None
                        };
                        if filter.matches(&name_buf, path.as_deref(), raw, node.mtime) {
                            (raw, 1)
                        } else {
                            (0, 0)
                        }
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

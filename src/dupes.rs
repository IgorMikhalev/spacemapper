// SPDX-License-Identifier: GPL-3.0-or-later
//! Duplicate file search.
//!
//! Files are only opened for reading. Candidates are grouped by size, then by
//! a hash of their first 64 KiB, then by a hash of their full contents, so
//! most files are never read completely.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use xxhash_rust::xxh3::{xxh3_64, Xxh3};

const HEAD: usize = 64 * 1024;

pub struct Candidate {
    pub node: u32,
    pub path: PathBuf,
    pub size: u64,
}

pub struct Group {
    /// size of one copy in bytes
    pub size: u64,
    /// node ids of all copies
    pub nodes: Vec<u32>,
}

impl Group {
    /// space that would be freed by keeping only one copy
    pub fn wasted(&self) -> u64 {
        self.size * (self.nodes.len() as u64 - 1)
    }
}

#[derive(Default)]
pub struct Progress {
    pub files_total: AtomicU64,
    pub files_done: AtomicU64,
    pub bytes_total: AtomicU64,
    pub bytes_done: AtomicU64,
    pub unreadable: AtomicU64,
    pub cancel: AtomicBool,
    pub current: Mutex<String>,
}

/// Keep only sizes that occur more than once. Returns the candidates that
/// may have duplicates.
pub fn by_size(files: Vec<(u32, u64)>) -> Vec<(u32, u64)> {
    let mut count: HashMap<u64, u32> = HashMap::new();
    for &(_, s) in &files {
        *count.entry(s).or_default() += 1;
    }
    files.into_iter().filter(|(_, s)| count[s] > 1).collect()
}

fn read_head(path: &PathBuf, buf: &mut Vec<u8>) -> Option<u64> {
    let mut f = File::open(path).ok()?;
    buf.clear();
    f.by_ref().take(HEAD as u64).read_to_end(buf).ok()?;
    Some(xxh3_64(buf))
}

fn hash_full(path: &PathBuf, p: &Progress, buf: &mut [u8]) -> Option<u128> {
    let mut f = File::open(path).ok()?;
    let mut h = Xxh3::new();
    loop {
        if p.cancel.load(Relaxed) {
            return None;
        }
        let n = f.read(buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        p.bytes_done.fetch_add(n as u64, Relaxed);
    }
    Some(h.digest128())
}

/// Find groups of identical files. Runs on the calling thread; check
/// `progress` from elsewhere for status.
pub fn find(cands: Vec<Candidate>, progress: Arc<Progress>) -> Vec<Group> {
    let p = &*progress;
    p.files_total.store(cands.len() as u64, Relaxed);
    p.bytes_total.store(cands.iter().map(|c| c.size).sum(), Relaxed);

    // 1. same size + same first 64 KiB
    let mut head_groups: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
    let mut buf = Vec::with_capacity(HEAD);
    for (i, c) in cands.iter().enumerate() {
        if p.cancel.load(Relaxed) {
            return Vec::new();
        }
        if i % 32 == 0 {
            if let Ok(mut cur) = p.current.try_lock() {
                *cur = c.path.to_string_lossy().into_owned();
            }
        }
        match read_head(&c.path, &mut buf) {
            Some(h) => head_groups.entry((c.size, h)).or_default().push(i),
            None => {
                p.unreadable.fetch_add(1, Relaxed);
            }
        }
        p.files_done.fetch_add(1, Relaxed);
        p.bytes_done.fetch_add(c.size.min(HEAD as u64), Relaxed);
    }

    // 2. full contents for files larger than the head
    let mut groups = Vec::new();
    let mut big = vec![0u8; 1 << 20];
    let mut remaining: Vec<Vec<usize>> = head_groups.into_values().filter(|g| g.len() > 1).collect();
    // bytes still to read, for an honest progress bar
    let still: u64 = remaining
        .iter()
        .flatten()
        .map(|&i| cands[i].size.saturating_sub(HEAD as u64))
        .sum();
    p.bytes_total.store(p.bytes_done.load(Relaxed) + still, Relaxed);
    remaining.sort_by_key(|g| std::cmp::Reverse(cands[g[0]].size));
    for g in remaining {
        let size = cands[g[0]].size;
        if size <= HEAD as u64 {
            groups.push(Group { size, nodes: g.iter().map(|&i| cands[i].node).collect() });
            continue;
        }
        let mut full: HashMap<u128, Vec<u32>> = HashMap::new();
        for &i in &g {
            if p.cancel.load(Relaxed) {
                return Vec::new();
            }
            if let Ok(mut cur) = p.current.try_lock() {
                *cur = cands[i].path.to_string_lossy().into_owned();
            }
            // the head was already counted, so count only the rest here
            p.bytes_done.fetch_sub(size.min(HEAD as u64), Relaxed);
            match hash_full(&cands[i].path, p, &mut big) {
                Some(h) => full.entry(h).or_default().push(cands[i].node),
                None => {
                    p.unreadable.fetch_add(1, Relaxed);
                }
            }
        }
        groups.extend(full.into_values().filter(|v| v.len() > 1).map(|nodes| Group { size, nodes }));
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted()));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn finds_identical_files() {
        let dir = std::env::temp_dir().join(format!("spacemapper-dupes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big_a: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut big_b = big_a.clone();
        *big_b.last_mut().unwrap() ^= 1; // same size and head, different tail
        let write = |name: &str, data: &[u8]| {
            let p = dir.join(name);
            File::create(&p).unwrap().write_all(data).unwrap();
            p
        };
        let files = [
            write("a1", &big_a),
            write("a2", &big_a),
            write("b", &big_b),
            write("s1", b"small"),
            write("s2", b"small"),
            write("other", b"other"),
        ];
        let sizes: Vec<(u32, u64)> = files.iter().enumerate().map(|(i, p)| (i as u32, p.metadata().unwrap().len())).collect();
        let cands: Vec<Candidate> = by_size(sizes)
            .into_iter()
            .map(|(n, size)| Candidate { node: n, path: files[n as usize].clone(), size })
            .collect();
        let groups = find(cands, Arc::new(Progress::default()));
        std::fs::remove_dir_all(&dir).ok();
        let mut found: Vec<Vec<u32>> = groups
            .iter()
            .map(|g| {
                let mut v = g.nodes.clone();
                v.sort();
                v
            })
            .collect();
        found.sort();
        assert_eq!(found, vec![vec![0, 1], vec![3, 4]]);
        assert_eq!(groups[0].wasted(), 200_000);
    }
}

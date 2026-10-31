// SPDX-License-Identifier: GPL-3.0-or-later
//! Live updates: watches the scanned folder and collects changed paths, so
//! the app can read just those folders again.

use eframe::egui;
use notify::{EventKind, RecursiveMode, Watcher as _};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Wait this long after the last change before updating ...
const QUIET: Duration = Duration::from_millis(1000);
/// ... but no longer than this while something keeps writing.
const MAX_WAIT: Duration = Duration::from_millis(3000);

#[derive(Default)]
struct Pending {
    paths: HashSet<PathBuf>,
    /// the system lost track of single changes below these folders
    rescan: HashSet<PathBuf>,
    first: Option<Instant>,
    last: Option<Instant>,
}

#[derive(Default)]
struct Shared {
    pending: Mutex<Pending>,
    /// set when watching could not be started
    error: Mutex<Option<String>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    stopped: AtomicBool,
}

pub struct Watcher {
    pub root: PathBuf,
    shared: Arc<Shared>,
}

/// Changed paths, translated to paths below the scan root.
pub struct Changes {
    pub paths: Vec<PathBuf>,
    pub rescan: Vec<PathBuf>,
}

impl Watcher {
    /// Start watching `root` and everything below it. Setting up the watch
    /// can take a while (Linux adds one watch per folder), so it runs on a
    /// background thread.
    pub fn start(root: &Path, ctx: &egui::Context) -> Watcher {
        let shared = Arc::new(Shared::default());
        let (root2, shared2, ctx2) = (root.to_path_buf(), shared.clone(), ctx.clone());
        std::thread::Builder::new()
            .name("watch".into())
            .spawn(move || {
                let map = PathMap::new(&root2);
                let events = shared2.clone();
                let ctx3 = ctx2.clone();
                let handler = move |res: notify::Result<notify::Event>| {
                    let Ok(ev) = res else { return };
                    if matches!(ev.kind, EventKind::Access(_)) {
                        return;
                    }
                    let Ok(mut p) = events.pending.lock() else { return };
                    let rescan = ev.need_rescan();
                    for path in ev.paths.iter().filter_map(|x| map.to_tree(x)) {
                        if rescan {
                            p.rescan.insert(path);
                        } else {
                            p.paths.insert(path);
                        }
                    }
                    let now = Instant::now();
                    p.first.get_or_insert(now);
                    p.last = Some(now);
                    ctx3.request_repaint_after(QUIET);
                };
                let res = notify::recommended_watcher(handler).and_then(|mut w| {
                    w.watch(&root2, RecursiveMode::Recursive)?;
                    Ok(w)
                });
                match res {
                    Ok(w) => {
                        if let Ok(mut slot) = shared2.watcher.lock() {
                            if !shared2.stopped.load(Relaxed) {
                                *slot = Some(w);
                            }
                        }
                    }
                    Err(e) => {
                        let msg = match e.kind {
                            notify::ErrorKind::MaxFilesWatch => {
                                "Live updates are off: too many folders to watch (raise fs.inotify.max_user_watches)".to_string()
                            }
                            _ => format!("Live updates are off: {e}"),
                        };
                        if let Ok(mut slot) = shared2.error.lock() {
                            *slot = Some(msg);
                        }
                        ctx2.request_repaint();
                    }
                }
            })
            .expect("spawn watch thread");
        Watcher { root: root.to_path_buf(), shared }
    }

    /// Why watching does not work, once.
    pub fn take_error(&self) -> Option<String> {
        self.shared.error.lock().ok()?.take()
    }

    /// The changes collected so far, once things have been quiet for a moment.
    pub fn take_ready(&self) -> Option<Changes> {
        let mut p = self.shared.pending.lock().ok()?;
        let (first, last) = (p.first?, p.last?);
        if last.elapsed() < QUIET && first.elapsed() < MAX_WAIT {
            return None;
        }
        let p = std::mem::take(&mut *p);
        Some(Changes { paths: p.paths.into_iter().collect(), rescan: p.rescan.into_iter().collect() })
    }

    /// Time until `take_ready` will return something, if anything is waiting.
    pub fn due_in(&self) -> Option<Duration> {
        let p = self.shared.pending.lock().ok()?;
        let (first, last) = (p.first?, p.last?);
        Some(QUIET.saturating_sub(last.elapsed()).min(MAX_WAIT.saturating_sub(first.elapsed())))
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // the watcher's handler holds `shared` too, so stop it explicitly
        if let Ok(mut slot) = self.shared.watcher.lock() {
            self.shared.stopped.store(true, Relaxed);
            slot.take();
        }
    }
}

/// Event paths can name the scan root differently from how it was typed:
/// through symlinks (/tmp is /private/tmp on macOS) or, on macOS, through the
/// data volume (/System/Volumes/Data/Users/... for /Users/...).
struct PathMap {
    root: PathBuf,
    canonical: Option<PathBuf>,
}

impl PathMap {
    fn new(root: &Path) -> PathMap {
        let canonical = std::fs::canonicalize(root).ok().filter(|c| c != root);
        PathMap { root: root.to_path_buf(), canonical }
    }

    fn to_tree(&self, p: &Path) -> Option<PathBuf> {
        if p.starts_with(&self.root) {
            return Some(p.to_path_buf());
        }
        if let Some(c) = &self.canonical {
            if let Ok(rel) = p.strip_prefix(c) {
                return Some(self.root.join(rel));
            }
        }
        if cfg!(target_os = "macos") {
            if let Ok(rel) = p.strip_prefix("/System/Volumes/Data") {
                let q = Path::new("/").join(rel);
                if q != p && q.starts_with(&self.root) {
                    return Some(q);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slow (waits for the system to deliver events): cargo test -- --ignored
    #[test]
    #[ignore]
    fn reports_changes_below_root() {
        let dir = std::env::temp_dir().join(format!("spacemapper-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let w = Watcher::start(&dir, &egui::Context::default());
        std::thread::sleep(Duration::from_millis(1500));
        std::fs::write(dir.join("sub/file"), b"x").unwrap();
        let mut got = None;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(200));
            if let Some(c) = w.take_ready() {
                got = Some(c);
                break;
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        let c = got.expect("no changes reported");
        assert!(w.take_error().is_none());
        assert!(c.paths.iter().chain(&c.rescan).any(|p| p == &dir.join("sub/file")), "{:?} {:?}", c.paths, c.rescan);
    }
}

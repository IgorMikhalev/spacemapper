// SPDX-License-Identifier: GPL-3.0-or-later
//! Remembers view settings in ~/.config/spacemapper/settings.conf
//! (plain "key = value" lines).

use super::*;

fn path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| util::home_dir().join(".config"));
    base.join("spacemapper").join("settings.conf")
}

impl App {
    pub(super) fn prefs_text(&self) -> String {
        let s = &self.settings;
        let color = match s.color {
            ColorMode::Sections => "sections",
            ColorMode::Mono => "grayscale",
            ColorMode::ByType => "type",
            ColorMode::ByAge => "age",
        };
        let age = match s.age_date {
            DateKind::Modified => "modified",
            DateKind::Added => "added",
            DateKind::Opened => "opened",
        };
        let size = match s.size_mode {
            SizeMode::OnDisk => "disk",
            SizeMode::Apparent => "length",
        };
        let tab = match self.panel.tab {
            panel::Tab::Contents => "contents",
            panel::Tab::Files => "largest",
            panel::Tab::Types => "types",
            panel::Tab::Duplicates => "duplicates",
            panel::Tab::Changes => "changes",
        };
        format!(
            "# Spacemapper settings, written automatically\n\
             color = {color}\nage_date = {age}\nsize = {size}\ndetail = {}\ndepth = {}\n\
             show_free = {}\nshow_unknown = {}\nother_drives = {}\n\
             list = {}\nlist_tab = {tab}\nduplicates_min = {}\n",
            s.detail, s.depth, s.show_free, s.show_unknown, s.cross_fs, self.panel.open, self.dupes.min_idx
        )
    }

    pub(super) fn load_prefs(&mut self) {
        let Ok(text) = std::fs::read_to_string(path()) else { return };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = v == "true";
            let s = &mut self.settings;
            match k {
                "color" => {
                    s.color = match v {
                        "grayscale" => ColorMode::Mono,
                        "type" => ColorMode::ByType,
                        "age" => ColorMode::ByAge,
                        _ => ColorMode::Sections,
                    }
                }
                "age_date" => {
                    s.age_date = match v {
                        "modified" => DateKind::Modified,
                        "opened" => DateKind::Opened,
                        _ => DateKind::Added,
                    }
                }
                "size" => s.size_mode = if v == "length" { SizeMode::Apparent } else { SizeMode::OnDisk },
                "detail" => s.detail = v.parse::<usize>().unwrap_or(s.detail).min(DETAIL_LEVELS.len() - 1),
                "depth" => s.depth = v.parse::<u32>().unwrap_or(s.depth).clamp(1, DEPTH_UNLIMITED),
                "show_free" => s.show_free = b,
                "show_unknown" => s.show_unknown = b,
                "other_drives" => s.cross_fs = b,
                "list" => self.panel.open = b,
                "list_tab" => {
                    self.panel.tab = match v {
                        "largest" => panel::Tab::Files,
                        "types" => panel::Tab::Types,
                        "duplicates" => panel::Tab::Duplicates,
                        "changes" => panel::Tab::Changes,
                        _ => panel::Tab::Contents,
                    }
                }
                "duplicates_min" => self.dupes.min_idx = v.parse::<usize>().unwrap_or(0).min(tools::DUP_MIN.len() - 1),
                _ => {}
            }
        }
        self.saved_prefs = self.prefs_text();
    }

    /// Write settings when they changed (checked every frame; it is a short string).
    pub(super) fn save_prefs_if_changed(&mut self) {
        let text = self.prefs_text();
        if text == self.saved_prefs {
            return;
        }
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&p, &text).is_ok() {
            self.saved_prefs = text;
        }
    }
}

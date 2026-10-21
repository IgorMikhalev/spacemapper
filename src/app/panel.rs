// SPDX-License-Identifier: GPL-3.0-or-later
//! Side panel with sortable lists: folder contents, largest files, file types.

use super::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const MAX_FILES: usize = 500;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Contents,
    Files,
    Types,
    Duplicates,
    Changes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SortBy {
    Size,
    Name,
    Modified,
    Count,
}

pub(super) struct TypeRow {
    pub ext: String,
    pub size: u64,
    pub count: u64,
}

pub(super) struct ListCache {
    key: (u32, u64, Tab, SortBy, bool),
    rows: Vec<u32>,
    types: Vec<TypeRow>,
    total: u64,
}

pub(super) struct PanelState {
    pub open: bool,
    pub tab: Tab,
    pub sort: SortBy,
    pub descending: bool,
    pub cache: Option<ListCache>,
    /// node hovered in the list, highlighted in the treemap
    pub hovered: Option<u32>,
    /// several nodes to highlight (copies of a duplicate)
    pub highlight: Vec<u32>,
}

impl Default for PanelState {
    fn default() -> Self {
        PanelState { open: true, tab: Tab::Contents, sort: SortBy::Size, descending: true, cache: None, hovered: None, highlight: Vec::new() }
    }
}

fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && ext.len() <= 10 && !ext.contains(' ') => {
            ext.to_lowercase()
        }
        _ => String::new(),
    }
}

enum Clicked {
    Select(u32),
    Open(u32),
    Menu(u32),
    Filter(String),
}

impl App {
    fn build_list(&self, tab: Tab, sort: SortBy, desc: bool) -> ListCache {
        let tree = self.tree.as_ref().unwrap();
        let vals = &self.vals;
        let root = self.view_root;
        let key = (root, self.ver, tab, sort, desc);
        let total = vals.size[root as usize];
        let mut rows: Vec<u32> = Vec::new();
        let mut types: Vec<TypeRow> = Vec::new();

        match tab {
            Tab::Contents | Tab::Duplicates | Tab::Changes => {
                rows = tree.nodes[root as usize]
                    .children
                    .iter()
                    .copied()
                    .filter(|&c| vals.size[c as usize] > 0)
                    .collect();
            }
            Tab::Files => {
                let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
                let mut stack = vec![root];
                while let Some(i) = stack.pop() {
                    let n = &tree.nodes[i as usize];
                    if n.is_dir() {
                        stack.extend_from_slice(&n.children);
                    } else if !n.is_pseudo() {
                        let v = vals.size[i as usize];
                        if v > 0 {
                            heap.push(Reverse((v, i)));
                            if heap.len() > MAX_FILES {
                                heap.pop();
                            }
                        }
                    }
                }
                rows = heap.into_iter().map(|Reverse((_, i))| i).collect();
            }
            Tab::Types => {
                let mut map: HashMap<String, (u64, u64)> = HashMap::new();
                let mut stack = vec![root];
                while let Some(i) = stack.pop() {
                    let n = &tree.nodes[i as usize];
                    if n.is_dir() {
                        stack.extend_from_slice(&n.children);
                    } else if !n.is_pseudo() {
                        let v = vals.size[i as usize];
                        if v > 0 {
                            let e = map.entry(extension(&n.name)).or_insert((0, 0));
                            e.0 += v;
                            e.1 += 1;
                        }
                    }
                }
                types = map.into_iter().map(|(ext, (size, count))| TypeRow { ext, size, count }).collect();
            }
        }

        // sorting
        let name_of = |i: u32| tree.nodes[i as usize].name.to_lowercase();
        match tab {
            Tab::Types => match sort {
                SortBy::Size | SortBy::Modified => types.sort_by_key(|t| t.size),
                SortBy::Name => types.sort_by(|a, b| a.ext.cmp(&b.ext)),
                SortBy::Count => types.sort_by_key(|t| t.count),
            },
            _ => match sort {
                SortBy::Size => rows.sort_by_key(|&i| vals.size[i as usize]),
                SortBy::Name => rows.sort_by_cached_key(|&i| name_of(i)),
                SortBy::Modified => rows.sort_by_key(|&i| tree.nodes[i as usize].mtime),
                SortBy::Count => rows.sort_by_key(|&i| vals.files[i as usize]),
            },
        }
        if desc {
            rows.reverse();
            types.reverse();
        }
        // free/unknown always at the end of the contents list
        rows.sort_by_key(|&i| tree.nodes[i as usize].is_pseudo());
        ListCache { key, rows, types, total }
    }

    pub(super) fn side_panel(&mut self, ui: &mut Ui) {
        let st = &mut self.panel;
        ui.add_space(8.0);
        // tabs
        ui.horizontal_wrapped(|ui| {
            group(ui, |ui| {
                for (tab, ic, label, tip) in [
                    (Tab::Contents, icon::FOLDER_SIMPLE, "Contents", "Items in the current folder"),
                    (Tab::Files, icon::FILE, "Largest", "Largest files in the current folder and below"),
                    (Tab::Types, icon::TAG, "Types", "Space used per file type"),
                ] {
                    if text_btn(ui, ic, label, tip, st.tab == tab).clicked() {
                        st.tab = tab;
                        if tab == Tab::Types && st.sort == SortBy::Modified {
                            st.sort = SortBy::Size;
                        }
                    }
                }
            });
            group(ui, |ui| {
                for (tab, ic, label, tip) in [
                    (Tab::Duplicates, icon::COPY, "Duplicates", "Files with identical contents"),
                    (Tab::Changes, icon::CHART_LINE_UP, "Changes", "What grew or shrank since an earlier scan"),
                ] {
                    if text_btn(ui, ic, label, tip, st.tab == tab).clicked() {
                        st.tab = tab;
                    }
                }
            });
        });
        ui.add_space(4.0);
        self.panel.highlight.clear();
        match self.panel.tab {
            Tab::Duplicates => return self.dupes_panel(ui),
            Tab::Changes => return self.changes_panel(ui),
            _ => {}
        }
        let st = &mut self.panel;
        // sorting
        ui.horizontal(|ui| {
            ui.label(RichText::new("Sort").size(12.5).color(TEXT_FAINT));
            let options: &[(SortBy, &str)] = match st.tab {
                Tab::Types => &[(SortBy::Size, "Size"), (SortBy::Count, "Files"), (SortBy::Name, "Name")],
                Tab::Files => &[(SortBy::Size, "Size"), (SortBy::Name, "Name"), (SortBy::Modified, "Modified")],
                _ => &[(SortBy::Size, "Size"), (SortBy::Count, "Files"), (SortBy::Name, "Name"), (SortBy::Modified, "Modified")],
            };
            group(ui, |ui| {
                for &(s, label) in options {
                    let b = egui::Button::new(RichText::new(label).size(12.5).color(if st.sort == s { ACCENT } else { TEXT }))
                        .selected(st.sort == s)
                        .min_size(vec2(0.0, 24.0));
                    if ui.add(b).clicked() {
                        if st.sort == s {
                            st.descending = !st.descending;
                        } else {
                            st.sort = s;
                            // sensible default direction per column
                            st.descending = !matches!(s, SortBy::Name);
                        }
                    }
                }
                let dir_icon = if st.descending { icon::SORT_DESCENDING } else { icon::SORT_ASCENDING };
                if icon_btn(ui, dir_icon, if st.descending { "Descending" } else { "Ascending" }, true, false).clicked() {
                    st.descending = !st.descending;
                }
            });
        });
        ui.add_space(6.0);

        let (tab, sort, desc) = (st.tab, st.sort, st.descending);
        let key = (self.view_root, self.ver, tab, sort, desc);
        if self.panel.cache.as_ref().map(|c| c.key != key).unwrap_or(true) {
            self.panel.cache = Some(self.build_list(tab, sort, desc));
        }
        let cache = self.panel.cache.as_ref().unwrap();
        let tree = self.tree.as_ref().unwrap();
        let total = cache.total.max(1);
        let row_h = 38.0;
        let mut clicked: Option<Clicked> = None;
        let mut hovered = None;

        let count = if tab == Tab::Types { cache.types.len() } else { cache.rows.len() };
        let caption = match tab {
            Tab::Contents | Tab::Duplicates | Tab::Changes => util::count_of(count as u64, "item"),
            Tab::Files if count >= MAX_FILES => format!("{MAX_FILES} largest files"),
            Tab::Files => util::count_of(count as u64, "file"),
            Tab::Types => util::count_of(count as u64, "file type"),
        };
        ui.label(RichText::new(caption).size(12.0).color(TEXT_FAINT));

        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, count, |ui, range| {
            for i in range {
                let (ic, name, sub, size, frac, node) = match tab {
                    Tab::Types => {
                        let t = &cache.types[i];
                        let name = if t.ext.is_empty() { "(no extension)".to_string() } else { format!(".{}", t.ext) };
                        let sub = util::count_of(t.count, "file");
                        (icon::TAG, name, sub, t.size, t.size as f32 / total as f32, None)
                    }
                    _ => {
                        let id = cache.rows[i];
                        let n = &tree.nodes[id as usize];
                        let v = self.vals.size[id as usize];
                        let ic = match n.kind {
                            Kind::Dir => icon::FOLDER_SIMPLE,
                            Kind::Free => icon::CIRCLE_DASHED,
                            Kind::Unknown => icon::QUESTION,
                            Kind::Link => icon::LINK,
                            _ => icon::FILE,
                        };
                        let name = match n.kind {
                            Kind::Free => "Free space".to_string(),
                            Kind::Unknown => "Unknown".to_string(),
                            _ => n.name.to_string(),
                        };
                        let sub = if n.is_pseudo() {
                            String::new()
                        } else if tab == Tab::Files {
                            // folder relative to the current view
                            let parent = tree.path(n.parent);
                            let base = tree.path(self.view_root);
                            let rel = parent.strip_prefix(&base).map(|p| p.to_path_buf()).unwrap_or(parent);
                            let rel = rel.to_string_lossy();
                            if rel.is_empty() { util::fmt_date(n.mtime) } else { format!("{rel}  ·  {}", util::fmt_date(n.mtime)) }
                        } else if n.is_dir() {
                            format!("{}  ·  {}", util::count_of(self.vals.files[id as usize], "file"), util::fmt_date(n.mtime))
                        } else {
                            util::fmt_date(n.mtime)
                        };
                        (ic, name, sub, v, v as f32 / total as f32, Some(id))
                    }
                };

                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                let selected = node.is_some() && self.selected.map(|t| t.node()) == node;
                let p = ui.painter();
                if selected {
                    p.rect_filled(rect, CornerRadius::same(7), ACCENT_DIM.gamma_multiply(0.6));
                } else if resp.hovered() {
                    p.rect_filled(rect, CornerRadius::same(7), SURFACE2);
                }
                // size bar along the bottom
                let bar = Rect::from_min_size(pos2(rect.min.x + 8.0, rect.max.y - 5.0), vec2((rect.width() - 16.0) * frac.clamp(0.0, 1.0), 2.0));
                p.rect_filled(Rect::from_min_size(bar.min, vec2(rect.width() - 16.0, 2.0)), CornerRadius::same(1), SURFACE3);
                p.rect_filled(bar, CornerRadius::same(1), ACCENT.gamma_multiply(0.75));

                let size_txt = fmt_size(size);
                let sg = p.layout_no_wrap(size_txt, font_bold(12.5), TEXT);
                let pct = p.layout_no_wrap(format!("{:.1}%", frac * 100.0), font(11.0), TEXT_FAINT);
                let right = rect.max.x - 8.0;
                p.galley(pos2(right - sg.size().x, rect.min.y + 4.0), sg.clone(), TEXT);
                p.galley(pos2(right - pct.size().x, rect.min.y + 20.0), pct, TEXT_FAINT);
                let text_w = rect.width() - 24.0 - sg.size().x.max(46.0);
                let painter = p.clone();
                if let Some(g) = fit_text(&painter, &format!("{ic}  {name}"), &font(13.0), TEXT, text_w) {
                    painter.galley(pos2(rect.min.x + 8.0, rect.min.y + 3.0), g, TEXT);
                }
                if !sub.is_empty() {
                    if let Some(g) = fit_text(&painter, &sub, &font(11.0), TEXT_FAINT, text_w - 18.0) {
                        painter.galley(pos2(rect.min.x + 26.0, rect.min.y + 20.0), g, TEXT_FAINT);
                    }
                }

                if resp.hovered() {
                    hovered = node;
                }
                match (tab, node) {
                    (Tab::Types, _) => {
                        let resp = resp.on_hover_text("Click to show only this file type");
                        if resp.clicked() {
                            let pat = if cache.types[i].ext.is_empty() { String::new() } else { format!("*.{}", cache.types[i].ext) };
                            clicked = Some(Clicked::Filter(pat));
                        }
                    }
                    (_, Some(id)) => {
                        if resp.double_clicked() {
                            clicked = Some(Clicked::Open(id));
                        } else if resp.clicked() {
                            clicked = Some(Clicked::Select(id));
                        } else if resp.secondary_clicked() {
                            clicked = Some(Clicked::Menu(id));
                        }
                        // keep the row's context menu alive while it is open
                        if resp.secondary_clicked() || (self.menu_from_list && self.menu_target == Some(Target::Node(id))) {
                            self.list_menu_response = Some(resp);
                        }
                    }
                    _ => {}
                }
            }
        });

        self.panel.hovered = hovered;
        match clicked {
            Some(Clicked::Select(id)) => self.selected = Some(Target::Node(id)),
            Some(Clicked::Open(id)) => {
                let tree = self.tree.as_ref().unwrap();
                let n = &tree.nodes[id as usize];
                if n.is_dir() {
                    self.go(id);
                } else if !n.is_pseudo() {
                    let p = n.parent;
                    if p != self.view_root {
                        self.go(p);
                    }
                    self.selected = Some(Target::Node(id));
                }
            }
            Some(Clicked::Menu(id)) => {
                self.menu_target = Some(Target::Node(id));
                self.menu_from_list = true;
                self.selected = Some(Target::Node(id));
            }
            Some(Clicked::Filter(pat)) => {
                if !pat.is_empty() {
                    self.filter_text = pat;
                    self.apply_filter();
                }
            }
            None => {}
        }
        if let Some(resp) = self.list_menu_response.take() {
            self.context_menu(&resp);
        }
    }
}

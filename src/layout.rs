// SPDX-License-Identifier: GPL-3.0-or-later
//! Nested squarified treemap layout (Bruls, Huizing & van Wijk).

use crate::scan::Kind;
use crate::tree::{Tree, Values};
use eframe::egui::{pos2, vec2, Rect};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemKind {
    /// folder whose children are drawn inside it
    DirOpen,
    /// folder drawn as a single block (too small or depth limit)
    DirClosed,
    File,
    Free,
    Unknown,
    /// several items too small to draw, collected into one block
    Group,
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub rect: Rect,
    pub kind: ItemKind,
    /// node id; for Group the id of the containing folder
    pub node: u32,
    pub depth: u16,
    pub value: u64,
    /// number of items in a Group
    pub count: u32,
    /// folder header drawn at the top of the box
    pub header: bool,
    /// index of the top-level section (child of the view root) this belongs to
    pub section: u16,
    /// first folder of a merged chain (folders that only contain one folder are
    /// drawn as a single box "a / b / c"); equals `node` when nothing was merged
    pub chain: u32,
}

pub struct Params {
    pub max_depth: u32,
    /// items smaller than this (in px, per side) are grouped
    pub min_box: f32,
    pub header_h: f32,
    pub pad: f32,
    /// space between sibling boxes
    pub gap: f32,
}

fn shrink_gap(r: Rect, g: f32) -> Rect {
    let gx = g.min((r.width() - 1.0).max(0.0) / 2.0);
    let gy = g.min((r.height() - 1.0).max(0.0) / 2.0);
    Rect::from_min_max(pos2(r.min.x + gx, r.min.y + gy), pos2(r.max.x - gx, r.max.y - gy))
}

pub fn layout(tree: &Tree, vals: &Values, root: u32, rect: Rect, p: &Params) -> Vec<Item> {
    let mut out = Vec::new();
    if vals.size[root as usize] == 0 || rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let mut lay = Layouter { tree, vals, p, out: &mut out };
    lay.place(root, rect, 0, 0);
    out
}

struct Layouter<'a> {
    tree: &'a Tree,
    vals: &'a Values,
    p: &'a Params,
    out: &'a mut Vec<Item>,
}

enum Entry {
    Node(u32),
    Group(u32, u64),
}

impl Layouter<'_> {
    fn place(&mut self, id: u32, rect: Rect, depth: u32, section: u16) {
        let rect = if depth > 0 { shrink_gap(rect, self.p.gap / 2.0) } else { rect };
        let node = &self.tree.nodes[id as usize];
        let value = self.vals.size[id as usize];
        let kind = match node.kind {
            Kind::Dir => ItemKind::DirOpen,
            Kind::Free => ItemKind::Free,
            Kind::Unknown => ItemKind::Unknown,
            _ => ItemKind::File,
        };
        if kind != ItemKind::DirOpen {
            self.out.push(Item { rect, kind, node: id, depth: depth as u16, value, count: 0, header: false, section, chain: id });
            return;
        }
        // Merge chains of folders that only contain a single folder, so the
        // same size isn't repeated in a stack of nested boxes.
        let chain = id;
        let mut id = id;
        if depth > 0 {
            // Also merge when one subfolder holds nearly everything and the rest
            // would be too small to see at this size anyway.
            let area = (rect.width() * rect.height()) as f64;
            let tiny = (self.p.min_box as f64 * 2.0).powi(2);
            loop {
                let n = &self.tree.nodes[id as usize];
                let total = self.vals.size[id as usize];
                let biggest = n
                    .children
                    .iter()
                    .copied()
                    .filter(|&c| self.vals.size[c as usize] > 0)
                    .max_by_key(|&c| self.vals.size[c as usize]);
                let Some(big) = biggest else { break };
                if self.tree.nodes[big as usize].kind != Kind::Dir || total == 0 {
                    break;
                }
                let rest = (total - self.vals.size[big as usize]) as f64 / total as f64;
                if rest == 0.0 || (rest < 0.03 && rest * area < tiny) {
                    id = big;
                } else {
                    break;
                }
            }
        }
        let node = &self.tree.nodes[id as usize];
        let p = self.p;
        let header = rect.height() >= p.header_h + 10.0 && rect.width() >= 36.0;
        let mut inner = rect;
        if header {
            inner.min.y += p.header_h;
        } else {
            inner.min.y += p.pad;
        }
        inner.min.x += p.pad;
        inner.max.x -= p.pad;
        inner.max.y -= p.pad;

        let can_open = depth < p.max_depth
            && inner.width() >= p.min_box.max(4.0)
            && inner.height() >= p.min_box.max(4.0)
            && !node.children.is_empty();

        let idx = self.out.len();
        self.out.push(Item {
            rect,
            kind: if can_open { ItemKind::DirOpen } else { ItemKind::DirClosed },
            node: id,
            depth: depth as u16,
            value,
            count: 0,
            header: header && can_open,
            section,
            chain,
        });
        if !can_open {
            return;
        }

        // visible children sorted by size, largest first
        let mut kids: Vec<(u32, u64)> = node
            .children
            .iter()
            .map(|&c| (c, self.vals.size[c as usize]))
            .filter(|&(_, v)| v > 0)
            .collect();
        if kids.is_empty() {
            self.out[idx].kind = ItemKind::DirClosed;
            self.out[idx].header = false;
            return;
        }
        kids.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        let total: u64 = kids.iter().map(|k| k.1).sum();
        let area = inner.width() * inner.height();
        let scale = area as f64 / total as f64;
        let min_area = (p.min_box * p.min_box) as f64;

        // cut off the tail of tiny items into one group
        let mut entries: Vec<(Entry, f64)> = Vec::with_capacity(kids.len().min(4096));
        let mut cut = kids.len();
        for (i, &(_, v)) in kids.iter().enumerate() {
            if (v as f64) * scale < min_area {
                cut = i;
                break;
            }
        }
        if kids.len() - cut == 1 {
            cut = kids.len(); // a single small item: just draw it
        }
        for &(c, v) in &kids[..cut] {
            entries.push((Entry::Node(c), v as f64 * scale));
        }
        if cut < kids.len() {
            let rest: u64 = kids[cut..].iter().map(|k| k.1).sum();
            entries.push((Entry::Group((kids.len() - cut) as u32, rest), rest as f64 * scale));
        }

        let areas: Vec<f64> = entries.iter().map(|e| e.1).collect();
        let mut rects = Vec::with_capacity(areas.len());
        squarify(&areas, inner, &mut rects);
        let mut next_section = 0u16;
        for ((entry, _), r) in entries.into_iter().zip(rects) {
            // free/unknown space don't use up a section colour
            let pseudo = matches!(entry, Entry::Node(c) if self.tree.nodes[c as usize].is_pseudo());
            let sec = if depth == 0 && !pseudo {
                next_section += 1;
                next_section - 1
            } else {
                section
            };
            match entry {
                Entry::Node(c) => self.place(c, r, depth + 1, sec),
                Entry::Group(count, v) => self.out.push(Item {
                    rect: shrink_gap(r, self.p.gap / 2.0),
                    section: sec,
                    chain: id,
                    kind: ItemKind::Group,
                    node: id,
                    depth: depth as u16 + 1,
                    value: v,
                    count,
                    header: false,
                }),
            }
        }
    }
}

fn worst(sum: f64, max: f64, min: f64, side: f64) -> f64 {
    let s2 = sum * sum;
    let w2 = side * side;
    (w2 * max / s2).max(s2 / (w2 * min))
}

/// Lay out `areas` (largest first, summing to the rect area) inside `rect`.
pub fn squarify(areas: &[f64], rect: Rect, out: &mut Vec<Rect>) {
    let (mut x0, mut y0) = (rect.min.x as f64, rect.min.y as f64);
    let (x1, y1) = (rect.max.x as f64, rect.max.y as f64);
    let mut i = 0;
    while i < areas.len() {
        let w = (x1 - x0).max(0.0);
        let h = (y1 - y0).max(0.0);
        let side = w.min(h).max(1e-9);
        let mut j = i + 1;
        let mut sum = areas[i];
        let (mut mx, mut mn) = (areas[i], areas[i]);
        let mut best = worst(sum, mx, mn, side);
        while j < areas.len() {
            let a = areas[j];
            let nsum = sum + a;
            let nw = worst(nsum, mx.max(a), mn.min(a), side);
            if nw > best {
                break;
            }
            best = nw;
            sum = nsum;
            mx = mx.max(a);
            mn = mn.min(a);
            j += 1;
        }
        if j == areas.len() {
            // last row takes whatever space is left (avoids float gaps)
        }
        if w >= h {
            // column on the left
            let cw = if j == areas.len() { w } else { (sum / h.max(1e-9)).min(w) };
            let mut y = y0;
            for k in i..j {
                let ch = if k + 1 == j { y1 - y } else { areas[k] / cw.max(1e-9) };
                out.push(Rect::from_min_size(pos2(x0 as f32, y as f32), vec2(cw as f32, ch.max(0.0) as f32)));
                y += ch;
            }
            x0 += cw;
        } else {
            // row on top
            let rh = if j == areas.len() { h } else { (sum / w.max(1e-9)).min(h) };
            let mut x = x0;
            for k in i..j {
                let cw = if k + 1 == j { x1 - x } else { areas[k] / rh.max(1e-9) };
                out.push(Rect::from_min_size(pos2(x as f32, y0 as f32), vec2(cw.max(0.0) as f32, rh as f32)));
                x += cw;
            }
            y0 += rh;
        }
        i = j;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn squarify_fills_rect() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0));
        let raw = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let total: f64 = raw.iter().sum();
        let areas: Vec<f64> = raw.iter().map(|a| a / total * 240000.0).collect();
        let mut out = Vec::new();
        squarify(&areas, rect, &mut out);
        assert_eq!(out.len(), raw.len());
        let covered: f32 = out.iter().map(|r| r.area()).sum();
        assert!((covered - 240000.0).abs() < 1.0, "{covered}");
        for (r, a) in out.iter().zip(&areas) {
            assert!(((r.area() as f64) - a).abs() / a < 0.01);
            assert!(rect.expand(0.01).contains_rect(*r));
        }
    }
}

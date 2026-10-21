// SPDX-License-Identifier: GPL-3.0-or-later
//! Filter syntax. Terms are separated by `;` and all must match:
//!
//!   *.mkv;*.mp4       files matching any of these name patterns
//!   cache             no wildcard = "name contains"
//!   /node_modules/    a pattern containing `/` is matched against the full path
//!   !*.log            exclude matching files
//!   >100mb  <2gb      size limits (b, kb, mb, gb, tb)
//!   >1y  <7d          modified more than a year ago / within the last 7 days
//!   added>1y          on this disk (installed, copied, extracted) for over a year
//!   added<7d          added to this disk or changed in the last 7 days
//!   opened>1y         not opened for over a year (needs atime, see below)
//!   empty             files of 0 bytes
//!   hidden            files inside hidden folders or starting with a dot
//!
//! Age units: h, d, w, mo, y. `modified>1y` is the same as `>1y`.

use crate::tree::DateKind;

pub struct Pat {
    kind: PatKind,
    path: bool,
}

/// Most patterns are a plain "contains", "ends with" or "starts with";
/// those are matched directly, only the rest goes through the general glob.
enum PatKind {
    Contains(String),
    Suffix(String),
    Prefix(String),
    Exact(String),
    Glob(Vec<char>),
}

impl Pat {
    fn matches(&self, s: &str) -> bool {
        match &self.kind {
            PatKind::Contains(t) => s.contains(t.as_str()),
            PatKind::Suffix(t) => s.ends_with(t.as_str()),
            PatKind::Prefix(t) => s.starts_with(t.as_str()),
            PatKind::Exact(t) => s == t,
            PatKind::Glob(g) => glob_match(g, s),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct AgeCond {
    kind: DateKind,
    /// true: date must be before `cutoff` (older), false: after it (newer)
    older: bool,
    cutoff: i64,
}

#[derive(Default)]
pub struct Filter {
    include: Vec<Pat>,
    exclude: Vec<Pat>,
    min_size: Option<u64>,
    max_size: Option<u64>,
    ages: Vec<AgeCond>,
    empty: bool,
    hidden: bool,
    pub needs_path: bool,
    pub needs_hidden: bool,
    pub error: Option<String>,
}

/// What the filter looks at for one file. `name` and `path` are lowercase.
pub struct FileInfo<'a> {
    pub name: &'a str,
    pub path: Option<&'a str>,
    pub size: u64,
    pub mtime: i64,
    pub ctime: i64,
    pub atime: i64,
    pub hidden: bool,
}

impl FileInfo<'_> {
    fn date(&self, kind: DateKind) -> i64 {
        match kind {
            DateKind::Modified => self.mtime,
            DateKind::Added => self.mtime.max(self.ctime),
            DateKind::Opened => self.atime.max(self.mtime),
        }
    }
}

fn make_pat(s: &str) -> Pat {
    let s = s.to_lowercase();
    let path = s.contains('/');
    let glob = if s.contains('*') || s.contains('?') { s.clone() } else { format!("*{s}*") };
    let lead = glob.starts_with('*');
    let trail = glob.ends_with('*') && glob.len() > 1;
    let inner = glob.trim_start_matches('*').trim_end_matches('*');
    let kind = if inner.contains('*') || inner.contains('?') {
        PatKind::Glob(glob.chars().collect())
    } else {
        let t = inner.to_string();
        match (lead, trail) {
            (true, true) => PatKind::Contains(t),
            (true, false) => PatKind::Suffix(t),
            (false, true) => PatKind::Prefix(t),
            (false, false) => PatKind::Exact(t),
        }
    };
    Pat { kind, path }
}

fn glob_match(p: &[char], s: &str) -> bool {
    let t: Vec<char> = s.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn parse_quantity(s: &str) -> Option<(f64, String)> {
    let s = s.trim().to_lowercase();
    let idx = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',')).unwrap_or(s.len());
    let num: f64 = s[..idx].replace(',', ".").parse().ok()?;
    Some((num, s[idx..].trim().to_string()))
}

enum Q {
    Size(u64),
    Age(i64),
}

fn quantity(s: &str) -> Option<Q> {
    let (n, unit) = parse_quantity(s)?;
    let size = |m: f64| Some(Q::Size((n * m) as u64));
    let age = |m: f64| Some(Q::Age((n * m) as i64));
    match unit.as_str() {
        "" | "b" => size(1.0),
        "k" | "kb" | "kib" => size(1024.0),
        "m" | "mb" | "mib" => size(1024.0 * 1024.0),
        "g" | "gb" | "gib" => size(1024.0 * 1024.0 * 1024.0),
        "t" | "tb" | "tib" => size(1024.0f64.powi(4)),
        "h" | "hour" | "hours" => age(3600.0),
        "d" | "day" | "days" => age(86400.0),
        "w" | "week" | "weeks" => age(7.0 * 86400.0),
        "mo" | "month" | "months" => age(30.0 * 86400.0),
        "y" | "year" | "years" => age(365.0 * 86400.0),
        _ => None,
    }
}

impl Filter {
    pub fn parse(text: &str, now: i64) -> Filter {
        let mut f = Filter::default();
        for raw in text.split(';') {
            let term = raw.trim();
            if term.is_empty() {
                continue;
            }
            let lower = term.to_lowercase();
            match lower.as_str() {
                "empty" => {
                    f.empty = true;
                    continue;
                }
                "hidden" => {
                    f.hidden = true;
                    f.needs_hidden = true;
                    continue;
                }
                _ => {}
            }
            // optional date keyword in front of > or <
            let (kind, cmp) = [
                ("modified", DateKind::Modified),
                ("added", DateKind::Added),
                ("opened", DateKind::Opened),
            ]
            .iter()
            .find_map(|(k, d)| lower.strip_prefix(k).map(|rest| (Some(*d), rest.trim_start())))
            .unwrap_or((None, lower.as_str()));
            let cmp = cmp.strip_prefix(':').map(str::trim_start).unwrap_or(cmp);

            if let Some(rest) = cmp.strip_prefix('>').or_else(|| cmp.strip_prefix('<')) {
                let greater = cmp.starts_with('>');
                let rest = rest.trim_start_matches('=');
                match (quantity(rest), kind) {
                    (Some(Q::Size(b)), None) => {
                        if greater {
                            f.min_size = Some(b)
                        } else {
                            f.max_size = Some(b)
                        }
                    }
                    (Some(Q::Age(secs)), k) => f.ages.push(AgeCond {
                        kind: k.unwrap_or(DateKind::Modified),
                        // ">30d" = older than 30 days, "<30d" = newer
                        older: greater,
                        cutoff: now - secs,
                    }),
                    _ => f.error = Some(format!("Can't understand \"{term}\"")),
                }
            } else if kind.is_some() {
                f.error = Some(format!("Use {term}>1y or {term}<7d"));
            } else if let Some(rest) = term.strip_prefix('!') {
                if !rest.is_empty() {
                    let p = make_pat(rest);
                    f.needs_path |= p.path;
                    f.exclude.push(p);
                }
            } else {
                let p = make_pat(term);
                f.needs_path |= p.path;
                f.include.push(p);
            }
        }
        f
    }

    pub fn is_active(&self) -> bool {
        !self.include.is_empty()
            || !self.exclude.is_empty()
            || self.min_size.is_some()
            || self.max_size.is_some()
            || !self.ages.is_empty()
            || self.empty
            || self.hidden
    }

    /// True when the filter uses last-opened dates.
    pub fn uses_opened(&self) -> bool {
        self.ages.iter().any(|a| a.kind == DateKind::Opened)
    }

    pub fn matches(&self, f: &FileInfo) -> bool {
        if self.empty && f.size != 0 {
            return false;
        }
        if self.hidden && !f.hidden {
            return false;
        }
        if let Some(m) = self.min_size {
            if f.size < m {
                return false;
            }
        }
        if let Some(m) = self.max_size {
            if f.size > m {
                return false;
            }
        }
        for a in &self.ages {
            let d = f.date(a.kind);
            if (a.older && d > a.cutoff) || (!a.older && d < a.cutoff) {
                return false;
            }
        }
        let hit = |p: &Pat| {
            if p.path {
                f.path.map(|s| p.matches(s)).unwrap_or(false)
            } else {
                p.matches(f.name)
            }
        };
        if self.exclude.iter().any(hit) {
            return false;
        }
        self.include.is_empty() || self.include.iter().any(hit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn globbing() {
        assert!(glob_match(&"*.mkv".chars().collect::<Vec<_>>(), "film.mkv"));
        assert!(!glob_match(&"*.mkv".chars().collect::<Vec<_>>(), "film.mkv.part"));
        assert!(glob_match(&"*cache*".chars().collect::<Vec<_>>(), ".cache"));
        assert!(glob_match(&"a?c".chars().collect::<Vec<_>>(), "abc"));
        // fast paths behave like the glob
        for (pat, yes, no) in [
            ("*.mkv", "film.mkv", "film.mkv.part"),
            ("cache", "my.cache.db", "cach"),
            ("img_*", "img_01.jpg", "my_img_01.jpg"),
            ("*a*b*", "xaxbx", "xbxax"),
            ("readme", "readme", "other"),
            ("*", "anything", ""),
        ] {
            let p = make_pat(pat);
            assert!(p.matches(yes), "{pat} should match {yes}");
            if !no.is_empty() {
                assert!(!p.matches(no), "{pat} should not match {no}");
            }
        }
    }
    fn file<'a>(name: &'a str, size: u64, mtime: i64) -> FileInfo<'a> {
        FileInfo { name, path: None, size, mtime, ctime: mtime, atime: mtime, hidden: false }
    }

    #[test]
    fn parsing() {
        let f = Filter::parse("*.mkv; >100mb; !*sample*; >30d", 1_000_000_000);
        assert!(f.is_active());
        assert!(f.matches(&file("movie.mkv", 200 << 20, 0)));
        assert!(!f.matches(&file("movie.mkv", 50 << 20, 0)));
        assert!(!f.matches(&file("sample.mkv", 200 << 20, 0)));
        assert!(!f.matches(&file("movie.mkv", 200 << 20, 999_999_999)));
        let g = Filter::parse("/node_modules/", 0);
        assert!(g.needs_path);
        let mut x = file("x.js", 1, 0);
        x.path = Some("/home/a/node_modules/x.js");
        assert!(g.matches(&x));
        x.path = Some("/home/a/src/x.js");
        assert!(!g.matches(&x));
        assert!(Filter::parse(">5zz", 0).error.is_some());
        assert!(Filter::parse("added", 0).error.is_some());
    }

    #[test]
    fn date_kinds() {
        let now = 100 * 86400 * 365;
        let year = 365 * 86400;
        // contents from 3 years ago, installed on this disk yesterday
        let installed = FileInfo { name: "lib.so", path: None, size: 10, mtime: now - 3 * year, ctime: now - 86400, atime: now - 3 * year, hidden: false };
        assert!(Filter::parse(">1y", now).matches(&installed));
        assert!(Filter::parse("modified>1y", now).matches(&installed));
        assert!(!Filter::parse("added>1y", now).matches(&installed));
        assert!(Filter::parse("added<7d", now).matches(&installed));
        assert!(Filter::parse("added: <7d", now).matches(&installed));
        assert!(Filter::parse("opened>1y", now).matches(&installed));
        assert!(Filter::parse("opened>1y", now).uses_opened());
        let mut opened = installed;
        opened.atime = now - 3600;
        assert!(!Filter::parse("opened>1y", now).matches(&opened));
    }

    #[test]
    fn keywords() {
        let mut f = file("a", 0, 0);
        assert!(Filter::parse("empty", 0).matches(&f));
        f.size = 1;
        assert!(!Filter::parse("empty", 0).matches(&f));
        let h = Filter::parse("hidden", 0);
        assert!(h.needs_hidden);
        assert!(!h.matches(&f));
        f.hidden = true;
        assert!(h.matches(&f));
    }
}

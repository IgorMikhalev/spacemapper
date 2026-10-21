// SPDX-License-Identifier: GPL-3.0-or-later
//! SpaceSniffer-like filter syntax.
//!
//! Terms are separated by `;`:
//!   *.mkv;*.mp4     show only files matching any of these name patterns
//!   cache           no wildcard = "name contains"
//!   /node_modules/  a pattern containing `/` is matched against the full path
//!   !*.log          exclude matching files
//!   >100mb  <2gb    size limits (b, kb, mb, gb, tb)
//!   >30d  <1w       age: older than 30 days / changed within the last week
//!                   (h, d, w, mo, y)

pub struct Pat {
    glob: Vec<char>,
    path: bool,
}

#[derive(Default)]
pub struct Filter {
    include: Vec<Pat>,
    exclude: Vec<Pat>,
    min_size: Option<u64>,
    max_size: Option<u64>,
    /// file mtime must be <= this
    older_than: Option<i64>,
    /// file mtime must be >= this
    newer_than: Option<i64>,
    pub needs_path: bool,
    pub error: Option<String>,
}

fn make_pat(s: &str) -> Pat {
    let s = s.to_lowercase();
    let path = s.contains('/');
    let glob: Vec<char> = if s.contains('*') || s.contains('?') {
        s.chars().collect()
    } else {
        format!("*{s}*").chars().collect()
    };
    Pat { glob, path }
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
            if let Some(rest) = term.strip_prefix('>').or_else(|| term.strip_prefix('<')) {
                let greater = term.starts_with('>');
                let rest = rest.trim_start_matches('=');
                match quantity(rest) {
                    Some(Q::Size(b)) => {
                        if greater {
                            f.min_size = Some(b)
                        } else {
                            f.max_size = Some(b)
                        }
                    }
                    Some(Q::Age(secs)) => {
                        // ">30d" = older than 30 days, "<30d" = newer
                        if greater {
                            f.older_than = Some(now - secs)
                        } else {
                            f.newer_than = Some(now - secs)
                        }
                    }
                    None => f.error = Some(format!("Can't understand \"{term}\"")),
                }
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
            || self.older_than.is_some()
            || self.newer_than.is_some()
    }

    /// `name` and `path` must already be lowercase.
    pub fn matches(&self, name: &str, path: Option<&str>, size: u64, mtime: i64) -> bool {
        if let Some(m) = self.min_size {
            if size < m {
                return false;
            }
        }
        if let Some(m) = self.max_size {
            if size > m {
                return false;
            }
        }
        if let Some(t) = self.older_than {
            if mtime > t {
                return false;
            }
        }
        if let Some(t) = self.newer_than {
            if mtime < t {
                return false;
            }
        }
        let hit = |p: &Pat| {
            if p.path {
                path.map(|s| glob_match(&p.glob, s)).unwrap_or(false)
            } else {
                glob_match(&p.glob, name)
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
    }
    #[test]
    fn parsing() {
        let f = Filter::parse("*.mkv; >100mb; !*sample*; >30d", 1_000_000_000);
        assert!(f.is_active());
        assert!(f.matches("movie.mkv", None, 200 << 20, 0));
        assert!(!f.matches("movie.mkv", None, 50 << 20, 0));
        assert!(!f.matches("sample.mkv", None, 200 << 20, 0));
        assert!(!f.matches("movie.mkv", None, 200 << 20, 999_999_999));
        let g = Filter::parse("/node_modules/", 0);
        assert!(g.needs_path);
        assert!(g.matches("x.js", Some("/home/a/node_modules/x.js"), 1, 0));
        assert!(!g.matches("x.js", Some("/home/a/src/x.js"), 1, 0));
        assert!(Filter::parse(">5zz", 0).error.is_some());
    }
}

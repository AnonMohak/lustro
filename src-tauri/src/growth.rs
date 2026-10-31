//! "What grew": the change of folder sizes between two scans of the same drive or folder,
//! and the used space of each drive over time, with a forecast of when it is full.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths::key;
use crate::scan::model::{Tree, flags};

/// Folders deeper than this below the scanned folder are not compared.
const DEPTH: usize = 4;
/// Smaller changes are noise.
const MIN_CHANGE: u64 = 10 * 1024 * 1024;
const MAX_CHANGES: usize = 15;
/// The space log keeps this many days for each drive.
const MAX_DAYS: usize = 730;
const REPORTS: &str = "growth.json";
const SPACE_LOG: &str = "space-history.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub path: String,
    pub before: u64,
    pub after: u64,
}

/// The change between two scans of one root.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub root: String,
    /// The times of the two scans (Unix seconds).
    pub from: u64,
    pub to: u64,
    /// The size of the root in the two scans.
    pub before: u64,
    pub after: u64,
    /// The folders that changed most, biggest change first.
    pub changes: Vec<Change>,
}

fn delta(c: &Change) -> i128 {
    c.after as i128 - c.before as i128
}

/// The size of every folder of `t` down to `DEPTH`, by path key (with its shown path).
fn folder_sizes(t: &Tree) -> HashMap<String, (String, u64)> {
    let mut out = HashMap::new();
    let mut stack = vec![(Tree::ROOT, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        for c in t.children(id) {
            let n = t.node(c);
            if !n.is_dir() || n.flags & flags::LINK != 0 {
                continue;
            }
            let path = t.path(c);
            out.insert(key(&path), (path, n.size));
            if depth + 1 < DEPTH {
                stack.push((c, depth + 1));
            }
        }
    }
    out
}

/// Compares two scans of the same root. A folder whose change comes mostly (80 % or more)
/// from one of its subfolders does not show; the subfolder does.
pub fn compare(old: &Tree, new: &Tree) -> Report {
    let (a, b) = (folder_sizes(old), folder_sizes(new));
    let mut changes: Vec<Change> = Vec::new();
    for (k, (path, after)) in &b {
        let before = a.get(k).map_or(0, |(_, s)| *s);
        changes.push(Change {
            path: path.clone(),
            before,
            after: *after,
        });
    }
    for (k, (path, before)) in &a {
        if !b.contains_key(k) {
            changes.push(Change {
                path: path.clone(),
                before: *before,
                after: 0,
            });
        }
    }
    changes.retain(|c| delta(c).unsigned_abs() >= MIN_CHANGE as u128);
    let explained = |p: &Change| {
        changes.iter().any(|c| {
            c.path != p.path
                && crate::paths::inside(&c.path, &p.path)
                && delta(c).signum() == delta(p).signum()
                && delta(c).unsigned_abs() * 5 >= delta(p).unsigned_abs() * 4
        })
    };
    let mut kept: Vec<Change> = changes.iter().filter(|p| !explained(p)).cloned().collect();
    kept.sort_by_key(|c| std::cmp::Reverse(delta(c).unsigned_abs()));
    kept.truncate(MAX_CHANGES);
    Report {
        root: new.root_path.clone(),
        from: old.scanned_at,
        to: new.scanned_at,
        before: old.total_size(),
        after: new.total_size(),
        changes: kept,
    }
}

fn gb(bytes: i128) -> String {
    let sign = if bytes < 0 { "−" } else { "+" };
    format!(
        "{sign}{:.1} GB",
        bytes.unsigned_abs() as f64 / 1024f64.powi(3)
    )
}

fn last_part(path: &str) -> &str {
    path.trim_end_matches(crate::paths::SEP)
        .rsplit(crate::paths::SEP)
        .next()
        .unwrap_or(path)
}

/// One line for a notification: "+12.3 GB since the last scan. Most: Downloads +8.1 GB, ...".
pub fn summary_line(r: &Report) -> String {
    let total = r.after as i128 - r.before as i128;
    let mut s = format!("{} since the last scan.", gb(total));
    let top: Vec<String> = r
        .changes
        .iter()
        .take(3)
        .map(|c| format!("{} {}", last_part(&c.path), gb(delta(c))))
        .collect();
    if !top.is_empty() {
        s.push_str(&format!(" Most: {}.", top.join(", ")));
    }
    s
}

pub fn load_reports(dir: &Path) -> HashMap<String, Report> {
    std::fs::read(dir.join(REPORTS))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_report(dir: &Path, r: &Report) -> std::io::Result<()> {
    let mut all = load_reports(dir);
    all.insert(r.root.clone(), r.clone());
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(REPORTS), serde_json::to_vec(&all)?)
}

/// The used space of a drive on one day.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Days since 1970-01-01.
    pub day: u32,
    pub used: u64,
    pub total: u64,
}

/// The used space of each drive (by root), one point a day.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SpaceLog {
    pub drives: HashMap<String, Vec<Point>>,
}

impl SpaceLog {
    pub fn load(dir: &Path) -> SpaceLog {
        std::fs::read(dir.join(SPACE_LOG))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(SPACE_LOG), serde_json::to_vec(self)?)
    }

    /// Records today's used space of a drive. A second record on one day replaces the first.
    pub fn record(&mut self, root: &str, p: Point) {
        let list = self.drives.entry(root.to_string()).or_default();
        match list.last_mut() {
            Some(last) if last.day == p.day => *last = p,
            _ => list.push(p),
        }
        if list.len() > MAX_DAYS {
            let extra = list.len() - MAX_DAYS;
            list.drain(..extra);
        }
    }
}

/// Days until the drive is full at the growth of the last 30 days (a straight line through
/// the points). None when the drive does not grow, or with less than 4 points in 7 days.
pub fn days_left(points: &[Point]) -> Option<u32> {
    let last = points.last()?;
    let recent: Vec<&Point> = points.iter().filter(|p| p.day + 30 >= last.day).collect();
    if recent.len() < 4 || last.day - recent[0].day < 7 {
        return None;
    }
    let n = recent.len() as f64;
    let mx = recent.iter().map(|p| p.day as f64).sum::<f64>() / n;
    let my = recent.iter().map(|p| p.used as f64).sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for p in &recent {
        sxy += (p.day as f64 - mx) * (p.used as f64 - my);
        sxx += (p.day as f64 - mx).powi(2);
    }
    let per_day = sxy / sxx;
    if per_day <= 0.0 {
        return None;
    }
    let free = last.total.saturating_sub(last.used) as f64;
    Some((free / per_day).min(3650.0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn forecast_from_steady_growth() {
        // 1 GB a day, 20 GB free.
        let pts: Vec<Point> = (0..10)
            .map(|d| Point {
                day: 100 + d,
                used: (70 + d as u64) * GB,
                total: 100 * GB,
            })
            .collect();
        assert_eq!(days_left(&pts), Some(21));
        assert_eq!(days_left(&pts[..3]), None, "too few points");
        let flat: Vec<Point> = pts
            .iter()
            .map(|p| Point {
                used: 50 * GB,
                ..*p
            })
            .collect();
        assert_eq!(days_left(&flat), None, "no growth");
    }

    #[test]
    fn one_point_a_day() {
        let mut log = SpaceLog::default();
        let p = |day, used| Point {
            day,
            used,
            total: 10,
        };
        log.record("C", p(1, 1));
        log.record("C", p(1, 2));
        log.record("C", p(2, 3));
        assert_eq!(log.drives["C"], vec![p(1, 2), p(2, 3)]);
    }

    #[test]
    fn the_growing_subfolder_shows_not_its_parent() {
        use crate::scan::{Progress, walk::walk};
        let root = std::env::temp_dir().join(format!("lustro-growth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("user/Downloads")).unwrap();
        std::fs::create_dir_all(root.join("user/Music")).unwrap();
        std::fs::write(root.join("user/Music/a.mp3"), vec![1u8; 1024]).unwrap();
        let old = walk(root.to_str().unwrap(), &Progress::default()).unwrap();
        std::fs::write(
            root.join("user/Downloads/big.iso"),
            vec![0u8; 12 * 1024 * 1024],
        )
        .unwrap();
        let new = walk(root.to_str().unwrap(), &Progress::default()).unwrap();

        let r = compare(&old, &new);
        let names: Vec<&str> = r.changes.iter().map(|c| last_part(&c.path)).collect();
        assert_eq!(names, ["Downloads"], "user grew only through Downloads");
        assert_eq!(r.changes[0].after - r.changes[0].before, 12 * 1024 * 1024);
        assert!(summary_line(&r).contains("Most: Downloads"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn summary_names_the_biggest_changes() {
        let line = summary_line(&Report {
            root: "C:".into(),
            from: 0,
            to: 1,
            before: 10 * GB,
            after: 20 * GB,
            changes: vec![Change {
                path: crate::paths::os(r"C:\Users\Downloads"),
                before: GB,
                after: 10 * GB,
            }],
        });
        assert_eq!(
            line,
            "+10.0 GB since the last scan. Most: Downloads +9.0 GB."
        );
    }
}

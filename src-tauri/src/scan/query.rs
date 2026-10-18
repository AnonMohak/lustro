//! Read-only queries over a scan tree.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use serde::Serialize;

use super::kind::Kind;
use super::model::Tree;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TreemapNode {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub kind: Kind,
    pub is_dir: bool,
    /// True for the "smaller items" block that stands for many small children.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub rest: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreemapNode>,
}

/// Nodes smaller than this share of the treemap area are merged into one "smaller items" block.
const MIN_SHARE: f64 = 1.0 / 4000.0;
const MAX_DEPTH: u32 = 12;
const MAX_CHILDREN: usize = 300;

/// A nested, pruned tree for the treemap of `id`. At most about `max_nodes` nodes.
pub fn treemap(t: &Tree, id: u32, max_nodes: usize) -> TreemapNode {
    let total = t.node(id).size.max(1) as f64;
    let mut budget = max_nodes;
    build(t, id, total, 0, &mut budget)
}

fn build(t: &Tree, id: u32, total: f64, depth: u32, budget: &mut usize) -> TreemapNode {
    let n = t.node(id);
    let mut out = TreemapNode {
        id,
        name: if id == Tree::ROOT {
            t.root_path.clone()
        } else {
            t.name(id).to_string()
        },
        size: n.size,
        kind: Kind::from_u8(n.kind),
        is_dir: n.is_dir(),
        rest: false,
        children: Vec::new(),
    };
    if !n.is_dir() || depth >= MAX_DEPTH || *budget == 0 {
        return out;
    }
    let mut shown = 0u64;
    let mut hidden = 0usize;
    // Children are sorted biggest first, so stop at the first one that is too small.
    for (i, c) in t.children(id).enumerate() {
        let cs = t.node(c).size;
        if cs == 0 {
            break;
        }
        if (cs as f64) / total < MIN_SHARE || i >= MAX_CHILDREN || *budget == 0 {
            hidden += 1;
            continue;
        }
        *budget -= 1;
        shown += cs;
        out.children.push(build(t, c, total, depth + 1, budget));
    }
    let rest = n.size.saturating_sub(shown);
    if hidden > 0 && rest > 0 {
        out.children.push(TreemapNode {
            id,
            name: format!("{hidden} smaller items"),
            size: rest,
            kind: Kind::Other,
            is_dir: false,
            rest: true,
            children: Vec::new(),
        });
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileSort {
    /// Biggest first.
    Size,
    /// Oldest first.
    Age,
}

pub struct FileFilter {
    pub sort: FileSort,
    pub min_size: u64,
    /// Only files with a FILETIME before this value (0 = no limit).
    pub older_than: u64,
    /// Allowed kinds; empty = all.
    pub kinds: Vec<Kind>,
    pub limit: usize,
}

/// Ids of the files that match, in sort order.
pub fn find_files(t: &Tree, f: &FileFilter) -> Vec<u32> {
    // Keep the best `limit` files in a heap whose top is the worst kept file.
    let mut heap: BinaryHeap<(u64, u32)> = BinaryHeap::new();
    let mut heap_rev: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
    for (i, n) in t.nodes.iter().enumerate() {
        if n.is_dir() || n.size < f.min_size || n.parent == super::model::NONE {
            continue;
        }
        if f.older_than > 0 && (n.modified == 0 || n.modified >= f.older_than) {
            continue;
        }
        if !f.kinds.is_empty() && !f.kinds.contains(&Kind::from_u8(n.kind)) {
            continue;
        }
        match f.sort {
            FileSort::Size => {
                heap_rev.push(Reverse((n.size, i as u32)));
                if heap_rev.len() > f.limit {
                    heap_rev.pop();
                }
            }
            FileSort::Age => {
                heap.push((n.modified, i as u32));
                if heap.len() > f.limit {
                    heap.pop();
                }
            }
        }
    }
    match f.sort {
        FileSort::Size => {
            let mut v: Vec<(u64, u32)> = heap_rev.into_iter().map(|Reverse(x)| x).collect();
            v.sort_by(|a, b| b.cmp(a));
            v.into_iter().map(|(_, id)| id).collect()
        }
        FileSort::Age => {
            let mut v = heap.into_vec();
            v.sort();
            v.into_iter().map(|(_, id)| id).collect()
        }
    }
}

/// What to search for. Text with `*` or `?` is a wildcard pattern for the whole name;
/// other text matches any part of the name. Case does not matter.
pub struct SearchQuery {
    pub text: String,
    pub kinds: Vec<Kind>,
    pub min_size: u64,
    pub folders: bool,
    pub limit: usize,
}

/// True when `hay` contains `needle_lower` (already lower case), ignoring ASCII case.
fn contains_ci(hay: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if !needle_lower.is_ascii() || !hay.is_ascii() {
        return hay.to_lowercase().contains(needle_lower);
    }
    let (h, n) = (hay.as_bytes(), needle_lower.as_bytes());
    h.windows(n.len())
        .any(|w| w.iter().zip(n).all(|(a, b)| a.to_ascii_lowercase() == *b))
}

/// Compares a part of a name with lower-case text, without allocating for ASCII text.
fn part_eq_ci(part: &str, lower: &str) -> bool {
    if lower.is_ascii() {
        part.len() == lower.len()
            && part
                .as_bytes()
                .iter()
                .zip(lower.as_bytes())
                .all(|(a, b)| a.to_ascii_lowercase() == *b)
    } else {
        part.to_lowercase() == lower
    }
}

fn ends_with_ci(hay: &str, suffix_lower: &str) -> bool {
    hay.len() >= suffix_lower.len()
        && hay.is_char_boundary(hay.len() - suffix_lower.len())
        && part_eq_ci(&hay[hay.len() - suffix_lower.len()..], suffix_lower)
}

fn starts_with_ci(hay: &str, prefix_lower: &str) -> bool {
    hay.len() >= prefix_lower.len()
        && hay.is_char_boundary(prefix_lower.len())
        && part_eq_ci(&hay[..prefix_lower.len()], prefix_lower)
}

/// A name matcher. Simple wildcard forms get fast paths; the rest use glob.
enum Matcher {
    Contains(String),
    Prefix(String),
    Suffix(String),
    Exact(String),
    Glob(glob::Pattern),
    Nothing,
}

impl Matcher {
    fn new(text: &str) -> Matcher {
        let lower = text.to_lowercase();
        if !text.contains(['*', '?']) {
            return Matcher::Contains(lower);
        }
        let inner = lower.trim_matches('*');
        let simple = !inner.contains(['*', '?', '[', ']']);
        match (lower.starts_with('*'), lower.ends_with('*'), simple) {
            (true, true, true) => Matcher::Contains(inner.to_string()),
            (true, false, true) => Matcher::Suffix(inner.to_string()),
            (false, true, true) => Matcher::Prefix(inner.to_string()),
            (false, false, true) => Matcher::Exact(inner.to_string()),
            _ => glob::Pattern::new(text)
                .map(Matcher::Glob)
                .unwrap_or(Matcher::Nothing),
        }
    }

    fn matches(&self, name: &str) -> bool {
        const OPTS: glob::MatchOptions = glob::MatchOptions {
            case_sensitive: false,
            require_literal_separator: false,
            require_literal_leading_dot: false,
        };
        match self {
            Matcher::Contains(n) => contains_ci(name, n),
            Matcher::Prefix(p) => starts_with_ci(name, p),
            Matcher::Suffix(s) => ends_with_ci(name, s),
            Matcher::Exact(e) => part_eq_ci(name, e),
            Matcher::Glob(p) => p.matches_with(name, OPTS),
            Matcher::Nothing => false,
        }
    }
}

/// Ids of matching nodes, biggest first, and the number of matches before the limit.
pub fn search(t: &Tree, q: &SearchQuery) -> (Vec<u32>, usize) {
    use rayon::prelude::*;
    let matcher = Matcher::new(q.text.trim());
    let mut hits: Vec<u32> = t
        .nodes
        .par_iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let id = i as u32;
            if id == Tree::ROOT || n.parent == super::model::NONE {
                return None;
            }
            if n.is_dir() && !q.folders {
                return None;
            }
            if n.size < q.min_size
                || (!q.kinds.is_empty()
                    && (n.is_dir() || !q.kinds.contains(&Kind::from_u8(n.kind))))
            {
                return None;
            }
            matcher.matches(t.name(id)).then_some(id)
        })
        .collect();
    let total = hits.len();
    // Only the biggest `limit` hits need to be in order.
    let by_size = |a: &u32, b: &u32| t.node(*b).size.cmp(&t.node(*a).size);
    if hits.len() > q.limit && q.limit > 0 {
        hits.select_nth_unstable_by(q.limit - 1, by_size);
        hits.truncate(q.limit);
    }
    hits.sort_unstable_by(by_size);
    (hits, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::model::tests::sample;

    /// Times search on 2 million names: `cargo test --profile perf -- --ignored search_speed --nocapture`.
    #[test]
    #[ignore]
    fn search_speed() {
        use crate::scan::model::{ScanMethod, TreeBuilder, flags};
        let mut b = TreeBuilder::new("C:", 2_000_100);
        let mut dirs = vec![0u32];
        for d in 0..2000 {
            let id = b.push(&format!("folder-{d}"), flags::DIR, 0, 0, 0);
            b.link(id, 0);
            dirs.push(id);
        }
        let exts = ["txt", "mp4", "iso", "dll", "rs", "jpg", "json", "pdf"];
        for i in 0..2_000_000u32 {
            let id = b.push(
                &format!("file-{i}-report.{}", exts[i as usize % exts.len()]),
                0,
                i as u64,
                0,
                0,
            );
            b.link(id, dirs[1 + (i as usize % 2000)]);
        }
        let t = b.finish("C:\\".into(), ScanMethod::Walk, 0, 0);
        for text in ["*.iso", "REPORT", "file-12345*", "nothing-here"] {
            let q = SearchQuery {
                text: text.into(),
                kinds: vec![],
                min_size: 0,
                folders: true,
                limit: 500,
            };
            let start = std::time::Instant::now();
            let (_, total) = search(&t, &q);
            println!("{text:<14} {total:>9} matches in {:?}", start.elapsed());
        }
    }

    #[test]
    fn searches_by_text_and_wildcard() {
        let t = sample();
        let q = |text: &str| SearchQuery {
            text: text.into(),
            kinds: vec![],
            min_size: 0,
            folders: true,
            limit: 10,
        };
        let names = |q: SearchQuery| {
            search(&t, &q)
                .0
                .into_iter()
                .map(|id| t.name(id).to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(q("MP4")), ["a.mp4"]);
        assert_eq!(names(q("*.sys")), ["pagefile.sys"]);
        assert_eq!(names(q("?.txt")), ["b.txt"]);
        assert_eq!(names(q("users")), ["Users"]);
        assert_eq!(names(q("PAGE*")), ["pagefile.sys"]);
        assert_eq!(names(q("*file*")), ["pagefile.sys"]);
        assert_eq!(names(q("b.TXT*")), ["b.txt"]);
        assert_eq!(
            names(q("[ab].*")),
            ["a.mp4", "b.txt"]
                .map(String::from)
                .into_iter()
                .filter(|n| n == "a.mp4" || n == "b.txt")
                .collect::<Vec<_>>()
        );
        let mut files_only = q("e");
        files_only.folders = false;
        assert!(names(files_only).iter().all(|n| n.contains('.')));
        assert_eq!(search(&t, &q("nothing-like-this")).1, 0);
    }

    #[test]
    fn case_insensitive_contains() {
        assert!(contains_ci("Trip-LADAKH.mp4", "ladakh"));
        assert!(contains_ci("Ünïcødé.txt", "ünï"));
        assert!(!contains_ci("abc", "abcd"));
    }

    #[test]
    fn treemap_includes_nested_children() {
        let t = sample();
        let m = treemap(&t, Tree::ROOT, 1000);
        assert_eq!(m.size, 200);
        assert_eq!(m.children.len(), 3);
        let users = &m.children[0];
        assert_eq!(users.name, "Users");
        assert_eq!(users.children[0].name, "me");
        assert_eq!(users.children[0].children.len(), 2);
    }

    #[test]
    fn treemap_respects_the_budget() {
        let t = sample();
        let m = treemap(&t, Tree::ROOT, 1);
        assert_eq!(m.children.iter().filter(|c| !c.rest).count(), 1);
        let rest = m.children.iter().find(|c| c.rest).unwrap();
        assert_eq!(rest.size, 90);
    }

    #[test]
    fn finds_biggest_files_first() {
        let t = sample();
        let f = FileFilter {
            sort: FileSort::Size,
            min_size: 0,
            older_than: 0,
            kinds: vec![],
            limit: 2,
        };
        let names: Vec<&str> = find_files(&t, &f)
            .into_iter()
            .map(|id| t.name(id))
            .collect();
        assert_eq!(names, ["a.mp4", "x.unknownext"]);
    }

    #[test]
    fn filters_by_size_and_kind() {
        let t = sample();
        let f = FileFilter {
            sort: FileSort::Size,
            min_size: 45,
            older_than: 0,
            kinds: vec![Kind::System],
            limit: 10,
        };
        let names: Vec<&str> = find_files(&t, &f)
            .into_iter()
            .map(|id| t.name(id))
            .collect();
        assert_eq!(names, ["x.unknownext"]);
    }
}

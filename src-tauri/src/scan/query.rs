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
const MAX_DEPTH: u32 = 4;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::model::tests::sample;

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

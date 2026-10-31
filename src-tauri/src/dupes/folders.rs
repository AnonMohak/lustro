//! Duplicate folders: folders with the same names, structure and content, for example a
//! backup copy of a project. First every folder gets a "shape" hash from its names, file
//! sizes and subfolder shapes, with no file reads. Folders with the same shape are then
//! compared by the content hashes of their files.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use super::cache::HashCache;
use super::{
    Candidate, DupeFile, DupeGroup, DupeProgress, Options, cached_full, filetime_ms, in_folder,
};
use crate::paths::key;
use crate::scan::model::{Tree, flags};

/// Smaller folders are not worth a row.
const MIN_FOLDER: u64 = 1024 * 1024;

/// A folder: the index of its scan and its node.
type Folder = (usize, u32);

/// The folders of `t` from the bottom up: each folder comes after all its subfolders.
fn post_order(t: &Tree, top: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = vec![(top, false)];
    while let Some((id, done)) = stack.pop() {
        if done {
            out.push(id);
            continue;
        }
        stack.push((id, true));
        for c in t.children(id) {
            if t.node(c).is_dir() {
                stack.push((c, false));
            }
        }
    }
    out
}

/// The hash of one folder from its entries. `file` gives the bytes that stand for a file
/// (its size, or its content hash); None means that the folder cannot be compared.
fn folder_hash(
    t: &Tree,
    id: u32,
    sub: &HashMap<u32, [u8; 32]>,
    file: &mut dyn FnMut(u32) -> Option<[u8; 32]>,
) -> Option<[u8; 32]> {
    let n = t.node(id);
    if n.flags & (flags::DENIED | flags::LINK) != 0 {
        return None;
    }
    let mut entries: Vec<(String, u8, [u8; 32])> = Vec::new();
    for c in t.children(id) {
        let cn = t.node(c);
        let name = key(t.name(c));
        if cn.is_dir() {
            entries.push((name, 1, *sub.get(&c)?));
        } else {
            if cn.flags & (flags::LINK | flags::ONLINE_ONLY) != 0 {
                return None;
            }
            entries.push((name, 0, file(c)?));
        }
    }
    entries.sort();
    let mut h = blake3::Hasher::new();
    for (name, kind, value) in &entries {
        h.update(name.as_bytes());
        h.update(&[0, *kind]);
        h.update(value);
    }
    Some(*h.finalize().as_bytes())
}

fn size_bytes(size: u64) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[..8].copy_from_slice(&size.to_le_bytes());
    b
}

/// The shape hash of every folder of `t` that can be compared.
fn shapes(t: &Tree) -> HashMap<u32, [u8; 32]> {
    let mut out = HashMap::new();
    for id in post_order(t, Tree::ROOT) {
        if let Some(h) = folder_hash(t, id, &out, &mut |c| Some(size_bytes(t.node(c).size))) {
            out.insert(id, h);
        }
    }
    out
}

fn unhex(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// The content hash of the folder `top`: file contents from `known` (the exact groups) or
/// from the hash cache and the files.
fn content(
    t: &Tree,
    top: u32,
    known: &HashMap<(String, u32), [u8; 32]>,
    p: &DupeProgress,
    cache: &Mutex<HashCache>,
) -> Option<[u8; 32]> {
    let mut sub = HashMap::new();
    let mut file = |c: u32| {
        if let Some(h) = known.get(&(t.root_path.clone(), c)) {
            return Some(*h);
        }
        let n = t.node(c);
        let cand = Candidate {
            root: t.root_path.clone(),
            id: c,
            path: t.path(c),
            size: n.size,
            modified: n.modified,
        };
        cached_full(&cand, n.size, p, cache)
    };
    for id in post_order(t, top) {
        if p.is_cancelled() {
            return None;
        }
        let h = folder_hash(t, id, &sub, &mut file)?;
        sub.insert(id, h);
    }
    sub.get(&top).copied()
}

/// Duplicate folders in the scans. `files` are the exact groups of the same search: their
/// hashes are used again. A folder whose parent is a duplicate of the other copies'
/// parents does not show; its parent does.
pub fn find_folders(
    trees: &[Arc<Tree>],
    o: &Options,
    files: &[DupeGroup],
    p: &DupeProgress,
    cache: &Mutex<HashCache>,
) -> Vec<DupeGroup> {
    let mut known = HashMap::new();
    for g in files {
        if let Some(h) = unhex(&g.hash) {
            for f in &g.files {
                known.insert((f.root.clone(), f.id), h);
            }
        }
    }

    // Folders with the same shape. A path in two scans counts once.
    let mut by_shape: HashMap<[u8; 32], Vec<Folder>> = HashMap::new();
    let mut seen = HashSet::new();
    for (ti, t) in trees.iter().enumerate() {
        for (id, h) in shapes(t) {
            let n = t.node(id);
            if id == Tree::ROOT || n.size < o.min_size.max(MIN_FOLDER) || n.items < 2 {
                continue;
            }
            let path = t.path(id);
            let lower = path.to_lowercase();
            let k = key(&path);
            if crate::ops::protect::is_protected(&path)
                || (!o.include_app_data
                    && (lower.contains("\\appdata\\") || lower.contains("\\programdata\\")))
                || (!o.only_in.is_empty() && !o.only_in.iter().any(|f| in_folder(&k, f)))
                || o.not_in.iter().any(|f| in_folder(&k, f))
                || !seen.insert(k)
            {
                continue;
            }
            by_shape.entry(h).or_default().push((ti, id));
        }
    }
    by_shape.retain(|_, v| v.len() > 1);

    // The same shape, checked by content.
    let verified: Vec<([u8; 32], Vec<Folder>)> = by_shape
        .into_par_iter()
        .flat_map(|(_, members)| {
            let mut by_content: HashMap<[u8; 32], Vec<Folder>> = HashMap::new();
            for (ti, id) in members {
                if let Some(h) = content(&trees[ti], id, &known, p, cache) {
                    by_content.entry(h).or_default().push((ti, id));
                }
            }
            by_content
                .into_iter()
                .filter(|(_, v)| v.len() > 1)
                .collect::<Vec<_>>()
        })
        .collect();

    // Only the top folders: drop a group when every copy's parent is a duplicate too.
    let all: HashSet<Folder> = verified
        .iter()
        .flat_map(|(_, v)| v.iter().copied())
        .collect();
    verified
        .into_iter()
        .filter(|(_, v)| {
            !v.iter()
                .all(|&(ti, id)| all.contains(&(ti, trees[ti].node(id).parent)))
        })
        .map(|(h, v)| {
            let (t0, id0) = v[0];
            let first = trees[t0].node(id0);
            p.groups.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            DupeGroup {
                size: first.size,
                hash: h.iter().map(|b| format!("{b:02x}")).collect(),
                kind: "folder",
                items: first.items,
                files: v
                    .into_iter()
                    .map(|(ti, id)| {
                        let t = &trees[ti];
                        let n = t.node(id);
                        DupeFile {
                            root: t.root_path.clone(),
                            id,
                            path: t.path(id),
                            modified: filetime_ms(n.modified),
                            size: n.size,
                            same: 0,
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

//! Duplicate finder: same size, then same partial hash, then same full hash (BLAKE3).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rayon::prelude::*;
use serde::Serialize;

use crate::scan::model::{NONE, Tree, flags};

const EDGE: u64 = 64 * 1024;

#[derive(Default)]
pub struct DupeProgress {
    pub total_bytes: AtomicU64,
    pub hashed_bytes: AtomicU64,
    pub groups: AtomicU64,
    cancel: AtomicBool,
}

impl DupeProgress {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeFile {
    pub root: String,
    pub id: u32,
    pub path: String,
    /// Unix time in milliseconds.
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeGroup {
    pub size: u64,
    pub hash: String,
    pub files: Vec<DupeFile>,
}

struct Candidate {
    root: String,
    id: u32,
    path: String,
    modified: u64,
}

fn filetime_ms(ft: u64) -> i64 {
    if ft == 0 {
        0
    } else {
        (ft as i64 - 116_444_736_000_000_000) / 10_000
    }
}

/// Volume serial and file index: the same value means the same file (a hard link).
#[cfg(windows)]
fn file_identity(f: &File) -> Option<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut info) }.ok()?;
    Some((
        info.dwVolumeSerialNumber as u64,
        ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
    ))
}

/// Device and inode: the same value means the same file (a hard link).
#[cfg(unix)]
fn file_identity(f: &File) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let m = f.metadata().ok()?;
    Some((m.dev(), m.ino()))
}

/// Hash of the first and last 64 KB (the whole file when it is small).
fn partial_hash(path: &str, size: u64, p: &DupeProgress) -> Option<([u8; 32], (u64, u64))> {
    let mut f = File::open(path).ok()?;
    let id = file_identity(&f)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; EDGE as usize];
    let n = f.read(&mut buf).ok()?;
    h.update(&buf[..n]);
    if size > EDGE * 2 {
        f.seek(SeekFrom::End(-(EDGE as i64))).ok()?;
        let n = f.read(&mut buf).ok()?;
        h.update(&buf[..n]);
    }
    p.hashed_bytes
        .fetch_add(size.min(EDGE * 2), Ordering::Relaxed);
    Some((*h.finalize().as_bytes(), id))
}

fn full_hash(path: &str, p: &DupeProgress) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        if p.is_cancelled() {
            return None;
        }
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        p.hashed_bytes.fetch_add(n as u64, Ordering::Relaxed);
    }
    Some(*h.finalize().as_bytes())
}

/// Finds groups of identical files of at least `min_size` bytes in the scans.
/// Groups are sorted by the space that deleting the extra copies frees.
/// Protected paths are skipped; AppData and ProgramData only with `include_app_data`.
pub fn find(
    trees: &[Arc<Tree>],
    min_size: u64,
    include_app_data: bool,
    p: &DupeProgress,
) -> Vec<DupeGroup> {
    // 1. Group by size. A path in two scans (a drive and a folder on it) counts once.
    let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    for t in trees {
        for (i, n) in t.nodes.iter().enumerate() {
            if n.is_dir()
                || n.size < min_size.max(1)
                || n.parent == NONE
                || n.flags & (flags::ONLINE_ONLY | flags::LINK) != 0
            {
                continue;
            }
            let path = t.path(i as u32);
            let lower = path.to_lowercase();
            if crate::ops::protect::is_protected(&path)
                || (!include_app_data
                    && (lower.contains("\\appdata\\") || lower.contains("\\programdata\\")))
                || !seen.insert(lower)
            {
                continue;
            }
            by_size.entry(n.size).or_default().push(Candidate {
                root: t.root_path.clone(),
                id: i as u32,
                path,
                modified: n.modified,
            });
        }
    }
    by_size.retain(|_, v| v.len() > 1);
    let total: u64 = by_size.iter().map(|(s, v)| s * v.len() as u64).sum();
    p.total_bytes.store(total, Ordering::Relaxed);

    // 2. Partial hash, per size group in parallel. Hard links of one file are kept once.
    let partial: Vec<(u64, Vec<Candidate>)> = by_size
        .into_par_iter()
        .flat_map(|(size, list)| {
            if p.is_cancelled() {
                return Vec::new();
            }
            let mut by_hash: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
            let mut ids = std::collections::HashSet::new();
            for c in list {
                if let Some((h, id)) = partial_hash(&c.path, size, p)
                    && ids.insert(id)
                {
                    by_hash.entry(h).or_default().push(c);
                }
            }
            by_hash
                .into_values()
                .filter(|v| v.len() > 1)
                .map(|v| (size, v))
                .collect::<Vec<_>>()
        })
        .collect();

    // 3. Full hash. Small files were hashed completely in step 2.
    let mut groups: Vec<DupeGroup> = partial
        .into_par_iter()
        .flat_map(|(size, list)| {
            if p.is_cancelled() {
                return Vec::new();
            }
            let mut by_hash: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
            for c in list {
                let h = if size <= EDGE * 2 {
                    partial_hash(&c.path, size, p).map(|(h, _)| h)
                } else {
                    full_hash(&c.path, p)
                };
                if let Some(h) = h {
                    by_hash.entry(h).or_default().push(c);
                }
            }
            by_hash
                .into_iter()
                .filter(|(_, v)| v.len() > 1)
                .map(|(h, v)| {
                    p.groups.fetch_add(1, Ordering::Relaxed);
                    DupeGroup {
                        size,
                        hash: h.iter().map(|b| format!("{b:02x}")).collect(),
                        files: v
                            .into_iter()
                            .map(|c| DupeFile {
                                root: c.root,
                                id: c.id,
                                path: c.path,
                                modified: filetime_ms(c.modified),
                            })
                            .collect(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * (g.files.len() as u64 - 1)));
    groups
}

// Unix: needs the protected paths of Phase 2b (until then every Unix path is protected).
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::scan::walk::walk;
    use std::fs;

    #[test]
    fn finds_exact_duplicates_only() {
        let root = std::env::temp_dir().join(format!("lustro-dupes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        fs::write(root.join("a/one.bin"), &big).unwrap();
        fs::write(root.join("b/copy.bin"), &big).unwrap();
        // Same size, same start and end, different middle: not a duplicate.
        let mut other = big.clone();
        other[150_000] ^= 0xFF;
        fs::write(root.join("b/almost.bin"), &other).unwrap();
        // Small identical files.
        fs::write(root.join("a/s1.txt"), b"hello world").unwrap();
        fs::write(root.join("b/s2.txt"), b"hello world").unwrap();
        // A hard link is the same file, not a copy.
        fs::hard_link(root.join("a/one.bin"), root.join("a/link.bin")).unwrap();

        let tree =
            Arc::new(walk(root.to_str().unwrap(), &crate::scan::Progress::default()).unwrap());
        let groups = find(&[tree.clone(), tree], 1, true, &DupeProgress::default());
        assert_eq!(groups.len(), 2, "{groups:#?}");
        let big_group = &groups[0];
        assert_eq!(big_group.size, 300_000);
        let mut names: Vec<String> = big_group
            .files
            .iter()
            .map(|f| f.path.rsplit(crate::paths::SEP).next().unwrap().to_string())
            .collect();
        names.sort();
        assert_eq!(
            names.len(),
            2,
            "one.bin and its hard link count once: {names:?}"
        );
        assert!(names.contains(&"copy.bin".to_string()));
        assert_eq!(groups[1].size, 11);
        // TEMP is inside AppData, so without app data nothing is found here.
        let tree =
            Arc::new(walk(root.to_str().unwrap(), &crate::scan::Progress::default()).unwrap());
        if root
            .to_string_lossy()
            .to_lowercase()
            .contains("\\appdata\\")
        {
            assert!(find(&[tree], 1, false, &DupeProgress::default()).is_empty());
        }
        fs::remove_dir_all(root).unwrap();
    }
}

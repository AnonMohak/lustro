//! Duplicate finder: same size, then same partial hash, then same full hash (BLAKE3).
//! The hash cache (`cache.rs`) keeps the hashes between searches.

pub mod cache;
mod folders;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::paths::{SEP, key};
use crate::scan::kind::{Kind, kind_of_name};
use crate::scan::model::{NONE, Tree, flags};
use cache::HashCache;

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

/// What a search looks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Files with the same content.
    #[default]
    Exact,
    /// Files with the same name, also copy names like `photo (1).jpg`. The content may differ.
    Name,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub min_size: u64,
    #[serde(default)]
    pub include_app_data: bool,
    #[serde(default)]
    pub mode: Mode,
    /// File types to search: "image", "video" (with music), "doc", "archive" and "other".
    /// Empty: all types.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Search only in these folders (empty: in all scans).
    #[serde(default)]
    pub only_in: Vec<String>,
    /// Never search in these folders.
    #[serde(default)]
    pub not_in: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeFile {
    pub root: String,
    pub id: u32,
    pub path: String,
    /// Unix time in milliseconds.
    pub modified: i64,
    pub size: u64,
    /// Name groups: files with the same number have the same content. 0 when no other
    /// file in the group has the same content.
    pub same: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupeGroup {
    /// Exact groups: the size of each copy. Name groups: the size of the biggest file.
    pub size: u64,
    /// Exact groups: the content hash. Name groups: the shared name.
    pub hash: String,
    /// "exact", "name" or "folder" (`files` are then the copies of the folder).
    pub kind: &'static str,
    /// The number of files in each copy: 1, or the files in a duplicate folder.
    pub items: u32,
    pub files: Vec<DupeFile>,
}

struct Candidate {
    root: String,
    id: u32,
    path: String,
    size: u64,
    modified: u64,
}

impl Candidate {
    fn name(&self) -> &str {
        self.path
            .rsplit(crate::paths::SEP)
            .next()
            .unwrap_or(&self.path)
    }

    fn into_file(self, same: u32) -> DupeFile {
        DupeFile {
            root: self.root,
            id: self.id,
            path: self.path,
            modified: filetime_ms(self.modified),
            size: self.size,
            same,
        }
    }
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

/// The partial hash and the file identity, from the cache or from the file.
fn cached_partial(
    c: &Candidate,
    size: u64,
    p: &DupeProgress,
    cache: &Mutex<HashCache>,
) -> Option<([u8; 32], (u64, u64))> {
    let hit = cache
        .lock()
        .ok()
        .and_then(|mut k| k.get(&c.path, size, c.modified));
    if let Some(cache::Entry {
        partial: Some(h),
        identity: Some(id),
        ..
    }) = hit
    {
        p.hashed_bytes
            .fetch_add(size.min(EDGE * 2), Ordering::Relaxed);
        return Some((h, id));
    }
    let (h, id) = partial_hash(&c.path, size, p)?;
    if let Ok(mut k) = cache.lock() {
        k.update(&c.path, size, c.modified, |e| {
            e.partial = Some(h);
            e.identity = Some(id);
        });
    }
    Some((h, id))
}

/// The hash of the whole file, from the cache or from the file. A small file's partial
/// hash already covers all of it.
fn cached_full(
    c: &Candidate,
    size: u64,
    p: &DupeProgress,
    cache: &Mutex<HashCache>,
) -> Option<[u8; 32]> {
    if size <= EDGE * 2 {
        return cached_partial(c, size, p, cache).map(|(h, _)| h);
    }
    let hit = cache
        .lock()
        .ok()
        .and_then(|mut k| k.get(&c.path, size, c.modified));
    if let Some(h) = hit.and_then(|e| e.full) {
        p.hashed_bytes.fetch_add(size, Ordering::Relaxed);
        return Some(h);
    }
    let h = full_hash(&c.path, p)?;
    if let Ok(mut k) = cache.lock() {
        k.update(&c.path, size, c.modified, |e| e.full = Some(h));
    }
    Some(h)
}

/// The type group of a file name for the type filter.
fn kind_key(name: &str) -> &'static str {
    match kind_of_name(name) {
        Kind::Image => "image",
        Kind::Video => "video",
        Kind::Doc => "doc",
        Kind::Archive => "archive",
        _ => "other",
    }
}

/// True when `path_key` (a `paths::key`) is `folder` or lies inside it.
fn in_folder(path_key: &str, folder: &str) -> bool {
    let f = key(folder.trim_end_matches(SEP));
    path_key == f
        || path_key
            .strip_prefix(f.as_str())
            .is_some_and(|r| r.starts_with(SEP))
}

/// The files of the scans that a search looks at. A path in two scans (a drive and a
/// folder on it) counts once.
fn candidates(trees: &[Arc<Tree>], o: &Options) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for t in trees {
        for (i, n) in t.nodes.iter().enumerate() {
            if n.is_dir()
                || n.size < o.min_size.max(1)
                || n.parent == NONE
                || n.flags & (flags::ONLINE_ONLY | flags::LINK) != 0
            {
                continue;
            }
            let id = i as u32;
            if !o.kinds.is_empty() && !o.kinds.iter().any(|k| k == kind_key(t.name(id))) {
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
                || !seen.insert(lower)
            {
                continue;
            }
            out.push(Candidate {
                root: t.root_path.clone(),
                id,
                path,
                size: n.size,
                modified: n.modified,
            });
        }
    }
    out
}

/// Finds duplicate files in the scans, the way `o.mode` says. Groups are sorted by the
/// space that deleting the extra copies frees. Protected paths are skipped; AppData and
/// ProgramData only with `include_app_data`.
pub fn find(
    trees: &[Arc<Tree>],
    o: &Options,
    p: &DupeProgress,
    cache: &Mutex<HashCache>,
) -> Vec<DupeGroup> {
    let list = candidates(trees, o);
    match o.mode {
        Mode::Exact => {
            let mut groups = exact(list, p, cache);
            // Folders hold files of all types, so the type filter turns them off.
            if o.kinds.is_empty() && !p.is_cancelled() {
                let dirs = folders::find_folders(trees, o, &groups, p, cache);
                with_folders(&mut groups, dirs);
            }
            groups
        }
        Mode::Name => by_name(list, p, cache),
    }
}

/// Adds the duplicate folders to the file groups. Files inside a duplicate folder leave
/// the file groups, so that one copy cannot go through its folder and the other one on
/// its own.
fn with_folders(groups: &mut Vec<DupeGroup>, dirs: Vec<DupeGroup>) {
    let inside: Vec<String> = dirs
        .iter()
        .flat_map(|d| d.files.iter().map(|f| key(&f.path)))
        .collect();
    for g in groups.iter_mut() {
        g.files.retain(|f| {
            let k = key(&f.path);
            !inside.iter().any(|d| in_folder(&k, d))
        });
    }
    groups.retain(|g| g.files.len() > 1);
    groups.extend(dirs);
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * (g.files.len() as u64 - 1)));
}

/// Groups of identical files: same size, then same partial hash, then same full hash.
fn exact(list: Vec<Candidate>, p: &DupeProgress, cache: &Mutex<HashCache>) -> Vec<DupeGroup> {
    let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
    for c in list {
        by_size.entry(c.size).or_default().push(c);
    }
    by_size.retain(|_, v| v.len() > 1);
    let total: u64 = by_size.iter().map(|(s, v)| s * v.len() as u64).sum();
    p.total_bytes.store(total, Ordering::Relaxed);

    // Partial hash, per size group in parallel. Hard links of one file are kept once.
    let partial: Vec<(u64, Vec<Candidate>)> = by_size
        .into_par_iter()
        .flat_map(|(size, list)| {
            if p.is_cancelled() {
                return Vec::new();
            }
            let mut by_hash: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
            let mut ids = HashSet::new();
            for c in list {
                if let Some((h, id)) = cached_partial(&c, size, p, cache)
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

    // Full hash. Small files were hashed completely in the partial step.
    let mut groups: Vec<DupeGroup> = partial
        .into_par_iter()
        .flat_map(|(size, list)| {
            if p.is_cancelled() {
                return Vec::new();
            }
            let mut by_hash: HashMap<[u8; 32], Vec<Candidate>> = HashMap::new();
            for c in list {
                if let Some(h) = cached_full(&c, size, p, cache) {
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
                        kind: "exact",
                        items: 1,
                        files: v.into_iter().map(|c| c.into_file(0)).collect(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * (g.files.len() as u64 - 1)));
    groups
}

/// `name` split into the part before the last dot and the extension (with the dot).
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// The stem without the marks that copies get: `(2)`, `- Copy`, `copy 3`, `Copy of`.
fn strip_copy_marks(stem: &str) -> &str {
    let mut s = stem.trim();
    loop {
        let before = s;
        if let Some(rest) = s.strip_prefix("copy of ") {
            s = rest.trim_start();
        }
        // `name (2)`
        if let Some(open) = s.rfind(" (")
            && s.ends_with(')')
            && s[open + 2..s.len() - 1].bytes().all(|b| b.is_ascii_digit())
            && s.len() > open + 3
        {
            s = s[..open].trim_end();
        }
        // `name copy 2`
        if let Some(i) = s.rfind(" copy ")
            && s[i + 6..].bytes().all(|b| b.is_ascii_digit())
            && s.len() > i + 6
        {
            s = s[..i].trim_end();
        }
        for mark in [" - copy", "-copy", "_copy", " copy"] {
            if let Some(rest) = s.strip_suffix(mark) {
                s = rest.trim_end();
            }
        }
        if s == before || s.is_empty() {
            return if s.is_empty() { before } else { s };
        }
    }
}

/// The name of a file without copy marks, in lower case: `Photo - Copy (2).JPG` gives
/// `photo.jpg`.
pub fn base_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let (stem, ext) = split_ext(&lower);
    format!("{}{ext}", strip_copy_marks(stem))
}

/// `photo 2.jpg` without the number (`photo.jpg`): the name that macOS gives a copy.
fn without_number(base: &str) -> Option<String> {
    let (stem, ext) = split_ext(base);
    let (head, num) = stem.rsplit_once(' ')?;
    let numeric = (1..=3).contains(&num.len()) && num.bytes().all(|b| b.is_ascii_digit());
    (numeric && !head.is_empty()).then(|| format!("{head}{ext}"))
}

/// Groups of files with the same name (without copy marks). A trailing number counts as
/// a copy mark only when the name without it exists too, so `Track 01` and `Track 02`
/// stay apart. In each group, files with the same size are hashed to show which ones
/// have the same content.
fn by_name(list: Vec<Candidate>, p: &DupeProgress, cache: &Mutex<HashCache>) -> Vec<DupeGroup> {
    let names: Vec<String> = list.iter().map(|c| base_name(c.name())).collect();
    let known: HashSet<&str> = names.iter().map(String::as_str).collect();
    let mut by_key: HashMap<String, Vec<Candidate>> = HashMap::new();
    for (c, n) in list.into_iter().zip(names.iter()) {
        let k = match without_number(n) {
            Some(b) if known.contains(b.as_str()) => b,
            _ => n.clone(),
        };
        by_key.entry(k).or_default().push(c);
    }
    by_key.retain(|_, v| v.len() > 1);

    // Only files that share a size with another file of their group need a hash.
    let total: u64 = by_key
        .values()
        .map(|v| {
            let mut sizes: HashMap<u64, u64> = HashMap::new();
            for c in v {
                *sizes.entry(c.size).or_default() += 1;
            }
            sizes
                .iter()
                .filter(|(_, n)| **n > 1)
                .map(|(s, n)| s * n)
                .sum::<u64>()
        })
        .sum();
    p.total_bytes.store(total, Ordering::Relaxed);

    let mut groups: Vec<DupeGroup> = by_key
        .into_par_iter()
        .filter_map(|(name, list)| {
            if p.is_cancelled() {
                return None;
            }
            let mut count: HashMap<u64, usize> = HashMap::new();
            for c in &list {
                *count.entry(c.size).or_default() += 1;
            }
            let hashes: Vec<Option<[u8; 32]>> = list
                .iter()
                .map(|c| {
                    (count[&c.size] > 1)
                        .then(|| cached_full(c, c.size, p, cache))
                        .flatten()
                })
                .collect();
            let mut seen: HashMap<[u8; 32], usize> = HashMap::new();
            for h in hashes.iter().flatten() {
                *seen.entry(*h).or_default() += 1;
            }
            // Number the contents that two or more files share: 1, 2, ...
            let mut numbers: HashMap<[u8; 32], u32> = HashMap::new();
            let mut files: Vec<DupeFile> = list
                .into_iter()
                .zip(hashes)
                .map(|(c, h)| {
                    let same = match h {
                        Some(h) if seen[&h] > 1 => {
                            let next = numbers.len() as u32 + 1;
                            *numbers.entry(h).or_insert(next)
                        }
                        _ => 0,
                    };
                    c.into_file(same)
                })
                .collect();
            files.sort_by_key(|f| f.modified);
            p.groups.fetch_add(1, Ordering::Relaxed);
            Some(DupeGroup {
                size: files.iter().map(|f| f.size).max().unwrap_or(0),
                hash: name,
                kind: "name",
                items: 1,
                files,
            })
        })
        .collect();
    let extra = |g: &DupeGroup| g.files.iter().map(|f| f.size).sum::<u64>() - g.size;
    groups.sort_by_key(|g| std::cmp::Reverse(extra(g)));
    groups
}

// macOS: needs its protected paths (until then every macOS path is protected).
#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;

    fn all() -> Options {
        Options {
            min_size: 1,
            include_app_data: true,
            ..Default::default()
        }
    }

    fn names(g: &DupeGroup) -> Vec<String> {
        let mut v: Vec<String> = g
            .files
            .iter()
            .map(|f| f.path.rsplit(crate::paths::SEP).next().unwrap().to_string())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn finds_same_names_and_marks_same_content() {
        let root = std::env::temp_dir().join(format!("lustro-dupes-name-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in ["a", "b", "c"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join("a/Report.docx"), b"version one").unwrap();
        fs::write(root.join("b/Report (1).docx"), b"version one").unwrap();
        fs::write(root.join("c/report - Copy.docx"), b"version two!").unwrap();
        fs::write(root.join("a/Track 01.mp3"), b"aaaa").unwrap();
        fs::write(root.join("b/Track 02.mp3"), b"bbbb").unwrap();
        fs::write(root.join("a/IMG_1.jpg"), b"x1").unwrap();
        fs::write(root.join("b/IMG_1 2.jpg"), b"x2").unwrap();
        let tree =
            Arc::new(walk(root.to_str().unwrap(), &crate::scan::Progress::default()).unwrap());
        let o = Options {
            mode: Mode::Name,
            ..all()
        };
        let groups = find(&[tree], &o, &DupeProgress::default(), &Mutex::default());
        assert_eq!(groups.len(), 2, "{groups:#?}");
        let report = groups.iter().find(|g| g.hash == "report.docx").unwrap();
        assert_eq!(report.kind, "name");
        assert_eq!(
            names(report),
            ["Report (1).docx", "Report.docx", "report - Copy.docx"]
        );
        let same: Vec<u32> = report.files.iter().map(|f| f.same).collect();
        assert_eq!(
            same.iter().filter(|&&n| n == 1).count(),
            2,
            "two have one content"
        );
        assert_eq!(
            same.iter().filter(|&&n| n == 0).count(),
            1,
            "the edited one is unique"
        );
        assert_eq!(report.size, 12);
        let img = groups.iter().find(|g| g.hash == "img_1.jpg").unwrap();
        assert_eq!(names(img), ["IMG_1 2.jpg", "IMG_1.jpg"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn filters_by_type_and_folder() {
        let root = std::env::temp_dir().join(format!("lustro-dupes-filter-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in ["keep", "skip"] {
            fs::create_dir_all(root.join(d)).unwrap();
            fs::write(root.join(d).join("a.jpg"), b"same photo").unwrap();
            fs::write(root.join(d).join("b.txt"), b"same text").unwrap();
        }
        fs::write(root.join("c.jpg"), b"same photo").unwrap();
        let tree =
            Arc::new(walk(root.to_str().unwrap(), &crate::scan::Progress::default()).unwrap());
        let run = |o: Options| {
            find(
                std::slice::from_ref(&tree),
                &o,
                &DupeProgress::default(),
                &Mutex::default(),
            )
        };

        let photos = run(Options {
            kinds: vec!["image".into()],
            ..all()
        });
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].files.len(), 3);

        let skip = root.join("skip").to_string_lossy().into_owned();
        let without_skip = run(Options {
            not_in: vec![skip],
            ..all()
        });
        assert_eq!(without_skip.len(), 1, "only the photo in keep and the root");

        let only_keep = run(Options {
            only_in: vec![root.join("keep").to_string_lossy().into_owned()],
            ..all()
        });
        assert!(only_keep.is_empty(), "one copy of each file in keep");
        fs::remove_dir_all(root).unwrap();
    }
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
        let groups = find(
            &[tree.clone(), tree],
            &all(),
            &DupeProgress::default(),
            &Mutex::default(),
        );
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
            assert!(
                find(
                    &[tree],
                    &Options {
                        include_app_data: false,
                        ..all()
                    },
                    &DupeProgress::default(),
                    &Mutex::default()
                )
                .is_empty()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finds_duplicate_folders() {
        let root = std::env::temp_dir().join(format!("lustro-dupes-dirs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let big: Vec<u8> = (0..1_100_000u32).map(|i| (i % 253) as u8).collect();
        for d in ["proj", "backup/proj", "other"] {
            fs::create_dir_all(root.join(d).join("src")).unwrap();
            fs::write(root.join(d).join("data.bin"), &big).unwrap();
            fs::write(root.join(d).join("src/main.rs"), b"fn main() {}").unwrap();
        }
        // Same shape, one small file differs: not a duplicate folder.
        fs::write(root.join("other/src/main.rs"), b"fn main() {;").unwrap();
        let tree =
            Arc::new(walk(root.to_str().unwrap(), &crate::scan::Progress::default()).unwrap());
        let groups = find(&[tree], &all(), &DupeProgress::default(), &Mutex::default());
        let dirs: Vec<&DupeGroup> = groups.iter().filter(|g| g.kind == "folder").collect();
        assert_eq!(dirs.len(), 1, "{groups:#?}");
        assert_eq!(dirs[0].items, 2);
        let mut paths: Vec<String> = dirs[0].files.iter().map(|f| f.path.clone()).collect();
        paths.sort();
        let sep = crate::paths::SEP;
        assert!(paths[0].ends_with(&format!("backup{sep}proj")), "{paths:?}");
        assert!(paths[1].ends_with(&format!("{sep}proj")));
        // data.bin of "other" still has copies in the two folders, but those copies now
        // belong to the folder group, so no file group is left with two files.
        assert!(groups.iter().all(|g| g.kind == "folder"), "{groups:#?}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_result_with_the_cache() {
        let root = std::env::temp_dir().join(format!("lustro-dupes-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 249) as u8).collect();
        fs::write(root.join("x.bin"), &big).unwrap();
        fs::write(root.join("y.bin"), &big).unwrap();
        let dir = root.join("data");
        let tree =
            Arc::new(walk(root.to_str().unwrap(), &crate::scan::Progress::default()).unwrap());

        let cache = Mutex::new(HashCache::load(&dir));
        let p1 = DupeProgress::default();
        let first = find(std::slice::from_ref(&tree), &all(), &p1, &cache);
        cache.lock().unwrap().save().unwrap();
        let cache = Mutex::new(HashCache::load(&dir));
        let p = DupeProgress::default();
        let second = find(&[tree], &all(), &p, &cache);
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_eq!(first[0].hash, second[0].hash);
        // Cached files count as read, so the progress bar moves the same way.
        assert_eq!(
            p.hashed_bytes.load(Ordering::Relaxed),
            p1.hashed_bytes.load(Ordering::Relaxed)
        );
        fs::remove_dir_all(root).unwrap();
    }
}

/// The copy-name rules are plain string checks, so they run on every platform.
#[cfg(test)]
mod name_tests {
    use super::*;

    #[test]
    fn copy_marks_go() {
        for (name, base) in [
            ("Photo.JPG", "photo.jpg"),
            ("photo (1).jpg", "photo.jpg"),
            ("photo (12).jpg", "photo.jpg"),
            ("photo - Copy.jpg", "photo.jpg"),
            ("photo - Copy (2).jpg", "photo.jpg"),
            ("photo copy.jpg", "photo.jpg"),
            ("photo copy 3.jpg", "photo.jpg"),
            ("Copy of photo.jpg", "photo.jpg"),
            ("photo_copy.jpg", "photo.jpg"),
            ("archive.tar.gz", "archive.tar.gz"),
            ("(1).txt", "(1).txt"),
            ("copy.txt", "copy.txt"),
            ("README", "readme"),
            ("photo (a).jpg", "photo (a).jpg"),
        ] {
            assert_eq!(base_name(name), base, "{name}");
        }
    }

    #[test]
    fn trailing_numbers() {
        assert_eq!(without_number("img_1 2.jpg").as_deref(), Some("img_1.jpg"));
        assert_eq!(without_number("track 01.mp3").as_deref(), Some("track.mp3"));
        assert_eq!(without_number("track.mp3"), None);
        assert_eq!(without_number("2.mp3"), None);
        assert_eq!(without_number("song 2024x.mp3"), None);
    }
}

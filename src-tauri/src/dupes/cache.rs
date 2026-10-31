//! The hash cache: the hashes of each file from earlier searches, so that the next search
//! reads only new or changed files. An entry counts only while the size and the change
//! time of the file stay the same.
//!
//! The file `dupe-hashes.tsv` in the app data folder has one line for each file:
//! `size  modified  dev  ino  partial  full  photo  path`, separated by tabs. A value that
//! is not known yet is `-`.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

const FILE: &str = "dupe-hashes.tsv";
/// The cache keeps at most this many files (the ones of the last searches).
const MAX_ENTRIES: usize = 1_000_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub size: u64,
    /// The change time from the scan (FILETIME).
    pub modified: u64,
    /// Volume and file index (Windows) or device and inode (Unix): hard links share it.
    pub identity: Option<(u64, u64)>,
    pub partial: Option<[u8; 32]>,
    pub full: Option<[u8; 32]>,
    /// The perceptual hash of a photo.
    pub photo: Option<u64>,
    /// True when this search used the entry. Entries that no search uses go first.
    used: bool,
}

#[derive(Default)]
pub struct HashCache {
    file: Option<PathBuf>,
    map: HashMap<String, Entry>,
    dirty: bool,
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
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

fn opt<T>(s: &str, f: impl FnOnce(&str) -> Option<T>) -> Option<T> {
    if s == "-" { None } else { f(s) }
}

fn parse_line(line: &str) -> Option<(String, Entry)> {
    let mut p = line.splitn(8, '\t');
    let size = p.next()?.parse().ok()?;
    let modified = p.next()?.parse().ok()?;
    let dev = p.next()?;
    let ino = p.next()?;
    let identity = opt(dev, |d| Some((d.parse().ok()?, ino.parse().ok()?)));
    let partial = opt(p.next()?, unhex);
    let full = opt(p.next()?, unhex);
    let photo = opt(p.next()?, |s| u64::from_str_radix(s, 16).ok());
    let path = p.next()?.to_string();
    Some((
        path,
        Entry {
            size,
            modified,
            identity,
            partial,
            full,
            photo,
            used: false,
        },
    ))
}

fn write_line(w: &mut impl Write, path: &str, e: &Entry) -> std::io::Result<()> {
    let (dev, ino) = e
        .identity
        .map_or(("-".to_string(), "-".to_string()), |(d, i)| {
            (d.to_string(), i.to_string())
        });
    writeln!(
        w,
        "{}\t{}\t{dev}\t{ino}\t{}\t{}\t{}\t{path}",
        e.size,
        e.modified,
        e.partial.as_ref().map_or("-".into(), hex),
        e.full.as_ref().map_or("-".into(), hex),
        e.photo.map_or("-".into(), |h| format!("{h:016x}")),
    )
}

impl HashCache {
    /// Loads the cache of `dir`. A missing or damaged file gives an empty cache.
    pub fn load(dir: &Path) -> HashCache {
        let file = dir.join(FILE);
        let map = fs::File::open(&file)
            .map(|f| {
                BufReader::new(f)
                    .lines()
                    .map_while(Result::ok)
                    .filter_map(|l| parse_line(&l))
                    .collect()
            })
            .unwrap_or_default();
        HashCache {
            file: Some(file),
            map,
            dirty: false,
        }
    }

    /// The entry of `path`, when the file did not change since it was hashed.
    pub fn get(&mut self, path: &str, size: u64, modified: u64) -> Option<Entry> {
        let e = self.map.get_mut(path)?;
        if e.size != size || e.modified != modified {
            return None;
        }
        e.used = true;
        Some(e.clone())
    }

    /// Changes the entry of `path`. A changed file starts a new entry.
    pub fn update(&mut self, path: &str, size: u64, modified: u64, f: impl FnOnce(&mut Entry)) {
        // Tabs and line breaks would break the file format.
        if path.contains(['\t', '\n', '\r']) {
            return;
        }
        let e = self.map.entry(path.to_string()).or_default();
        if e.size != size || e.modified != modified {
            *e = Entry {
                size,
                modified,
                ..Default::default()
            };
        }
        e.used = true;
        f(e);
        self.dirty = true;
    }

    /// Writes the cache when something changed. Over the limit, unused entries go first.
    pub fn save(&mut self) -> std::io::Result<()> {
        let Some(file) = self.file.clone() else {
            return Ok(());
        };
        if !self.dirty {
            return Ok(());
        }
        if self.map.len() > MAX_ENTRIES {
            self.map.retain(|_, e| e.used);
        }
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = file.with_extension("tmp");
        let mut w = BufWriter::new(fs::File::create(&tmp)?);
        for (path, e) in &self.map {
            write_line(&mut w, path, e)?;
        }
        w.flush()?;
        drop(w);
        fs::rename(&tmp, &file)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_entries_until_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("lustro-hashcache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut c = HashCache::load(&dir);
        assert!(c.get("/a/b.jpg", 10, 5).is_none());
        c.update("/a/b.jpg", 10, 5, |e| {
            e.identity = Some((1, 2));
            e.partial = Some([7; 32]);
            e.photo = Some(0xdead_beef);
        });
        c.update("/a/c d.bin", 20, 6, |e| e.full = Some([9; 32]));
        c.update("/bad\tname", 1, 1, |e| e.full = Some([1; 32]));
        c.save().unwrap();

        let mut c = HashCache::load(&dir);
        let e = c.get("/a/b.jpg", 10, 5).unwrap();
        assert_eq!(e.identity, Some((1, 2)));
        assert_eq!(e.partial, Some([7; 32]));
        assert_eq!(e.full, None);
        assert_eq!(e.photo, Some(0xdead_beef));
        assert_eq!(c.get("/a/c d.bin", 20, 6).unwrap().full, Some([9; 32]));
        assert!(c.get("/a/b.jpg", 11, 5).is_none(), "the size changed");
        assert!(
            c.get("/a/b.jpg", 10, 6).is_none(),
            "the change time changed"
        );
        assert!(c.get("/bad\tname", 1, 1).is_none());

        // A changed file starts again with no hashes.
        c.update("/a/b.jpg", 11, 5, |_| {});
        assert_eq!(c.get("/a/b.jpg", 11, 5).unwrap().partial, None);
        fs::remove_dir_all(dir).unwrap();
    }
}

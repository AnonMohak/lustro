//! Saves and loads a scan result. The file is the raw node arena plus the name pool.

use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use super::model::{Node, ScanMethod, Tree};

const MAGIC: &[u8; 8] = b"LUSTRO\0\x01";
/// Version 2 adds the USN journal position and the MFT record map.
const VERSION: u32 = 2;

/// Cache file name for a scan root: `C.bin` for `C:\`, a hash for a folder.
pub fn file_name(root: &str) -> String {
    if let Some(letter) = super::is_drive_root(root) {
        return format!("{letter}.bin");
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in root.to_lowercase().bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    format!("folder-{h:016x}.bin")
}

pub fn save(tree: &Tree, dir: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(file_name(&tree.root_path));
    let tmp = path.with_extension("tmp");
    {
        let mut w = BufWriter::new(fs::File::create(&tmp)?);
        w.write_all(MAGIC)?;
        w.write_all(&VERSION.to_le_bytes())?;
        w.write_all(&[tree.method as u8])?;
        w.write_all(&tree.scanned_at.to_le_bytes())?;
        w.write_all(&tree.elapsed_ms.to_le_bytes())?;
        w.write_all(&tree.denied.to_le_bytes())?;
        let root = tree.root_path.as_bytes();
        w.write_all(&(root.len() as u32).to_le_bytes())?;
        w.write_all(root)?;
        w.write_all(&(tree.nodes.len() as u64).to_le_bytes())?;
        w.write_all(bytemuck::cast_slice(&tree.nodes))?;
        w.write_all(&(tree.names.len() as u64).to_le_bytes())?;
        w.write_all(tree.names.as_bytes())?;
        let (journal, next) = tree.usn.unwrap_or((0, -1));
        w.write_all(&journal.to_le_bytes())?;
        w.write_all(&next.to_le_bytes())?;
        w.write_all(&(tree.frn.len() as u64).to_le_bytes())?;
        w.write_all(bytemuck::cast_slice(&tree.frn))?;
        w.flush()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(path)
}

fn take<'a>(b: &mut &'a [u8], n: usize) -> std::io::Result<&'a [u8]> {
    if b.len() < n {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "cache file is cut off",
        ));
    }
    let (head, rest) = b.split_at(n);
    *b = rest;
    Ok(head)
}

fn u32_of(b: &mut &[u8]) -> std::io::Result<u32> {
    Ok(u32::from_le_bytes(take(b, 4)?.try_into().unwrap()))
}

fn u64_of(b: &mut &[u8]) -> std::io::Result<u64> {
    Ok(u64::from_le_bytes(take(b, 8)?.try_into().unwrap()))
}

pub fn load(path: &Path) -> std::io::Result<Tree> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    let mut data = Vec::new();
    fs::File::open(path)?.read_to_end(&mut data)?;
    let mut b = data.as_slice();
    if take(&mut b, 8)? != MAGIC || u32_of(&mut b)? != VERSION {
        return Err(bad("not a Lustro cache file of this version"));
    }
    let method = match take(&mut b, 1)?[0] {
        1 => ScanMethod::Mft,
        2 => ScanMethod::Walk,
        _ => return Err(bad("unknown scan method")),
    };
    let scanned_at = u64_of(&mut b)?;
    let elapsed_ms = u64_of(&mut b)?;
    let denied = u32_of(&mut b)?;
    let root_len = u32_of(&mut b)? as usize;
    let root_path =
        String::from_utf8(take(&mut b, root_len)?.to_vec()).map_err(|_| bad("bad root path"))?;
    let count = u64_of(&mut b)? as usize;
    let node_bytes = take(
        &mut b,
        count
            .checked_mul(size_of::<Node>())
            .ok_or_else(|| bad("too many nodes"))?,
    )?;
    // The byte slice may be unaligned, so copy instead of casting in place.
    let nodes: Vec<Node> = bytemuck::pod_collect_to_vec(node_bytes);
    let names_len = u64_of(&mut b)? as usize;
    let names =
        String::from_utf8(take(&mut b, names_len)?.to_vec()).map_err(|_| bad("bad names"))?;
    let journal = u64_of(&mut b)?;
    let next = u64_of(&mut b)? as i64;
    let usn = (next >= 0).then_some((journal, next));
    let frn_len = u64_of(&mut b)? as usize;
    let frn_bytes = take(
        &mut b,
        frn_len
            .checked_mul(4)
            .ok_or_else(|| bad("too many records"))?,
    )?;
    let frn: Vec<u32> = bytemuck::pod_collect_to_vec(frn_bytes);
    // Every link and name range must stay in bounds, so a damaged file cannot cause a panic later.
    let n = nodes.len() as u32;
    let valid = |v: u32| v == super::model::NONE || v < n;
    let ok = !nodes.is_empty()
        && nodes.iter().all(|x| {
            valid(x.parent)
                && valid(x.first_child)
                && valid(x.next_sibling)
                && (x.name_off as usize + x.name_len as usize) <= names.len()
                && names.is_char_boundary(x.name_off as usize)
                && names.is_char_boundary(x.name_off as usize + x.name_len as usize)
        })
        && frn.iter().all(|&v| valid(v));
    if !ok {
        return Err(bad("damaged cache file"));
    }
    Ok(Tree {
        nodes,
        names,
        root_path,
        method,
        scanned_at,
        elapsed_ms,
        denied,
        frn,
        usn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::model::tests::sample;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("lustro-cache-{}", std::process::id()));
        let mut tree = sample();
        tree.usn = Some((7, 99));
        tree.frn = vec![crate::scan::model::NONE, 0, 1];
        let path = save(&tree, &dir).unwrap();
        assert!(path.ends_with("C.bin"));
        let back = load(&path).unwrap();
        assert_eq!(back.total_size(), tree.total_size());
        assert_eq!(back.root_path, "C:\\");
        assert_eq!(back.nodes.len(), tree.nodes.len());
        let first = back.children(0).next().unwrap();
        assert_eq!(back.name(first), "Users");
        assert_eq!(back.usn, Some((7, 99)));
        assert_eq!(back.frn, tree.frn);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_damaged_files() {
        let dir = std::env::temp_dir().join(format!("lustro-cache-bad-{}", std::process::id()));
        let path = save(&sample(), &dir).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - 3);
        fs::write(&path, &bytes).unwrap();
        assert!(load(&path).is_err());
        fs::write(&path, b"garbage").unwrap();
        assert!(load(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name("C:\\"), "C.bin");
        assert!(file_name("D:\\Photos\\").starts_with("folder-"));
        assert_eq!(file_name("D:\\Photos\\"), file_name("d:\\photos\\"));
    }
}

//! The macOS Trash. The `trash` crate moves items there, but it cannot list or restore
//! them, so Lustro finds them by name. The Finder keeps the original folder in a
//! `.DS_Store` file that Lustro does not read.
//!
//! Each volume has its own Trash: `~/.Trash` for the system volume, and
//! `/Volumes/<disk>/.Trashes/<uid>` for other disks. Reading them needs Full Disk Access.

use std::path::{Path, PathBuf};

fn home_trash() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| Path::new(&h).join(".Trash"))
}

fn uid() -> u32 {
    // SAFETY: getuid has no failure case.
    unsafe { libc::getuid() }
}

/// The Trash folder that gets the deleted items from `path`.
fn trash_of(path: &str) -> Option<PathBuf> {
    match path
        .strip_prefix("/Volumes/")
        .and_then(|r| r.split('/').next())
    {
        Some(disk) if !disk.is_empty() => Some(
            Path::new("/Volumes")
                .join(disk)
                .join(".Trashes")
                .join(uid().to_string()),
        ),
        _ => home_trash(),
    }
}

/// All Trash folders of this user that exist.
pub fn folders() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = home_trash().into_iter().collect();
    for e in std::fs::read_dir("/Volumes")
        .into_iter()
        .flatten()
        .flatten()
    {
        out.push(e.path().join(".Trashes").join(uid().to_string()));
    }
    out.retain(|p| p.is_dir());
    out
}

/// The item in the Trash that was deleted from `original`: an item with the same name in
/// the Trash of its volume. The Finder renames an item when the Trash already has that
/// name (`photo 2.jpg`); Lustro does not find such items.
pub fn find(original: &str) -> Option<PathBuf> {
    let name = Path::new(original).file_name()?;
    let p = trash_of(original)?.join(name);
    std::fs::symlink_metadata(&p).is_ok().then_some(p)
}

/// Moves the item back to `original`. Fails when an item with that path exists again.
pub fn restore(item: &Path, original: &str) -> std::io::Result<()> {
    if std::fs::symlink_metadata(original).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "an item with this name is in the folder again",
        ));
    }
    std::fs::rename(item, original)
}

/// Deletes everything in all Trash folders. Returns true when nothing stayed.
pub fn empty() -> bool {
    let mut ok = true;
    for folder in folders() {
        for e in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let p = e.path();
            let r = match e.file_type() {
                Ok(t) if t.is_dir() => std::fs::remove_dir_all(&p),
                _ => std::fs::remove_file(&p),
            };
            ok &= r.is_ok();
        }
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_volume_has_its_trash() {
        let home = home_trash().unwrap();
        assert_eq!(trash_of("/Users/me/a.txt"), Some(home));
        assert_eq!(
            trash_of("/Volumes/USB/photos/a.jpg"),
            Some(PathBuf::from(format!("/Volumes/USB/.Trashes/{}", uid())))
        );
    }

    #[test]
    fn restores_an_item_by_name() {
        let base = std::env::temp_dir().join(format!("lustro-trash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let item = base.join("in-trash.txt");
        std::fs::write(&item, b"x").unwrap();
        let original = base.join("back.txt");
        let original = original.to_str().unwrap();
        restore(&item, original).unwrap();
        assert!(Path::new(original).exists());
        std::fs::write(&item, b"y").unwrap();
        assert!(restore(&item, original).is_err(), "never overwrites");
        std::fs::remove_dir_all(&base).unwrap();
    }
}

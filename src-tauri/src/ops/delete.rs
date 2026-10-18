//! Moves items to the Recycle Bin or deletes them permanently.
//! Every function checks the protected paths first.

use std::fs;
use std::io;
use std::os::windows::fs::MetadataExt;
use std::path::Path;

use super::protect::is_protected;

const ATTR_READONLY: u32 = 0x1;
const ATTR_DIRECTORY: u32 = 0x10;
const ATTR_REPARSE_POINT: u32 = 0x400;

#[derive(Debug, thiserror::Error)]
pub enum DeleteError {
    #[error("{0} is a protected system path. Lustro never deletes it.")]
    Protected(String),
    #[error("{0} no longer exists.")]
    Missing(String),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("The Recycle Bin refused the item: {0}")]
    Trash(String),
}

fn check(path: &Path) -> Result<fs::Metadata, DeleteError> {
    let s = path.to_string_lossy();
    if is_protected(&s) {
        return Err(DeleteError::Protected(s.into_owned()));
    }
    fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            DeleteError::Missing(s.into_owned())
        } else {
            DeleteError::Io(e)
        }
    })
}

/// Moves one file or folder to the Recycle Bin.
pub fn recycle(path: &Path) -> Result<(), DeleteError> {
    check(path)?;
    trash::delete(path).map_err(|e| DeleteError::Trash(e.to_string()))
}

/// Deletes one file or folder permanently. A junction or symbolic link is removed
/// as a link; Lustro never deletes the files that it points to.
pub fn delete_permanent(path: &Path) -> Result<(), DeleteError> {
    let meta = check(path)?;
    remove(path, &meta)?;
    Ok(())
}

fn remove(path: &Path, meta: &fs::Metadata) -> io::Result<()> {
    let attrs = meta.file_attributes();
    // Use the attribute, not `is_dir()`: Rust reports a junction as a symlink, not a folder.
    let is_dir = attrs & ATTR_DIRECTORY != 0;
    if attrs & ATTR_REPARSE_POINT != 0 {
        // A link: remove the link itself, never its target.
        return if is_dir {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        };
    }
    if is_dir {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let m = fs::symlink_metadata(entry.path())?;
            remove(&entry.path(), &m)?;
        }
        clear_readonly(path, attrs);
        fs::remove_dir(path)
    } else {
        clear_readonly(path, attrs);
        fs::remove_file(path)
    }
}

fn clear_readonly(path: &Path, attrs: u32) {
    if attrs & ATTR_READONLY != 0
        && let Ok(m) = fs::metadata(path)
    {
        let mut p = m.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        p.set_readonly(false);
        let _ = fs::set_permissions(path, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("lustro-del-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn deletes_a_folder_tree_with_read_only_files() {
        let root = temp("tree");
        let dir = root.join("victim");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/a.txt"), b"a").unwrap();
        let ro = dir.join("readonly.txt");
        fs::write(&ro, b"b").unwrap();
        let mut p = fs::metadata(&ro).unwrap().permissions();
        p.set_readonly(true);
        fs::set_permissions(&ro, p).unwrap();

        delete_permanent(&dir).unwrap();
        assert!(!dir.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn junction_delete_never_touches_the_target() {
        let root = temp("junc");
        let target = temp("junc-target");
        fs::write(target.join("keep.txt"), b"keep me").unwrap();
        let holder = root.join("holder");
        fs::create_dir_all(&holder).unwrap();
        let link = holder.join("link");
        let ok = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                link.to_str().unwrap(),
                target.to_str().unwrap(),
            ])
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok);

        // Deleting the folder that holds the junction must keep the target's files.
        delete_permanent(&holder).unwrap();
        assert!(!holder.exists());
        assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep me");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn refuses_protected_and_missing_paths() {
        assert!(matches!(
            delete_permanent(Path::new("C:\\Windows")),
            Err(DeleteError::Protected(_))
        ));
        assert!(matches!(
            recycle(Path::new("C:\\pagefile.sys")),
            Err(DeleteError::Protected(_))
        ));
        assert!(matches!(
            delete_permanent(Path::new("C:\\")),
            Err(DeleteError::Protected(_))
        ));
        let missing = std::env::temp_dir().join("lustro-surely-missing-9f8e7d");
        assert!(matches!(
            delete_permanent(&missing),
            Err(DeleteError::Missing(_))
        ));
    }

    #[test]
    fn recycles_and_restores_a_file() {
        let root = temp("bin");
        let f = root.join(format!("lustro-recycle-test-{}.txt", std::process::id()));
        fs::write(&f, b"recycle me").unwrap();
        recycle(&f).unwrap();
        assert!(!f.exists());
        let items = trash::os_limited::list().unwrap();
        let item = items
            .into_iter()
            .find(|i| i.original_path() == f)
            .expect("item in the Recycle Bin");
        trash::os_limited::restore_all([item]).unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"recycle me");
        fs::remove_dir_all(root).unwrap();
    }
}

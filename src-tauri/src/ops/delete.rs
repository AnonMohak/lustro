//! Moves items to the Recycle Bin or deletes them permanently.
//! Every function checks the protected paths first.

use std::fs;
use std::io;
use std::path::Path;

use super::protect::{is_app_folder, is_protected};
use crate::system::{ATTR_DIRECTORY, ATTR_READONLY, ATTR_REPARSE_POINT, file_attributes};

#[derive(Debug, thiserror::Error)]
pub enum DeleteError {
    #[error("{0} is a protected system path. Lustro never deletes it.")]
    Protected(String),
    #[error(
        "{0} is an app folder in Program Files. To delete it, run Lustro as admin and turn on Settings > Allow deleting app folders."
    )]
    AppFolder(String),
    #[error("{0} no longer exists.")]
    Missing(String),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("The Recycle Bin refused the item: {0}")]
    Trash(String),
}

/// `allow_app`: the user confirmed the warning for app folders, runs as admin and turned on
/// the setting. Only then an app folder in Program Files passes.
fn check(path: &Path, allow_app: bool) -> Result<fs::Metadata, DeleteError> {
    let s = path.to_string_lossy();
    if is_protected(&s) {
        if !is_app_folder(&s) {
            return Err(DeleteError::Protected(s.into_owned()));
        }
        if !allow_app {
            return Err(DeleteError::AppFolder(s.into_owned()));
        }
    }
    fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            DeleteError::Missing(s.into_owned())
        } else {
            DeleteError::Io(e)
        }
    })
}

/// Moves one file or folder to the Recycle Bin. `allow_app` lets an app folder pass (see `check`).
pub fn recycle(path: &Path, allow_app: bool) -> Result<(), DeleteError> {
    check(path, allow_app)?;
    trash::delete(path).map_err(|e| DeleteError::Trash(e.to_string()))
}

/// Deletes one file or folder permanently. A junction or symbolic link is removed
/// as a link; Lustro never deletes the files that it points to.
/// `allow_app` lets an app folder pass (see `check`).
pub fn delete_permanent(path: &Path, allow_app: bool) -> Result<(), DeleteError> {
    let meta = check(path, allow_app)?;
    remove(path, &meta)?;
    Ok(())
}

fn remove(path: &Path, meta: &fs::Metadata) -> io::Result<()> {
    let attrs = file_attributes(meta);
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

        delete_permanent(&dir, false).unwrap();
        assert!(!dir.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
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
        delete_permanent(&holder, false).unwrap();
        assert!(!holder.exists());
        assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep me");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn refuses_protected_and_missing_paths() {
        assert!(matches!(
            delete_permanent(Path::new("C:\\Windows"), false),
            Err(DeleteError::Protected(_))
        ));
        assert!(matches!(
            recycle(Path::new("C:\\pagefile.sys"), false),
            Err(DeleteError::Protected(_))
        ));
        // App folders need the explicit allow, and Windows folders never pass.
        assert!(matches!(
            delete_permanent(Path::new("C:\\Program Files\\NoSuchGame"), false),
            Err(DeleteError::AppFolder(_))
        ));
        assert!(matches!(
            delete_permanent(Path::new("C:\\Program Files\\NoSuchGame"), true),
            Err(DeleteError::Missing(_))
        ));
        assert!(matches!(
            delete_permanent(Path::new("C:\\Program Files\\WindowsApps"), true),
            Err(DeleteError::Protected(_))
        ));
        assert!(matches!(
            delete_permanent(Path::new("C:\\Windows\\System32"), true),
            Err(DeleteError::Protected(_))
        ));
        assert!(matches!(
            delete_permanent(Path::new("C:\\"), false),
            Err(DeleteError::Protected(_))
        ));
        let missing = std::env::temp_dir().join("lustro-surely-missing-9f8e7d");
        assert!(matches!(
            delete_permanent(&missing, false),
            Err(DeleteError::Missing(_))
        ));
    }

    /// Needs a desktop session with a Recycle Bin. GitHub's Windows runners run as a
    /// service session where the shell Recycle Bin does not list the item, so CI skips it.
    #[cfg(windows)]
    #[test]
    fn recycles_and_restores_a_file() {
        if std::env::var("CI").is_ok() {
            eprintln!("skipped on CI: the runner session has no usable Recycle Bin");
            return;
        }
        // The long form of the path: TEMP can use a short 8.3 name (RUNNER~1),
        // but the Recycle Bin reports the long name.
        let root = temp("bin");
        let long = fs::canonicalize(&root).unwrap();
        let root = std::path::PathBuf::from(long.to_string_lossy().trim_start_matches("\\\\?\\"));
        let f = root.join(format!("lustro-recycle-test-{}.txt", std::process::id()));
        fs::write(&f, b"recycle me").unwrap();
        recycle(&f, false).unwrap();
        assert!(!f.exists());
        let items = trash::os_limited::list().unwrap();
        let count = items.len();
        let item = items
            .into_iter()
            .find(|i| i.original_path() == f)
            .unwrap_or_else(|| {
                panic!(
                    "{} is not among the {count} items in the Recycle Bin",
                    f.display()
                )
            });
        trash::os_limited::restore_all([item]).unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"recycle me");
        fs::remove_dir_all(root).unwrap();
    }
}

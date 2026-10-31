//! "Move to another drive": copies a folder to another place, checks the copy, and leaves
//! a link in the old place (a junction on Windows, a symbolic link elsewhere), so that apps
//! that use the old path keep working.

use std::fs;
use std::io;
use std::path::Path;

use crate::system::{ATTR_DIRECTORY, ATTR_REPARSE_POINT, file_attributes};

/// Files and bytes of a folder tree.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Totals {
    pub files: u64,
    pub bytes: u64,
}

/// The totals of `dir`. Links inside are refused: a copy would follow them.
fn measure(dir: &Path) -> Result<Totals, String> {
    let mut t = Totals::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            let meta = fs::symlink_metadata(e.path()).map_err(|e| e.to_string())?;
            let attrs = file_attributes(&meta);
            if attrs & ATTR_REPARSE_POINT != 0 {
                return Err(format!(
                    "{} is a link. Lustro does not move folders with links inside.",
                    e.path().display()
                ));
            }
            if attrs & ATTR_DIRECTORY != 0 {
                stack.push(e.path());
            } else {
                t.files += 1;
                t.bytes += meta.len();
            }
        }
    }
    Ok(t)
}

/// Copies `src` to `dst` (which must not exist), with the change times of the files.
fn copy_tree(src: &Path, dst: &Path, done: &mut u64, progress: &dyn Fn(u64)) -> io::Result<()> {
    fs::create_dir(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let meta = fs::symlink_metadata(e.path())?;
        let to = dst.join(e.file_name());
        if meta.is_dir() {
            copy_tree(&e.path(), &to, done, progress)?;
        } else {
            fs::copy(e.path(), &to)?;
            if let Ok(m) = meta.modified() {
                let _ = fs::File::options().write(true).open(&to)?.set_modified(m);
            }
            *done += meta.len();
            progress(*done);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn make_link(link: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    // A junction needs no admin rights. cmd.exe would expand %NAME% in the paths.
    let (l, t) = (link.to_string_lossy(), target.to_string_lossy());
    if l.contains('%') || t.contains('%') {
        return Err(io::Error::other("Lustro cannot link a path with % in it."));
    }
    let mut cmd = std::process::Command::new("cmd");
    cmd.raw_arg(format!("/C mklink /J \"{l}\" \"{t}\""));
    let out = crate::system::no_window(&mut cmd).output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

#[cfg(unix)]
fn make_link(link: &Path, target: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// Moves the folder `src` into `dest_parent` and leaves a link at `src`. On an error before
/// the link exists, everything goes back as it was. Returns the moved totals, and a note
/// when the old copy could not be removed after the move.
pub fn relocate(
    src: &Path,
    dest_parent: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<(Totals, Option<String>), String> {
    let name = src.file_name().ok_or("This folder has no name.")?;
    let dst = dest_parent.join(name);
    if fs::symlink_metadata(&dst).is_ok() {
        return Err(format!("{} already exists.", dst.display()));
    }
    if dst.starts_with(src) {
        return Err("The new place is inside the folder itself.".into());
    }
    let before = measure(src)?;
    let mut done = 0;
    if let Err(e) = copy_tree(src, &dst, &mut done, &|d| progress(d, before.bytes)) {
        let _ = fs::remove_dir_all(&dst);
        return Err(format!("The copy failed: {e}"));
    }
    if measure(&dst).ok() != Some(before) {
        let _ = fs::remove_dir_all(&dst);
        return Err(
            "The copy is not complete. Lustro removed it; the folder stays where it was.".into(),
        );
    }
    let mut old_name = name.to_os_string();
    old_name.push(".lustro-old");
    let old = src.with_file_name(old_name);
    if let Err(e) = fs::rename(src, &old) {
        let _ = fs::remove_dir_all(&dst);
        return Err(format!(
            "A program uses the folder ({e}). Close it and try again."
        ));
    }
    if let Err(e) = make_link(src, &dst) {
        let _ = fs::rename(&old, src);
        let _ = fs::remove_dir_all(&dst);
        return Err(format!("Lustro could not make the link: {e}"));
    }
    let note = fs::remove_dir_all(&old).err().map(|e| {
        format!(
            "The old copy is still at {} ({e}). Delete it when it is free.",
            old.display()
        )
    });
    Ok((before, note))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("lustro-move-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn moves_a_folder_and_leaves_a_link() {
        let (from, to) = (temp("from"), temp("to"));
        let game = from.join("My Game");
        fs::create_dir_all(game.join("data")).unwrap();
        fs::write(game.join("game.bin"), vec![7u8; 5000]).unwrap();
        fs::write(game.join("data/level1.pak"), vec![1u8; 3000]).unwrap();

        let (t, note) = relocate(&game, &to, &|_, _| {}).unwrap();
        assert_eq!(
            t,
            Totals {
                files: 2,
                bytes: 8000
            }
        );
        assert_eq!(note, None);
        // The old path still works, through the link.
        assert_eq!(fs::read(game.join("data/level1.pak")).unwrap().len(), 3000);
        assert!(to.join("My Game/game.bin").is_file());
        let attrs = file_attributes(&fs::symlink_metadata(&game).unwrap());
        assert!(
            attrs & ATTR_REPARSE_POINT != 0,
            "the old path is a link now"
        );
        assert!(!from.join("My Game.lustro-old").exists());

        // A second move to the same place is refused, and nothing changes.
        let other = from.join("Other");
        fs::create_dir_all(&other).unwrap();
        fs::create_dir_all(to.join("Other")).unwrap();
        assert!(relocate(&other, &to, &|_, _| {}).is_err());
        assert!(other.is_dir());

        fs::remove_dir_all(&from).unwrap();
        fs::remove_dir_all(&to).unwrap();
    }
}

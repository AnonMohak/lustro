//! "Link instead of delete": a copy becomes a link to the kept file, so the path stays but
//! the data is stored once. Windows and Linux use hard links: the names share one file, so
//! a change through one name shows in all of them. macOS uses clones (APFS): the files stay
//! independent and share only the blocks that did not change.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// The BLAKE3 hash of a whole file.
fn hash(path: &Path) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(*h.finalize().as_bytes())
}

#[cfg(target_os = "macos")]
fn make_link(keep: &Path, tmp: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let src = CString::new(keep.as_os_str().as_bytes())?;
    let dst = CString::new(tmp.as_os_str().as_bytes())?;
    // SAFETY: both strings end with NUL and stay alive for the call.
    if unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
fn make_link(keep: &Path, tmp: &Path) -> std::io::Result<()> {
    std::fs::hard_link(keep, tmp)
}

/// Replaces `copy` with a link to `keep`. Both files must still have the same content.
/// The link is made under a temporary name next to `copy`, then renamed over it, so
/// `copy` is never missing; on an error it stays as it was.
pub fn link_copy(keep: &Path, copy: &Path) -> Result<(), String> {
    if crate::ops::protect::is_protected(&copy.to_string_lossy()) {
        return Err("This path is protected.".into());
    }
    match (hash(keep), hash(copy)) {
        (Some(a), Some(b)) if a == b => {}
        (Some(_), Some(_)) => {
            return Err("The files are no longer the same. Search again.".into());
        }
        _ => return Err("Lustro could not read the files.".into()),
    }
    let dir = copy.parent().ok_or("The copy has no folder.")?;
    let tmp = dir.join(format!(".lustro-link-{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    make_link(keep, &tmp).map_err(|e| {
        format!("This disk cannot link the files ({e}). Links need both files on one NTFS, ext4, APFS or similar disk.")
    })?;
    std::fs::rename(&tmp, copy).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

// macOS: the temp folder is protected, and clones need an APFS disk.
#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn replaces_a_copy_with_a_hard_link() {
        let dir = std::env::temp_dir().join(format!("lustro-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("b")).unwrap();
        let (keep, copy, other) = (dir.join("a.bin"), dir.join("b/a.bin"), dir.join("c.bin"));
        fs::write(&keep, b"same data").unwrap();
        fs::write(&copy, b"same data").unwrap();
        fs::write(&other, b"different").unwrap();

        link_copy(&keep, &copy).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"same data");
        // Hard links: a change through one name shows in the other.
        fs::write(&keep, b"new data!").unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"new data!");
        assert!(
            link_copy(&keep, &other).is_err(),
            "different content is refused"
        );
        assert_eq!(fs::read(&other).unwrap(), b"different");
        assert!(
            fs::read_dir(dir.join("b")).unwrap().count() == 1,
            "no temporary file is left"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

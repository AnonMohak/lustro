pub mod cache;
pub mod kind;
#[cfg(windows)]
pub mod mft;
pub mod model;
pub mod query;
#[cfg(windows)]
pub mod usn;
#[cfg(windows)]
pub mod walk;
#[cfg(not(windows))]
pub mod walk {
    //! The folder walk on Linux and macOS comes in a later phase.
    use super::model::Tree;
    use super::{Progress, ScanError};

    pub fn walk(_root: &str, _progress: &Progress) -> Result<Tree, ScanError> {
        Err(ScanError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Scans are not available on this system yet.",
        )))
    }
}

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use model::{ScanMethod, Tree};

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("The scan was cancelled.")]
    Cancelled,
    #[error("The fast scan needs admin rights.")]
    NeedsAdmin,
    #[error("This drive does not use NTFS.")]
    NotNtfs,
    #[error("Windows refused access to {0}.")]
    AccessDenied(String),
    #[error("The file table looks damaged ({0}).")]
    Corrupt(&'static str),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Counters that the scan updates and the UI reads.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    /// Bytes found (walk) or MFT bytes read (fast scan).
    pub bytes: AtomicU64,
    pub denied: AtomicU64,
    cancel: AtomicBool,
    current: Mutex<String>,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn set_current(&self, path: &str) {
        if let Ok(mut c) = self.current.try_lock() {
            c.clear();
            c.push_str(path);
        }
    }

    pub fn current(&self) -> String {
        self.current.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

/// True when `root` is a whole drive, like `C:\` or `C:`. Other platforms have no drive letters.
#[cfg(windows)]
pub fn is_drive_root(root: &str) -> Option<char> {
    let b = root.as_bytes();
    let ok = (b.len() == 2 || (b.len() == 3 && b[2] == b'\\'))
        && b[0].is_ascii_alphabetic()
        && b[1] == b':';
    ok.then(|| (b[0] as char).to_ascii_uppercase())
}

/// Scans a drive or a folder. Whole NTFS drives use the MFT when `allow_mft` is true
/// and the process has admin rights; everything else uses the folder walk.
pub fn scan(root: &str, allow_mft: bool, progress: &Progress) -> Result<Tree, ScanError> {
    #[cfg(windows)]
    if allow_mft && let Some(letter) = is_drive_root(root) {
        match mft::scan_volume(letter, progress) {
            Ok(tree) => return Ok(tree),
            Err(ScanError::Cancelled) => return Err(ScanError::Cancelled),
            Err(e) => {
                log::warn!("Fast scan of {letter}: failed ({e}); using the folder walk");
                progress.files.store(0, Ordering::Relaxed);
                progress.dirs.store(0, Ordering::Relaxed);
                progress.bytes.store(0, Ordering::Relaxed);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = allow_mft;
    walk::walk(root, progress)
}

#[cfg(not(windows))]
pub fn is_drive_root(_root: &str) -> Option<char> {
    None
}

pub fn method_name(m: ScanMethod) -> &'static str {
    match m {
        ScanMethod::Mft => "mft",
        ScanMethod::Walk => "walk",
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::is_drive_root;

    #[test]
    fn drive_roots() {
        assert_eq!(is_drive_root("C:\\"), Some('C'));
        assert_eq!(is_drive_root("d:"), Some('D'));
        assert_eq!(is_drive_root("C:\\Users"), None);
        assert_eq!(is_drive_root("\\\\server\\share"), None);
    }
}

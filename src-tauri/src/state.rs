use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::scan::Progress;
use crate::scan::model::Tree;

/// Shared state of the backend.
#[derive(Default)]
pub struct AppState {
    /// Finished scans by root path (for example `C:\`).
    pub trees: RwLock<HashMap<String, Arc<Tree>>>,
    /// The running scan, if any. Lustro runs one scan at a time.
    pub active: Mutex<Option<(String, Arc<Progress>)>>,
    /// Folder from `--scan <path>`, taken once by the UI.
    pub startup_scan: Mutex<Option<String>>,
}

impl AppState {
    pub fn tree(&self, root: &str) -> Option<Arc<Tree>> {
        self.trees.read().ok()?.get(&normalize_root(root)).cloned()
    }
}

/// `c:` and `C:\` become `C:\`. Folders get one trailing backslash.
pub fn normalize_root(root: &str) -> String {
    if let Some(letter) = crate::scan::is_drive_root(root) {
        return format!("{letter}:\\");
    }
    format!(
        "{}\\",
        root.trim_end_matches(['\\', '/']).replace('/', "\\")
    )
}

#[cfg(test)]
mod tests {
    use super::normalize_root;

    #[test]
    fn roots_are_normalized() {
        assert_eq!(normalize_root("c:"), "C:\\");
        assert_eq!(normalize_root("C:\\"), "C:\\");
        assert_eq!(normalize_root("D:/Photos/"), "D:\\Photos\\");
        assert_eq!(normalize_root("D:\\Photos"), "D:\\Photos\\");
    }
}

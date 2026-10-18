//! The delete log, kept as a JSON file in the app data folder.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Recycle,
    Permanent,
    Cleaner,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    /// Unix time in milliseconds.
    pub time: i64,
    pub action: Action,
    pub path: String,
    pub size: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub restorable: bool,
    #[serde(default)]
    pub restored: bool,
}

pub struct History {
    file: PathBuf,
    pub entries: Vec<Entry>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl History {
    /// Loads the log. A missing or damaged file gives an empty log.
    pub fn load(dir: &Path) -> Self {
        let file = dir.join("history.json");
        let entries = fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Entry>>(&b).ok())
            .unwrap_or_default();
        History { file, entries }
    }

    pub fn add(
        &mut self,
        action: Action,
        path: &str,
        size: u64,
        result: Result<(), String>,
    ) -> u64 {
        let id = self.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        let ok = result.is_ok();
        self.entries.push(Entry {
            id,
            time: now_ms(),
            action,
            path: path.to_string(),
            size,
            ok,
            error: result.err(),
            restorable: ok && action == Action::Recycle,
            restored: false,
        });
        if self.entries.len() > MAX_ENTRIES {
            let extra = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..extra);
        }
        id
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.entries)?)?;
        fs::rename(&tmp, &self.file)
    }

    /// Newest first.
    pub fn newest_first(&self) -> Vec<Entry> {
        let mut v = self.entries.clone();
        v.reverse();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_save_load() {
        let dir = std::env::temp_dir().join(format!("lustro-hist-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut h = History::load(&dir);
        assert!(h.entries.is_empty());
        let a = h.add(Action::Recycle, "C:\\x\\a.iso", 100, Ok(()));
        let b = h.add(Action::Blocked, "C:\\Windows", 5, Err("protected".into()));
        assert_eq!((a, b), (1, 2));
        h.save().unwrap();

        let back = History::load(&dir);
        assert_eq!(back.entries.len(), 2);
        assert!(back.entries[0].restorable);
        assert!(!back.entries[1].ok && !back.entries[1].restorable);
        assert_eq!(back.newest_first()[0].path, "C:\\Windows");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_file_gives_empty_log() {
        let dir = std::env::temp_dir().join(format!("lustro-hist-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("history.json"), b"{not json").unwrap();
        assert!(History::load(&dir).entries.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}

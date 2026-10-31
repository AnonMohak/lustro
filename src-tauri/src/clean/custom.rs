//! Your own clean rules: in a parent folder, find folders with given names (for example
//! `node_modules`) in projects that nobody changed for some days. The rules live in
//! `custom-rules.json` in the app data folder. Lustro moves the matches to the Recycle Bin.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::dev::RuleItem;
use crate::ops::protect::is_protected;
use crate::system::{ATTR_DIRECTORY, ATTR_REPARSE_POINT, file_attributes};

const FILE: &str = "custom-rules.json";
/// The walk stops this deep below the parent folder.
const MAX_DEPTH: usize = 32;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Schedule {
    /// Only when the user starts it.
    #[default]
    Off,
    /// Once after each start of Lustro.
    Start,
    Daily,
    Weekly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// Folders by name, in projects with no change (for example `node_modules`).
    #[default]
    Folders,
    /// Files by name pattern, with no change themselves (for example `*.iso` in Downloads).
    Files,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRule {
    pub id: String,
    pub name: String,
    /// The parent folder.
    pub root: String,
    /// What `targets` match: folder names, or file name patterns.
    #[serde(default)]
    pub kind: Target,
    /// Folder names (`node_modules`), or file patterns with `*` and `?` (`*.iso`).
    pub targets: Vec<String>,
    /// Folders: their project counts as untouched after this many days with no change.
    /// Files: the file itself had no change for this many days.
    pub age_days: u32,
    #[serde(default)]
    pub schedule: Schedule,
    /// Unix time in milliseconds of the last run, or 0.
    #[serde(default)]
    pub last_run: i64,
}

pub fn load(dir: &Path) -> Vec<CustomRule> {
    fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, rules: &[CustomRule]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let file = dir.join(FILE);
    let tmp = file.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(rules)?)?;
    fs::rename(&tmp, &file)
}

/// Why a rule cannot be saved, or None when it is valid.
pub fn invalid(r: &CustomRule) -> Option<String> {
    if r.name.trim().is_empty() {
        return Some("Give the rule a name.".into());
    }
    let root = Path::new(&r.root);
    if !root.is_absolute() || !root.is_dir() {
        return Some(format!("The folder {} does not exist.", r.root));
    }
    if r.targets.is_empty() {
        return Some(match r.kind {
            Target::Folders => "Type at least one folder name.".into(),
            Target::Files => "Type at least one file pattern.".into(),
        });
    }
    match r.kind {
        Target::Folders => {
            if let Some(bad) = r.targets.iter().find(|t| {
                let t = t.trim();
                t.is_empty() || t == "." || t == ".." || t.contains(['/', '\\', '*', '?'])
            }) {
                return Some(format!(
                    "\"{bad}\" is not a folder name. Type names only, for example node_modules."
                ));
            }
        }
        Target::Files => {
            // A pattern that matches every file ("*") is too easy to get wrong.
            if let Some(bad) = r.targets.iter().find(|t| {
                let t = t.trim();
                t.is_empty()
                    || t.contains(['/', '\\'])
                    || t.chars().all(|c| c == '*' || c == '?' || c == '.')
                    || glob::Pattern::new(t).is_err()
            }) {
                return Some(format!(
                    "\"{bad}\" is not a file pattern. Type patterns like *.iso or Screen Recording*.mov."
                ));
            }
        }
    }
    if r.age_days == 0 {
        return Some("The number of days must be 1 or more.".into());
    }
    None
}

fn is_target(name: &str, targets: &[String]) -> bool {
    targets.iter().any(|t| crate::paths::same(name, t.trim()))
}

/// A real folder: not a file, a symbolic link or a junction.
fn is_plain_dir(meta: &fs::Metadata) -> bool {
    let attrs = file_attributes(meta);
    attrs & ATTR_DIRECTORY != 0 && attrs & ATTR_REPARSE_POINT == 0
}

/// The folders under `root` (at any depth) whose name is a target, with the project
/// folder that holds each one. The walk does not go into a match or follow links.
pub fn find_matches(root: &Path, targets: &[String]) -> Vec<(PathBuf, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(meta) = fs::symlink_metadata(e.path()) else {
                continue;
            };
            if !is_plain_dir(&meta) {
                continue;
            }
            if is_target(&e.file_name().to_string_lossy(), targets) {
                out.push((e.path(), dir.clone()));
            } else if depth < MAX_DEPTH {
                stack.push((e.path(), depth + 1));
            }
        }
    }
    out.sort();
    out
}

fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The newest change (Unix ms) of the files and folders in `project`, without the target
/// folders. The project folder itself does not count: it changes when a target folder
/// comes or goes. 0 when the project has nothing else.
pub fn newest_change(project: &Path, targets: &[String]) -> i64 {
    let mut newest = 0;
    let mut stack = vec![(project.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(meta) = fs::symlink_metadata(e.path()) else {
                continue;
            };
            if is_plain_dir(&meta) {
                if is_target(&e.file_name().to_string_lossy(), targets) {
                    continue;
                }
                if depth < MAX_DEPTH {
                    stack.push((e.path(), depth + 1));
                }
            }
            newest = newest.max(mtime_ms(&meta));
        }
    }
    newest
}

/// The matches of a rule. A match is selected when its project had no change for
/// `age_days`. Protected paths never show.
pub fn scan(rule: &CustomRule, now_ms: i64) -> Vec<RuleItem> {
    if rule.kind == Target::Files {
        return scan_files(rule, now_ms);
    }
    let matches = find_matches(Path::new(&rule.root), &rule.targets);
    matches
        .par_iter()
        .filter(|(target, _)| !is_protected(&target.to_string_lossy()))
        .map(|(target, project)| {
            let changed = newest_change(project, &rule.targets);
            let age_days = (changed > 0).then(|| ((now_ms - changed).max(0) / DAY_MS) as u32);
            let m = super::measure_dir(target, None);
            RuleItem {
                label: None,
                path: target.to_string_lossy().into_owned(),
                bytes: m.bytes,
                files: m.files,
                age_days,
                selected: age_days.is_some_and(|a| a >= rule.age_days),
            }
        })
        .collect()
}

/// The files under the rule's folder (at any depth, not through links) whose name matches a
/// pattern. A file is selected when it had no change for `age_days`.
fn scan_files(rule: &CustomRule, now_ms: i64) -> Vec<RuleItem> {
    let patterns: Vec<glob::Pattern> = rule
        .targets
        .iter()
        .filter_map(|t| glob::Pattern::new(t.trim()).ok())
        .collect();
    let opts = glob::MatchOptions {
        case_sensitive: !crate::paths::IGNORE_CASE,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };
    let mut out = Vec::new();
    let mut stack = vec![(PathBuf::from(&rule.root), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(meta) = fs::symlink_metadata(e.path()) else {
                continue;
            };
            let attrs = file_attributes(&meta);
            if attrs & ATTR_REPARSE_POINT != 0 {
                continue;
            }
            if attrs & ATTR_DIRECTORY != 0 {
                if depth < MAX_DEPTH {
                    stack.push((e.path(), depth + 1));
                }
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path().to_string_lossy().into_owned();
            if !patterns.iter().any(|p| p.matches_with(&name, opts)) || is_protected(&path) {
                continue;
            }
            let changed = mtime_ms(&meta);
            let age_days = (changed > 0).then(|| ((now_ms - changed).max(0) / DAY_MS) as u32);
            out.push(RuleItem {
                label: None,
                path,
                bytes: meta.len(),
                files: 1,
                age_days,
                selected: age_days.is_some_and(|a| a >= rule.age_days),
            });
        }
    }
    out.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    out
}

/// True when an automatic run of the rule is due. `ran_this_start` says that the rule
/// ran after this start of Lustro.
pub fn is_due(rule: &CustomRule, now_ms: i64, ran_this_start: bool) -> bool {
    let since = now_ms - rule.last_run;
    match rule.schedule {
        Schedule::Off => false,
        Schedule::Start => !ran_this_start,
        Schedule::Daily => since >= DAY_MS,
        Schedule::Weekly => since >= 7 * DAY_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("lustro-custom-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[cfg(not(target_os = "macos"))]
    fn set_age(path: &Path, days: u64) {
        use std::time::{Duration, SystemTime};
        let t = SystemTime::now() - Duration::from_secs(days * 86_400);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    fn rule(root: &Path) -> CustomRule {
        CustomRule {
            id: "r1".into(),
            name: "Old node_modules".into(),
            root: root.to_string_lossy().into_owned(),
            kind: Target::Folders,
            targets: vec!["node_modules".into()],
            age_days: 30,
            schedule: Schedule::Off,
            last_run: 0,
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn file_rules_match_patterns_and_age() {
        let root = temp("files");
        fs::create_dir_all(root.join("sub")).unwrap();
        for (name, days) in [
            ("old.ISO", 40),
            ("new.iso", 1),
            ("sub/setup.exe", 20),
            ("keep.txt", 90),
        ] {
            let f = root.join(name);
            fs::write(&f, vec![0u8; 10]).unwrap();
            set_age(&f, days);
        }
        let r = CustomRule {
            kind: Target::Files,
            targets: vec!["*.iso".into(), "*.exe".into()],
            age_days: 14,
            ..rule(&root)
        };
        assert_eq!(invalid(&r), None);
        let items = scan(&r, crate::ops::history::now_ms());
        let picked: Vec<&str> = items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.path.rsplit(crate::paths::SEP).next().unwrap())
            .collect();
        let mut names: Vec<&str> = items
            .iter()
            .map(|i| i.path.rsplit(crate::paths::SEP).next().unwrap())
            .collect();
        names.sort();
        if crate::paths::IGNORE_CASE {
            assert_eq!(names, ["new.iso", "old.ISO", "setup.exe"]);
            assert!(picked.contains(&"old.ISO"));
        } else {
            assert_eq!(names, ["new.iso", "setup.exe"]);
        }
        assert!(picked.contains(&"setup.exe"));
        assert!(!picked.contains(&"new.iso"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finds_matches_at_any_depth_but_not_inside_a_match() {
        let root = temp("find");
        fs::create_dir_all(root.join("a/node_modules/x/node_modules")).unwrap();
        fs::create_dir_all(root.join("group/b/node_modules")).unwrap();
        fs::create_dir_all(root.join("c/src")).unwrap();
        let found = find_matches(&root, &["node_modules".into()]);
        assert_eq!(
            found,
            vec![
                (root.join("a/node_modules"), root.join("a")),
                (root.join("group/b/node_modules"), root.join("group/b")),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    // macOS: the temp folder is under the protected /var/folders, so the scan skips it.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn selects_only_untouched_projects() {
        let root = temp("age");
        for (p, days) in [("old", 40), ("new", 2)] {
            fs::create_dir_all(root.join(p).join("node_modules")).unwrap();
            let f = root.join(p).join("package.json");
            fs::write(&f, b"{}").unwrap();
            set_age(&f, days);
            // A new file in node_modules does not make a project "changed".
            fs::write(root.join(p).join("node_modules/lib.js"), vec![0u8; 100]).unwrap();
        }
        let items = scan(&rule(&root), crate::ops::history::now_ms());
        assert_eq!(items.len(), 2);
        let old = items.iter().find(|i| i.path.contains("old")).unwrap();
        let new = items.iter().find(|i| i.path.contains("new")).unwrap();
        assert!(old.selected && old.age_days.unwrap() >= 39);
        assert!(!new.selected && new.age_days.unwrap() <= 3);
        assert_eq!(old.bytes, 100);
        fs::remove_dir_all(root).unwrap();
    }

    // macOS: the temp folder is under the protected /var/folders, so the scan skips it.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn a_project_with_only_the_match_is_not_selected() {
        let root = temp("empty");
        fs::create_dir_all(root.join("p/node_modules")).unwrap();
        let items = scan(&rule(&root), crate::ops::history::now_ms());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].age_days, None);
        assert!(!items[0].selected);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checks_rules_before_save() {
        let root = temp("valid");
        let ok = rule(&root);
        assert_eq!(invalid(&ok), None);
        for bad in [
            CustomRule {
                name: " ".into(),
                ..ok.clone()
            },
            CustomRule {
                root: "relative/path".into(),
                ..ok.clone()
            },
            CustomRule {
                targets: vec![],
                ..ok.clone()
            },
            CustomRule {
                targets: vec!["..".into()],
                ..ok.clone()
            },
            CustomRule {
                targets: vec!["a/b".into()],
                ..ok.clone()
            },
            CustomRule {
                targets: vec!["*.log".into()],
                ..ok.clone()
            },
            CustomRule {
                age_days: 0,
                ..ok.clone()
            },
            CustomRule {
                kind: Target::Files,
                targets: vec!["*".into()],
                ..ok.clone()
            },
            CustomRule {
                kind: Target::Files,
                targets: vec!["*.*".into()],
                ..ok.clone()
            },
            CustomRule {
                kind: Target::Files,
                targets: vec!["sub/*.iso".into()],
                ..ok.clone()
            },
        ] {
            assert!(invalid(&bad).is_some(), "{bad:?}");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn schedules() {
        let now = 100 * DAY_MS;
        let mut r = rule(Path::new("/"));
        assert!(!is_due(&r, now, false));
        r.schedule = Schedule::Start;
        assert!(is_due(&r, now, false));
        assert!(!is_due(&r, now, true));
        r.schedule = Schedule::Daily;
        r.last_run = now - DAY_MS + 1;
        assert!(!is_due(&r, now, false));
        r.last_run = now - DAY_MS;
        assert!(is_due(&r, now, false));
        r.schedule = Schedule::Weekly;
        assert!(!is_due(&r, now, false));
        r.last_run = 0;
        assert!(is_due(&r, now, false));
    }

    #[test]
    fn saves_and_loads() {
        let dir = temp("store");
        assert!(load(&dir).is_empty());
        let rules = vec![rule(&dir)];
        save(&dir, &rules).unwrap();
        assert_eq!(load(&dir), rules);
        fs::remove_dir_all(dir).unwrap();
    }
}

//! The junk cleaner: rules from `rules.toml`, sizes of what they match, and the clean itself.

pub mod dev;
pub mod system;

use std::sync::Arc;

use crate::scan::model::Tree;
pub use dev::RuleItem;

use std::collections::HashSet;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ops::protect::cleaner_may_delete;

const ATTR_READONLY: u32 = 0x1;
const ATTR_DIRECTORY: u32 = 0x10;
const ATTR_REPARSE_POINT: u32 = 0x400;

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    pub group: String,
    pub name: String,
    pub desc: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub safe: bool,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub warn: bool,
    #[serde(default)]
    pub min_age_h: u64,
    #[serde(default)]
    pub process: Vec<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub special: Option<String>,
}

#[derive(Deserialize)]
struct RuleFile {
    rule: Vec<Rule>,
}

/// The built-in rules.
pub fn rules() -> Vec<Rule> {
    toml::from_str::<RuleFile>(include_str!("rules.toml"))
        .expect("rules.toml is valid")
        .rule
}

/// Expands `%VAR%` and turns the pattern into a glob. Values of variables are escaped,
/// so a `[` in a user name cannot change the pattern. Unknown variables give `None`.
pub fn expand(pattern: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        let value = env(&after[..end])?;
        out.push_str(&glob::Pattern::escape(&value));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// All existing paths that a rule matches.
pub fn matches(rule: &Rule) -> Vec<PathBuf> {
    let opts = glob::MatchOptions {
        case_sensitive: false,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for p in &rule.paths {
        let Some(pattern) = expand(p, &env_var) else {
            continue;
        };
        let Ok(paths) = glob::glob_with(&pattern, opts) else {
            continue;
        };
        for path in paths.flatten() {
            if seen.insert(path.to_string_lossy().to_lowercase()) {
                out.push(path);
            }
        }
    }
    out
}

/// Size and file count of what a clean would delete.
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Measure {
    pub bytes: u64,
    pub files: u64,
}

fn old_enough(meta: &fs::Metadata, min_age: Option<Duration>) -> bool {
    let Some(age) = min_age else { return true };
    let changed = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    SystemTime::now()
        .duration_since(changed)
        .map(|d| d >= age)
        .unwrap_or(false)
}

fn min_age(rule: &Rule) -> Option<Duration> {
    (rule.min_age_h > 0).then(|| Duration::from_secs(rule.min_age_h * 3600))
}

/// Adds up one folder. `DirEntry::metadata` comes from the directory listing on Windows,
/// so it costs no extra file open (unlike `symlink_metadata`). Subfolders run in parallel.
pub(crate) fn measure_dir(path: &Path, age: Option<Duration>) -> Measure {
    let mut m = Measure::default();
    let mut dirs = Vec::new();
    for e in fs::read_dir(path).into_iter().flatten().flatten() {
        let Ok(meta) = e.metadata() else { continue };
        let attrs = meta.file_attributes();
        if attrs & ATTR_REPARSE_POINT != 0 {
            continue; // links point elsewhere; the clean removes only the link
        }
        if attrs & ATTR_DIRECTORY != 0 {
            dirs.push(e.path());
        } else if old_enough(&meta, age) {
            m.bytes += meta.len();
            m.files += 1;
        }
    }
    let sub = dirs
        .par_iter()
        .map(|d| measure_dir(d, age))
        .reduce(Measure::default, |a, b| Measure {
            bytes: a.bytes + b.bytes,
            files: a.files + b.files,
        });
    Measure {
        bytes: m.bytes + sub.bytes,
        files: m.files + sub.files,
    }
}

/// Metadata for each matched path, read from one listing of each parent folder
/// (a glob like `%TEMP%\*` can match tens of thousands of entries).
fn with_meta(paths: Vec<PathBuf>) -> Vec<(PathBuf, fs::Metadata)> {
    let mut by_parent: std::collections::HashMap<PathBuf, HashSet<std::ffi::OsString>> =
        Default::default();
    for p in &paths {
        if let (Some(parent), Some(name)) = (p.parent(), p.file_name()) {
            by_parent
                .entry(parent.to_path_buf())
                .or_default()
                .insert(name.to_ascii_lowercase());
        }
    }
    let mut out = Vec::with_capacity(paths.len());
    for (parent, names) in by_parent {
        for e in fs::read_dir(&parent).into_iter().flatten().flatten() {
            if names.contains(&e.file_name().to_ascii_lowercase())
                && let Ok(meta) = e.metadata()
            {
                out.push((e.path(), meta));
            }
        }
    }
    out
}

pub fn measure(rule: &Rule) -> Measure {
    if rule.special.as_deref() == Some("recycle-bin") {
        return system::recycle_bin_size();
    }
    let age = min_age(rule);
    with_meta(matches(rule))
        .par_iter()
        .map(|(p, meta)| {
            let attrs = meta.file_attributes();
            if attrs & ATTR_REPARSE_POINT != 0 {
                Measure::default()
            } else if attrs & ATTR_DIRECTORY != 0 {
                measure_dir(p, age)
            } else if old_enough(meta, age) {
                Measure {
                    bytes: meta.len(),
                    files: 1,
                }
            } else {
                Measure::default()
            }
        })
        .reduce(Measure::default, |a, b| Measure {
            bytes: a.bytes + b.bytes,
            files: a.files + b.files,
        })
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanOutcome {
    pub freed_bytes: u64,
    pub deleted_files: u64,
    /// Files that Windows did not let Lustro delete (most are in use).
    pub skipped_files: u64,
}

fn clean_path(path: &Path, meta: &fs::Metadata, age: Option<Duration>, out: &mut CleanOutcome) {
    let attrs = meta.file_attributes();
    if !cleaner_may_delete(&path.to_string_lossy()) {
        out.skipped_files += 1;
        return;
    }
    if attrs & ATTR_REPARSE_POINT != 0 {
        let r = if attrs & ATTR_DIRECTORY != 0 {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        };
        if r.is_err() {
            out.skipped_files += 1;
        }
        return;
    }
    if attrs & ATTR_DIRECTORY != 0 {
        for e in fs::read_dir(path).into_iter().flatten().flatten() {
            if let Ok(m) = e.metadata() {
                clean_path(&e.path(), &m, age, out);
            }
        }
        // Remove the folder only when it is empty now; errors mean files stayed.
        let _ = fs::remove_dir(path);
        return;
    }
    if !old_enough(meta, age) {
        return;
    }
    if attrs & ATTR_READONLY != 0 {
        let mut perm = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        let _ = fs::set_permissions(path, perm);
    }
    match fs::remove_file(path) {
        Ok(()) => {
            out.freed_bytes += meta.len();
            out.deleted_files += 1;
        }
        Err(_) => out.skipped_files += 1,
    }
}

/// What the special rules need: the scans (for project folders) and the age setting.
pub struct Ctx {
    pub trees: Vec<Arc<Tree>>,
    pub dev_age_days: u32,
}

pub struct Scanned {
    pub measure: Measure,
    /// Folders the user picks one by one (project folders, disks, old IDE data).
    pub items: Vec<RuleItem>,
    /// Why the rule cannot run now, for example "Docker does not run".
    pub unavailable: Option<String>,
}

fn sum(items: &[RuleItem]) -> Measure {
    Measure {
        bytes: items.iter().map(|i| i.bytes).sum(),
        files: items.iter().map(|i| i.files).sum(),
    }
}

/// Measures a rule, including the special rules.
pub fn scan_rule(rule: &Rule, ctx: &Ctx) -> Scanned {
    let with_items = |items: Vec<RuleItem>, unavailable: Option<String>| Scanned {
        measure: sum(&items),
        items,
        unavailable,
    };
    match rule.special.as_deref() {
        Some("dev-projects") => with_items(
            dev::find_projects(&ctx.trees, ctx.dev_age_days),
            ctx.trees.is_empty().then(|| {
                "Scan a drive or folder first. Then Lustro finds the project folders in it.".into()
            }),
        ),
        Some("wsl-compact") => with_items(dev::wsl_disks(), None),
        Some("jetbrains-old") => with_items(dev::jetbrains_old(), None),
        Some(kind @ ("docker-images" | "docker-build" | "docker-containers")) => {
            match dev::docker_df() {
                Some(df) => {
                    let bytes = match kind {
                        "docker-images" => df.images,
                        "docker-build" => df.build_cache,
                        _ => df.containers,
                    };
                    Scanned {
                        measure: Measure { bytes, files: 0 },
                        items: Vec::new(),
                        unavailable: None,
                    }
                }
                None => Scanned {
                    measure: Measure::default(),
                    items: Vec::new(),
                    unavailable: Some("Docker is not installed or does not run.".into()),
                },
            }
        }
        _ => Scanned {
            measure: measure(rule),
            items: Vec::new(),
            unavailable: None,
        },
    }
}

/// Runs a rule. For rules with items, `picked` names the chosen paths; the paths are
/// matched against a fresh scan of the rule, so the UI cannot add other paths.
pub fn clean_rule(rule: &Rule, ctx: &Ctx, picked: Option<&[String]>) -> CleanOutcome {
    let mut out = CleanOutcome::default();
    match rule.special.as_deref() {
        Some(kind @ ("dev-projects" | "wsl-compact" | "jetbrains-old")) => {
            let scanned = scan_rule(rule, ctx);
            let chosen: Vec<RuleItem> = scanned
                .items
                .into_iter()
                .filter(|i| match picked {
                    Some(p) => p.iter().any(|x| x.eq_ignore_ascii_case(&i.path)),
                    None => i.selected,
                })
                .collect();
            if kind == "wsl-compact" {
                let paths: Vec<String> = chosen.iter().map(|i| i.path.clone()).collect();
                out.freed_bytes = dev::wsl_compact(&paths);
                return out;
            }
            for item in chosen {
                let p = Path::new(&item.path);
                if let Ok(m) = fs::symlink_metadata(p) {
                    clean_path(p, &m, None, &mut out);
                }
            }
            out
        }
        Some(kind @ ("docker-images" | "docker-build" | "docker-containers")) => {
            let before = scan_rule(rule, ctx).measure.bytes;
            if dev::docker_prune(kind) {
                out.freed_bytes = before;
            }
            out
        }
        _ => clean(rule),
    }
}

/// Runs one rule. The caller checks admin rights and running programs first.
pub fn clean(rule: &Rule) -> CleanOutcome {
    let mut out = CleanOutcome::default();
    match rule.special.as_deref() {
        Some("recycle-bin") => {
            let before = system::recycle_bin_size();
            if system::empty_recycle_bin() {
                out.freed_bytes = before.bytes;
                out.deleted_files = before.files;
            }
            return out;
        }
        Some("windows-old") => {
            for p in matches(rule) {
                system::take_ownership(&p);
                if let Ok(m) = fs::symlink_metadata(&p) {
                    clean_path(&p, &m, None, &mut out);
                }
            }
            return out;
        }
        _ => {}
    }
    let stopped = rule.service.as_deref().is_some_and(system::stop_service);
    for (p, m) in with_meta(matches(rule)) {
        clean_path(&p, &m, min_age(rule), &mut out);
    }
    if stopped && let Some(s) = rule.service.as_deref() {
        system::start_service(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("lustro-clean-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// Times the measure step of every rule: `cargo test --profile perf -- --ignored time_rules --nocapture`.
    #[test]
    #[ignore]
    fn time_rules() {
        for r in rules() {
            let t = std::time::Instant::now();
            let m = measure(&r);
            println!(
                "{:<24} {:>14} bytes {:>9} files {:?}",
                r.id,
                m.bytes,
                m.files,
                t.elapsed()
            );
        }
    }

    #[test]
    fn rules_file_is_valid() {
        let r = rules();
        assert!(r.len() >= 10);
        let ids: HashSet<&str> = r.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids.len(), r.len(), "rule ids are unique");
        for rule in &r {
            assert!(
                ["sys", "web", "dev", "docker", "ide"].contains(&rule.group.as_str()),
                "{}",
                rule.id
            );
            assert!(
                !rule.paths.is_empty() || rule.special.is_some(),
                "{} has nothing to clean",
                rule.id
            );
            assert!(
                !(rule.safe && rule.warn),
                "{} cannot be both safe and warn",
                rule.id
            );
        }
    }

    #[test]
    fn expands_and_escapes_variables() {
        let env = |k: &str| match k {
            "TEMP" => Some("C:\\Users\\a[1]\\Temp".to_string()),
            _ => None,
        };
        assert_eq!(
            expand("%TEMP%\\*", &env).unwrap(),
            "C:\\Users\\a[[]1[]]\\Temp\\*"
        );
        assert_eq!(expand("%NOPE%\\x", &env), None);
        assert_eq!(expand("C:\\plain\\*", &env).unwrap(), "C:\\plain\\*");
    }

    #[test]
    fn measures_and_cleans_matching_files_only() {
        let root = temp("rule");
        fs::create_dir_all(root.join("cache/sub")).unwrap();
        fs::write(root.join("cache/a.bin"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("cache/sub/b.bin"), vec![0u8; 500]).unwrap();
        fs::write(root.join("keep.txt"), b"keep").unwrap();
        let pattern = format!(
            "{}\\cache\\*",
            glob::Pattern::escape(&root.to_string_lossy())
        );
        let rule = Rule {
            id: "t".into(),
            group: "sys".into(),
            name: "t".into(),
            desc: String::new(),
            paths: vec![pattern],
            safe: true,
            admin: false,
            warn: false,
            min_age_h: 0,
            process: vec![],
            service: None,
            special: None,
        };
        let m = measure(&rule);
        assert_eq!((m.bytes, m.files), (1500, 2));
        let out = clean(&rule);
        assert_eq!((out.freed_bytes, out.deleted_files), (1500, 2));
        assert!(
            root.join("cache").exists(),
            "the matched parent folder stays"
        );
        assert!(!root.join("cache/sub").exists());
        assert!(root.join("keep.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn min_age_keeps_new_files() {
        let root = temp("age");
        fs::write(root.join("new.tmp"), b"new").unwrap();
        let pattern = format!("{}\\*", glob::Pattern::escape(&root.to_string_lossy()));
        let rule = Rule {
            id: "t".into(),
            group: "sys".into(),
            name: "t".into(),
            desc: String::new(),
            paths: vec![pattern],
            safe: true,
            admin: false,
            warn: false,
            min_age_h: 24,
            process: vec![],
            service: None,
            special: None,
        };
        assert_eq!(measure(&rule).files, 0);
        assert_eq!(clean(&rule).deleted_files, 0);
        assert!(root.join("new.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

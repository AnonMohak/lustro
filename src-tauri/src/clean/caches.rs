//! The "App caches" group: one item for each cache folder of an app. Apps make their caches
//! again, so the cleaner empties these folders and keeps the folders themselves.
//!
//! - macOS: `~/Library/Caches/<app>` and the caches of sandboxed apps in `~/Library/Containers`.
//! - Linux: `~/.cache/<app>` and the caches of Flatpak apps in `~/.var/app`.
//! - Windows: the temp and web cache folders of Store apps, and the `Cache`, `Code Cache`
//!   and `GPUCache` folders that Electron and Chromium apps keep in AppData.
//! - Android and iOS do not let an app clear the caches of other apps.
//!
//! Folders that another rule covers (browsers, Homebrew, pip, ...) do not show, so that
//! nothing counts twice.

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
use std::path::Path;
use std::path::PathBuf;

use rayon::prelude::*;

use super::dev::RuleItem;
use super::{Rule, matches, measure_dir};
use crate::paths::{SEP, key};

/// Cache folders that Lustro leaves alone: its own saved scans, active downloads, iCloud,
/// and IDE caches whose loss costs a long new index (the "JetBrains data of old versions"
/// rule cleans the old ones).
const SKIP: &[&str] = &[
    "com.anonmohak.lustro",
    "com.apple.nsurlsessiond",
    "com.apple.bird",
    "cloudkit",
    "jetbrains",
];

/// Cache subfolders of Electron and Chromium apps (Windows).
#[cfg(windows)]
const ELECTRON_CACHES: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "ShaderCache",
];

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn subdirs(dir: &Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// (label, folder) of each app cache folder of this platform.
#[cfg(target_os = "macos")]
fn candidates() -> Vec<(String, PathBuf)> {
    let Some(home) = env_path("HOME") else {
        return Vec::new();
    };
    let lib = home.join("Library");
    let mut out: Vec<(String, PathBuf)> = subdirs(&lib.join("Caches"))
        .map(|p| (name_of(&p), p))
        .collect();
    for c in subdirs(&lib.join("Containers")) {
        let caches = c.join("Data/Library/Caches");
        if caches.is_dir() {
            out.push((name_of(&c), caches));
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn candidates() -> Vec<(String, PathBuf)> {
    let Some(home) = env_path("HOME") else {
        return Vec::new();
    };
    let cache = env_path("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
    let mut out: Vec<(String, PathBuf)> = subdirs(&cache).map(|p| (name_of(&p), p)).collect();
    for app in subdirs(&home.join(".var/app")) {
        let c = app.join("cache");
        if c.is_dir() {
            out.push((name_of(&app), c));
        }
    }
    out
}

#[cfg(windows)]
fn candidates() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Some(local) = env_path("LOCALAPPDATA") {
        // Store apps: the name before `_` is the app, for example `SpotifyAB.SpotifyMusic`.
        // Only TempState (Windows may delete it at any time) and the web cache. Never
        // LocalCache: packaged desktop apps keep all their AppData there.
        for pkg in subdirs(&local.join("Packages")) {
            let name = name_of(&pkg);
            let label = name.split('_').next().unwrap_or(&name).to_string();
            for sub in ["TempState", "AC\\INetCache"] {
                let c = pkg.join(sub);
                if c.is_dir() {
                    out.push((label.clone(), c));
                }
            }
        }
    }
    for base in ["APPDATA", "LOCALAPPDATA"].into_iter().filter_map(env_path) {
        for app in subdirs(&base) {
            // The app folder, and one folder deeper (`<vendor>\<app>`).
            let mut dirs = vec![(name_of(&app), app.clone())];
            dirs.extend(subdirs(&app).map(|d| (format!("{} {}", name_of(&app), name_of(&d)), d)));
            for (label, dir) in dirs {
                for sub in ELECTRON_CACHES {
                    let c = dir.join(sub);
                    if c.is_dir() {
                        out.push((format!("{label} ({sub})"), c));
                    }
                }
            }
        }
    }
    out
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn candidates() -> Vec<(String, PathBuf)> {
    Vec::new()
}

/// `a` is `b`, or one holds the other.
fn overlaps(a: &str, b: &str) -> bool {
    let inside = |x: &str, y: &str| x.strip_prefix(y).is_some_and(|r| r.starts_with(SEP));
    a == b || inside(a, b) || inside(b, a)
}

/// The cache folders that no other rule covers, biggest first. Each one is selected.
pub fn app_caches(others: &[Rule]) -> Vec<RuleItem> {
    let covered: Vec<String> = others
        .par_iter()
        .filter(|r| r.special.is_none())
        .flat_map_iter(matches)
        .map(|p| key(&p.to_string_lossy()))
        .collect();
    let mut items: Vec<RuleItem> = candidates()
        .into_par_iter()
        .filter(|(label, p)| {
            let label = label.to_lowercase();
            let k = key(&p.to_string_lossy());
            !SKIP.iter().any(|s| label.contains(s)) && !covered.iter().any(|c| overlaps(&k, c))
        })
        .filter_map(|(label, p)| {
            let m = measure_dir(&p, None);
            (m.bytes > 0).then(|| RuleItem {
                path: p.to_string_lossy().into_owned(),
                label: Some(label),
                bytes: m.bytes,
                files: m.files,
                age_days: None,
                selected: true,
            })
        })
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_needs_a_whole_path_part() {
        let s = SEP;
        let a = format!("{s}home{s}me{s}.cache{s}google-chrome");
        assert!(overlaps(&a, &a));
        assert!(overlaps(&a, &format!("{a}{s}Default{s}Cache")));
        assert!(overlaps(&format!("{a}{s}Default"), &a));
        assert!(!overlaps(&a, &format!("{a}-beta")));
    }

    /// Prints the app caches of this computer: `cargo test -- --ignored print_app_caches --nocapture`.
    #[test]
    #[ignore]
    fn print_app_caches() {
        for i in app_caches(&super::super::rules()) {
            println!(
                "{:>14} {:<40} {}",
                i.bytes,
                i.label.unwrap_or_default(),
                i.path
            );
        }
    }

    #[test]
    fn lists_caches_without_panic() {
        let items = app_caches(&super::super::rules());
        assert!(items.iter().all(|i| i.bytes > 0 && i.label.is_some()));
        assert!(items.windows(2).all(|w| w[0].bytes >= w[1].bytes));
    }
}

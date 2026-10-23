//! Dev junk: project build folders found in the scans, Docker, WSL disks, old JetBrains data.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use serde::Serialize;

use crate::ops::protect::is_protected;
use crate::scan::model::{NONE, Tree};
use crate::system::no_window;

const DAY_100NS: u64 = 864_000_000_000;

/// One folder or file inside a rule that the user can pick on its own.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleItem {
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    /// Days since the project changed (dev projects only).
    pub age_days: Option<u32>,
    pub selected: bool,
}

/// Build folders and the file next to them that proves they belong to a project.
/// A marker that starts with '.' is a file extension.
const JUNK: &[(&str, &[&str])] = &[
    ("node_modules", &["package.json"]),
    ("target", &["cargo.toml"]),
    ("bin", &[".csproj", ".fsproj", ".vbproj"]),
    ("obj", &[".csproj", ".fsproj", ".vbproj"]),
    (
        ".venv",
        &["pyproject.toml", "requirements.txt", "setup.py", "pipfile"],
    ),
    (
        "venv",
        &["pyproject.toml", "requirements.txt", "setup.py", "pipfile"],
    ),
    (".next", &["package.json"]),
    (".nuxt", &["package.json"]),
    (".turbo", &["package.json"]),
    (".parcel-cache", &["package.json"]),
    (
        ".gradle",
        &[
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
        ],
    ),
];

/// Folders whose build folders belong to installed tools, not to your projects.
const SKIP_SEGMENTS: &[&str] = &[
    "appdata",
    "programdata",
    ".vscode",
    ".vscode-insiders",
    ".cursor",
    ".windsurf",
    "scoop",
    ".rustup",
    ".cargo",
    ".nvm",
    "nvm",
    "go",
    ".bun",
    ".deno",
    ".yarn",
    ".npm-global",
    ".pnpm-store",
    ".local",
    ".volta",
    ".fnm",
    #[cfg(unix)]
    ".cache",
    #[cfg(unix)]
    ".config",
    #[cfg(unix)]
    "snap",
    #[cfg(unix)]
    "flatpak",
];

fn junk_markers(name: &str) -> Option<&'static [&'static str]> {
    JUNK.iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, m)| *m)
}

fn has_marker(t: &Tree, project: u32, markers: &[&str]) -> bool {
    t.children(project).any(|c| {
        if t.node(c).is_dir() {
            return false;
        }
        let name = t.name(c).to_ascii_lowercase();
        markers.iter().any(|m| {
            if m.starts_with('.') {
                name.ends_with(m)
            } else {
                name == *m
            }
        })
    })
}

/// Newest change time in a project, without its build folders.
fn project_modified(t: &Tree, project: u32) -> u64 {
    let mut newest = 0;
    let mut stack = vec![project];
    while let Some(cur) = stack.pop() {
        for c in t.children(cur) {
            let n = t.node(c);
            if n.is_dir() {
                if junk_markers(t.name(c)).is_none() && t.name(c) != ".git" {
                    stack.push(c);
                }
            } else {
                newest = newest.max(n.modified);
            }
        }
    }
    newest
}

fn now_filetime() -> u64 {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (unix + 11_644_473_600) * 10_000_000
}

/// Build folders in all scans. Folders of projects with no change for `age_days` are selected.
pub fn find_projects(trees: &[Arc<Tree>], age_days: u32) -> Vec<RuleItem> {
    let now = now_filetime();
    let mut out = Vec::new();
    for t in trees {
        for (i, n) in t.nodes.iter().enumerate() {
            let id = i as u32;
            if !n.is_dir() || n.parent == NONE || n.size == 0 {
                continue;
            }
            let Some(markers) = junk_markers(t.name(id)) else {
                continue;
            };
            // Not inside another build folder (node_modules inside node_modules).
            let mut inside_junk = false;
            let mut cur = n.parent;
            while cur != NONE && cur != Tree::ROOT {
                if junk_markers(t.name(cur)).is_some() {
                    inside_junk = true;
                    break;
                }
                cur = t.node(cur).parent;
            }
            if inside_junk || !has_marker(t, n.parent, markers) {
                continue;
            }
            let path = t.path(id);
            let lower = path.to_lowercase();
            if is_protected(&path)
                || lower
                    .split(crate::paths::SEP)
                    .any(|s| SKIP_SEGMENTS.contains(&s))
            {
                continue;
            }
            let modified = project_modified(t, n.parent);
            let age = if modified == 0 {
                None
            } else {
                Some((now.saturating_sub(modified) / DAY_100NS) as u32)
            };
            out.push(RuleItem {
                path,
                bytes: n.size,
                files: n.items as u64,
                age_days: age,
                selected: age.is_some_and(|a| a >= age_days),
            });
        }
    }
    // A folder can be in two scans (a drive and a folder on it). Keep it once.
    let mut seen = std::collections::HashSet::new();
    out.retain(|i| seen.insert(i.path.to_lowercase()));
    out.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    out
}

// ---------- Docker ----------

/// Reclaimable bytes per type from `docker system df`, or None when Docker does not run.
#[derive(Debug, Default, Clone, Copy)]
pub struct DockerDf {
    pub images: u64,
    pub containers: u64,
    pub build_cache: u64,
}

/// Parses Docker sizes like "9.7GB", "512.5MB", "1.2kB", "0B" (decimal units).
pub fn parse_docker_size(s: &str) -> u64 {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f64 = num.trim().parse().unwrap_or(0.0);
    let mul = match unit.trim().to_ascii_lowercase().as_str() {
        "b" | "" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => 0.0,
    };
    (n * mul) as u64
}

fn docker(args: &[&str]) -> Option<String> {
    let out = no_window(Command::new("docker").args(args)).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn docker_df() -> Option<DockerDf> {
    let text = docker(&["system", "df", "--format", "{{json .}}"])?;
    let mut df = DockerDf::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let reclaim = v["Reclaimable"].as_str().unwrap_or("0B");
        let bytes = parse_docker_size(reclaim.split(' ').next().unwrap_or("0B"));
        match v["Type"].as_str() {
            Some("Images") => df.images = bytes,
            Some("Containers") => df.containers = bytes,
            Some("Build Cache") => df.build_cache = bytes,
            _ => {}
        }
    }
    Some(df)
}

/// Runs a fixed prune command. Returns true on success.
pub fn docker_prune(kind: &str) -> bool {
    let args: &[&str] = match kind {
        "docker-images" => &["image", "prune", "-a", "-f"],
        "docker-build" => &["builder", "prune", "-f"],
        "docker-containers" => &["container", "prune", "-f"],
        _ => return false,
    };
    docker(args).is_some()
}

// ---------- WSL disks ----------

const WSL_DISKS: &[&str] = &[
    r"%LOCALAPPDATA%\Packages\*\LocalState\ext4.vhdx",
    r"%LOCALAPPDATA%\Docker\wsl\disk\*.vhdx",
    r"%LOCALAPPDATA%\Docker\wsl\data\ext4.vhdx",
    r"%LOCALAPPDATA%\Docker\wsl\main\ext4.vhdx",
];

pub fn wsl_disks() -> Vec<RuleItem> {
    let rule = super::Rule {
        id: String::new(),
        group: String::new(),
        name: String::new(),
        desc: String::new(),
        paths: WSL_DISKS.iter().map(|s| s.to_string()).collect(),
        safe: false,
        admin: true,
        warn: true,
        min_age_h: 0,
        process: vec![],
        service: None,
        special: None,
    };
    super::matches(&rule)
        .into_iter()
        .filter_map(|p| {
            let len = std::fs::metadata(&p).ok()?.len();
            Some(RuleItem {
                path: p.to_string_lossy().into_owned(),
                bytes: len,
                files: 1,
                age_days: None,
                selected: true,
            })
        })
        .collect()
}

/// Shuts down WSL and compacts the disks with diskpart. Returns the bytes saved.
pub fn wsl_compact(disks: &[String]) -> u64 {
    if disks.is_empty() {
        return 0;
    }
    let _ = no_window(Command::new("wsl.exe").arg("--shutdown")).output();
    let mut saved = 0;
    for d in disks {
        let before = std::fs::metadata(d).map(|m| m.len()).unwrap_or(0);
        let script =
            std::env::temp_dir().join(format!("lustro-compact-{}.txt", std::process::id()));
        let body = format!(
            "select vdisk file=\"{d}\"\r\nattach vdisk readonly\r\ncompact vdisk\r\ndetach vdisk\r\n"
        );
        if std::fs::write(&script, body).is_ok() {
            let _ = no_window(Command::new("diskpart.exe").arg("/s").arg(&script)).output();
            let _ = std::fs::remove_file(&script);
        }
        let after = std::fs::metadata(d).map(|m| m.len()).unwrap_or(before);
        saved += before.saturating_sub(after);
    }
    saved
}

// ---------- JetBrains ----------

/// Splits "RustRover2024.3" into ("rustrover", [2024, 3]).
pub fn jetbrains_version(name: &str) -> Option<(String, Vec<u32>)> {
    let split = name.find(|c: char| c.is_ascii_digit())?;
    let (product, version) = name.split_at(split);
    if product.is_empty() {
        return None;
    }
    let parts: Vec<u32> = version
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    (parts.len() >= 2 && parts[0] >= 2015).then(|| (product.to_ascii_lowercase(), parts))
}

/// The folder with the caches and logs of each JetBrains IDE version.
#[cfg(windows)]
fn jetbrains_base() -> Option<std::path::PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(Path::new(&local).join("JetBrains"))
}

#[cfg(not(windows))]
fn jetbrains_base() -> Option<std::path::PathBuf> {
    let cache = std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|c| !c.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| Path::new(&h).join(".cache"))
        })?;
    Some(cache.join("JetBrains"))
}

/// Data folders of JetBrains IDE versions that a newer version of the same IDE replaced.
pub fn jetbrains_old() -> Vec<RuleItem> {
    let Some(base) = jetbrains_base() else {
        return Vec::new();
    };
    let mut found: Vec<(String, Vec<u32>, std::path::PathBuf)> = Vec::new();
    for e in std::fs::read_dir(&base).into_iter().flatten().flatten() {
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false)
            && let Some((product, v)) = jetbrains_version(&e.file_name().to_string_lossy())
        {
            found.push((product, v, e.path()));
        }
    }
    let mut out = Vec::new();
    for (product, v, path) in &found {
        let newest = found
            .iter()
            .filter(|(p, _, _)| p == product)
            .map(|(_, v, _)| v)
            .max();
        if newest.is_some_and(|n| n > v) {
            let mut m = super::Measure::default();
            let d = super::measure_dir(path, None);
            m.bytes += d.bytes;
            m.files += d.files;
            out.push(RuleItem {
                path: path.to_string_lossy().into_owned(),
                bytes: m.bytes,
                files: m.files,
                age_days: None,
                selected: true,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(windows, target_os = "linux"))]
    use crate::scan::model::{ScanMethod, TreeBuilder, flags};

    #[test]
    fn docker_sizes() {
        assert_eq!(parse_docker_size("9.7GB"), 9_700_000_000);
        assert_eq!(parse_docker_size("512.5MB"), 512_500_000);
        assert_eq!(parse_docker_size("1.2kB"), 1200);
        assert_eq!(parse_docker_size("0B"), 0);
        assert_eq!(parse_docker_size("junk"), 0);
    }

    #[test]
    fn jetbrains_versions() {
        assert_eq!(
            jetbrains_version("RustRover2024.3"),
            Some(("rustrover".into(), vec![2024, 3]))
        );
        assert_eq!(
            jetbrains_version("IntelliJIdea2025.1.2"),
            Some(("intellijidea".into(), vec![2025, 1, 2]))
        );
        assert_eq!(jetbrains_version("Toolbox"), None);
        assert_eq!(jetbrains_version("2024.1"), None);
        assert!(
            jetbrains_version("WebStorm2024.2").unwrap().1
                < jetbrains_version("WebStorm2025.1").unwrap().1
        );
    }

    /// C:\Users\me\code\{app\{package.json, src\a.js, node_modules\{x\...}}, lib\{Cargo.toml, target\b}},
    /// plus C:\Users\me\AppData\...\node_modules (must be skipped).
    #[cfg(any(windows, target_os = "linux"))]
    fn tree(app_modified: u64) -> Arc<Tree> {
        let mut b = TreeBuilder::new("C:", 32);
        let d = |b: &mut TreeBuilder, name: &str, parent: u32| {
            let id = b.push(name, flags::DIR, 0, 0, 0);
            b.link(id, parent);
            id
        };
        let f = |b: &mut TreeBuilder, name: &str, parent: u32, size: u64, m: u64| {
            let id = b.push(name, 0, size, size, m);
            b.link(id, parent);
            id
        };
        let users = d(&mut b, "Users", 0);
        let me = d(&mut b, "me", users);
        let code = d(&mut b, "code", me);
        let app = d(&mut b, "app", code);
        f(&mut b, "package.json", app, 10, app_modified);
        let src = d(&mut b, "src", app);
        f(&mut b, "a.js", src, 10, app_modified);
        let nm = d(&mut b, "node_modules", app);
        let x = d(&mut b, "x", nm);
        let inner = d(&mut b, "node_modules", x); // nested: skip
        f(&mut b, "dep.js", inner, 500, 1);
        f(&mut b, "x.js", x, 1000, 1);
        let lib = d(&mut b, "lib", code);
        f(&mut b, "Cargo.toml", lib, 10, 1);
        let target = d(&mut b, "target", lib);
        f(&mut b, "b.rlib", target, 3000, 1);
        let notproj = d(&mut b, "photos", code);
        let t2 = d(&mut b, "target", notproj); // no Cargo.toml: skip
        f(&mut b, "pic.jpg", t2, 7000, 1);
        let appdata = d(&mut b, "AppData", me);
        let tool = d(&mut b, "tool", appdata);
        f(&mut b, "package.json", tool, 10, 1);
        let nm2 = d(&mut b, "node_modules", tool); // inside AppData: skip
        f(&mut b, "t.js", nm2, 9000, 1);
        Arc::new(b.finish(crate::paths::os("C:\\"), ScanMethod::Walk, 1, 0))
    }

    // macOS: needs its protected paths (until then every macOS path is protected).
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn finds_build_folders_of_projects_only() {
        let items = find_projects(&[tree(now_filetime())], 30);
        let paths: Vec<&str> = items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                crate::paths::os("C:\\Users\\me\\code\\lib\\target"),
                crate::paths::os("C:\\Users\\me\\code\\app\\node_modules")
            ]
        );
        assert_eq!(items[1].bytes, 1500);
        // The app changed today, so its node_modules is not selected; lib is old.
        assert!(!items[1].selected);
        assert!(items[0].selected);
        assert_eq!(items[1].age_days, Some(0));
    }

    // macOS: needs its protected paths (until then every macOS path is protected).
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn a_folder_in_two_scans_is_listed_once() {
        let t = tree(now_filetime());
        assert_eq!(find_projects(&[t.clone(), t], 30).len(), 2);
    }
}

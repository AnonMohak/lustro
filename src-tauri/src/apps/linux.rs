//! Installed apps on Linux: deb packages with a menu entry, snaps and Flatpak apps.
//! Lustro only lists them. Uninstall tells the user the package manager command.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::App;

fn mtime_ms(path: &Path) -> i64 {
    std::fs::symlink_metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `Name=` of the `[Desktop Entry]` group, or None for an entry that the menu does not show.
fn desktop_name(text: &str) -> Option<String> {
    let mut in_entry = false;
    let mut name = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        match line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
            Some(("Name", v)) if name.is_none() => name = Some(v.to_string()),
            Some(("NoDisplay" | "Hidden", "true")) => return None,
            Some(("Type", t)) if t != "Application" => return None,
            _ => {}
        }
    }
    name.filter(|n| !n.is_empty())
}

#[derive(Debug, Default, PartialEq)]
struct Package {
    version: String,
    maintainer: String,
    /// Bytes.
    size: Option<u64>,
}

/// Installed packages from `/var/lib/dpkg/status`.
fn parse_dpkg_status(text: &str) -> HashMap<String, Package> {
    let mut out = HashMap::new();
    for para in text.split("\n\n") {
        let mut name = None;
        let mut installed = false;
        let mut p = Package::default();
        for line in para.lines() {
            let Some((k, v)) = line.split_once(": ") else {
                continue;
            };
            match k {
                "Package" => name = Some(v.to_string()),
                "Status" => installed = v.ends_with(" installed"),
                "Version" => p.version = v.to_string(),
                "Maintainer" => p.maintainer = v.split(" <").next().unwrap_or(v).to_string(),
                "Installed-Size" => p.size = v.trim().parse::<u64>().ok().map(|kib| kib * 1024),
                _ => {}
            }
        }
        if let (Some(n), true) = (name, installed) {
            out.insert(n, p);
        }
    }
    out
}

/// Packages with a menu entry: their file list holds a `.desktop` file.
fn deb_apps() -> Vec<App> {
    let Ok(status) = std::fs::read_to_string("/var/lib/dpkg/status") else {
        return Vec::new();
    };
    let packages = parse_dpkg_status(&status);
    let mut out = Vec::new();
    for e in std::fs::read_dir("/var/lib/dpkg/info")
        .into_iter()
        .flatten()
        .flatten()
    {
        let file = e.file_name().to_string_lossy().into_owned();
        let Some(stem) = file.strip_suffix(".list") else {
            continue;
        };
        let pkg = stem.split(':').next().unwrap_or(stem); // "name:amd64"
        let Some(info) = packages.get(pkg) else {
            continue;
        };
        let Ok(list) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let name = list
            .lines()
            .filter(|l| l.starts_with("/usr/share/applications/") && l.ends_with(".desktop"))
            .find_map(|l| desktop_name(&std::fs::read_to_string(l).ok()?));
        let Some(name) = name else { continue };
        out.push(App {
            id: format!("deb:{pkg}"),
            name,
            publisher: info.maintainer.clone(),
            version: info.version.clone(),
            source: "desktop",
            size_from: if info.size.is_some() {
                "registry"
            } else {
                "none"
            },
            size: info.size,
            installed: mtime_ms(&e.path()),
            last_used: 0,
            location: String::new(),
            uninstall: format!("sudo apt remove {pkg}"),
            exe_name: None,
        });
    }
    out
}

/// `key: value` of a snap.yaml line at the top level.
fn yaml_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .map(|v| v.trim().trim_matches(|c| c == '\'' || c == '"'))
}

/// Snaps that are apps (not bases, the kernel or snapd itself).
fn snap_apps() -> Vec<App> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("/snap").into_iter().flatten().flatten() {
        let snap = e.file_name().to_string_lossy().into_owned();
        let current = e.path().join("current");
        let Ok(meta) = std::fs::read_to_string(current.join("meta/snap.yaml")) else {
            continue;
        };
        let kind = yaml_value(&meta, "type").unwrap_or("app");
        if matches!(kind, "base" | "os" | "core" | "snapd" | "kernel" | "gadget") {
            continue;
        }
        let size: u64 = std::fs::read_dir("/var/lib/snapd/snaps")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|f| {
                let n = f.file_name().to_string_lossy().into_owned();
                n.strip_prefix(&snap)
                    .is_some_and(|r| r.starts_with('_') && r.ends_with(".snap"))
            })
            .filter_map(|f| f.metadata().ok().map(|m| m.len()))
            .sum();
        out.push(App {
            id: format!("snap:{snap}"),
            name: yaml_value(&meta, "title").unwrap_or(&snap).to_string(),
            publisher: String::new(),
            version: yaml_value(&meta, "version").unwrap_or_default().to_string(),
            source: "store",
            size_from: if size > 0 { "registry" } else { "none" },
            size: (size > 0).then_some(size),
            installed: mtime_ms(&current),
            last_used: 0,
            location: current.to_string_lossy().into_owned(),
            uninstall: format!("sudo snap remove {snap}"),
            exe_name: None,
        });
    }
    out
}

/// Flatpak apps of the system and of the user.
fn flatpak_apps() -> Vec<App> {
    let mut roots = vec![(PathBuf::from("/var/lib/flatpak/app"), "")];
    if let Ok(home) = std::env::var("HOME") {
        roots.push((Path::new(&home).join(".local/share/flatpak/app"), " --user"));
    }
    let mut out = Vec::new();
    for (root, flag) in roots {
        for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            let active = e.path().join("current/active");
            if !active.is_dir() {
                continue;
            }
            let desktop = active.join(format!("export/share/applications/{id}.desktop"));
            let name = std::fs::read_to_string(desktop)
                .ok()
                .and_then(|t| desktop_name(&t))
                .unwrap_or_else(|| id.clone());
            let size = crate::clean::measure_dir(&active, None).bytes;
            out.push(App {
                id: format!("flatpak:{id}"),
                name,
                publisher: String::new(),
                version: String::new(),
                source: "store",
                size_from: "folder",
                size: Some(size),
                installed: mtime_ms(&active),
                last_used: 0,
                location: active.to_string_lossy().into_owned(),
                uninstall: format!("flatpak uninstall{flag} {id}"),
                exe_name: None,
            });
        }
    }
    out
}

pub fn installed() -> Vec<App> {
    let mut apps = deb_apps();
    apps.extend(snap_apps());
    apps.extend(flatpak_apps());
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_desktop_entries() {
        let text = "[Desktop Entry]\nType=Application\nName=Files\nName[de]=Dateien\nExec=nautilus\n\n[Desktop Action new]\nName=New Window\n";
        assert_eq!(desktop_name(text), Some("Files".into()));
        assert_eq!(
            desktop_name("[Desktop Entry]\nName=X\nNoDisplay=true\n"),
            None
        );
        assert_eq!(desktop_name("[Desktop Entry]\nType=Link\nName=X\n"), None);
        assert_eq!(desktop_name("[Other]\nName=X\n"), None);
    }

    #[test]
    fn reads_dpkg_status() {
        let text = "Package: gimp\nStatus: install ok installed\nInstalled-Size: 20480\nMaintainer: Ubuntu Developers <ubuntu-devel@lists.ubuntu.com>\nVersion: 2.10.36-3\n\nPackage: gone\nStatus: deinstall ok config-files\nVersion: 1\n";
        let p = parse_dpkg_status(text);
        assert_eq!(p.len(), 1);
        assert_eq!(
            p["gimp"],
            Package {
                version: "2.10.36-3".into(),
                maintainer: "Ubuntu Developers".into(),
                size: Some(20480 * 1024),
            }
        );
    }

    #[test]
    fn reads_snap_yaml() {
        let yaml = "name: firefox\nversion: '131.0'\ntitle: Firefox\ntype: app\napps:\n  firefox:\n    command: firefox.launcher\n";
        assert_eq!(yaml_value(yaml, "version"), Some("131.0"));
        assert_eq!(yaml_value(yaml, "title"), Some("Firefox"));
        assert_eq!(yaml_value(yaml, "base"), None);
    }

    #[test]
    fn lists_apps_without_panic() {
        assert!(installed().iter().all(|a| !a.name.is_empty()));
    }
}

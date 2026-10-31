//! Installed apps on macOS: the `.app` bundles in `/Applications` and `~/Applications`.
//! Uninstall moves the bundle and its data in `~/Library` to the Trash.

use std::path::{Path, PathBuf};
use std::process::Command;

use rayon::prelude::*;

use super::App;

/// The folders that hold apps. Apps one folder deeper (`/Applications/Utilities`,
/// `/Applications/Adobe Photoshop/Adobe Photoshop.app`) count too.
fn app_folders() -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from("/Applications")];
    if let Ok(home) = std::env::var("HOME") {
        out.push(Path::new(&home).join("Applications"));
    }
    out
}

fn is_bundle(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

fn bundles() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for folder in app_folders() {
        for e in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let p = e.path();
            // A link (for example to a system app) is not an installed app.
            if e.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
                continue;
            }
            if is_bundle(&p) {
                out.push(p);
            } else if p.is_dir() {
                for inner in std::fs::read_dir(&p).into_iter().flatten().flatten() {
                    let ip = inner.path();
                    if is_bundle(&ip) && !inner.file_type().is_ok_and(|t| t.is_symlink()) {
                        out.push(ip);
                    }
                }
            }
        }
    }
    out
}

/// The values of `Contents/Info.plist` that Lustro shows.
#[derive(Debug, Default, PartialEq)]
struct Info {
    id: String,
    name: String,
    version: String,
}

fn read_info(bundle: &Path) -> Info {
    let Ok(plist::Value::Dictionary(d)) =
        plist::Value::from_file(bundle.join("Contents/Info.plist"))
    else {
        return Info::default();
    };
    let get = |k: &str| {
        d.get(k)
            .and_then(|v| v.as_string())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let mut name = get("CFBundleDisplayName");
    if name.is_empty() {
        name = get("CFBundleName");
    }
    Info {
        id: get("CFBundleIdentifier"),
        name,
        version: get("CFBundleShortVersionString"),
    }
}

fn ms(t: std::io::Result<std::time::SystemTime>) -> i64 {
    t.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A Spotlight date like `2026-05-01 10:20:30 +0000` in Unix milliseconds.
fn spotlight_date(s: &str) -> i64 {
    let mut parts = s.split_whitespace();
    let (Some(date), Some(time)) = (parts.next(), parts.next()) else {
        return 0;
    };
    let day = super::ymd_to_ms(&date.replace('-', ""));
    let secs: Vec<i64> = time.split(':').filter_map(|x| x.parse().ok()).collect();
    if day == 0 || secs.len() != 3 {
        return 0;
    }
    day + (secs[0] * 3600 + secs[1] * 60 + secs[2]) * 1000
}

/// The last start of each bundle from Spotlight (`kMDItemLastUsedDate`), in the order of
/// `bundles`. `mdls -raw` separates the values with NUL and writes `(null)` for no date.
fn last_used(bundles: &[PathBuf]) -> Vec<i64> {
    let out = Command::new("mdls")
        .args(["-name", "kMDItemLastUsedDate", "-raw"])
        .args(bundles)
        .output();
    let values: Vec<i64> = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .split('\0')
            .map(spotlight_date)
            .collect(),
        _ => Vec::new(),
    };
    if values.len() >= bundles.len() {
        values
    } else {
        vec![0; bundles.len()]
    }
}

pub fn installed() -> Vec<App> {
    let bundles = bundles();
    let used = last_used(&bundles);
    bundles
        .par_iter()
        .zip(used.par_iter())
        .map(|(b, &last)| {
            let info = read_info(b);
            let file_name = b
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let meta = std::fs::metadata(b);
            let store = b.join("Contents/_MASReceipt/receipt").exists();
            App {
                id: format!("app:{}", b.to_string_lossy()),
                name: if info.name.is_empty() {
                    file_name
                } else {
                    info.name
                },
                publisher: String::new(),
                version: info.version,
                source: if store { "store" } else { "desktop" },
                size: Some(crate::clean::measure_dir(b, None).bytes),
                size_from: "folder",
                installed: meta.as_ref().map(|m| ms(m.created())).unwrap_or(0),
                last_used: last,
                location: b.to_string_lossy().into_owned(),
                uninstall: b.to_string_lossy().into_owned(),
                exe_name: None,
            }
        })
        .collect()
}

/// The data of an app in `~/Library`, found by its bundle id. Folders that hold the data
/// of many apps are never in this list: each path ends with the id itself.
fn leftovers(home: &Path, id: &str) -> Vec<PathBuf> {
    // A bundle id is like `com.example.app`: never `..`, a path or an empty part.
    let valid = id.split('.').count() >= 2
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        });
    if !valid {
        return Vec::new();
    }
    let lib = home.join("Library");
    [
        lib.join("Application Support").join(id),
        lib.join("Caches").join(id),
        lib.join("Containers").join(id),
        lib.join("HTTPStorages").join(id),
        lib.join("Logs").join(id),
        lib.join("Preferences").join(format!("{id}.plist")),
        lib.join("Saved Application State")
            .join(format!("{id}.savedState")),
        lib.join("WebKit").join(id),
    ]
    .into_iter()
    .filter(|p| std::fs::symlink_metadata(p).is_ok())
    .collect()
}

/// Moves the bundle to the Trash, then its data in `~/Library`. macOS keeps system apps
/// on a read-only volume, so they fail here.
pub fn uninstall(app: &App) -> Result<(), String> {
    let bundle = Path::new(&app.uninstall);
    if !is_bundle(bundle) || !app_folders().iter().any(|f| bundle.starts_with(f)) {
        return Err(format!(
            "{} is not an app in an Applications folder.",
            app.name
        ));
    }
    let id = read_info(bundle).id;
    crate::ops::delete::to_trash(bundle)
        .map_err(|e| format!("macOS did not move {} to the Trash: {e}", app.name))?;
    if let Ok(home) = std::env::var("HOME") {
        for p in leftovers(Path::new(&home), &id) {
            let _ = crate::ops::delete::to_trash(&p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_spotlight_dates() {
        assert_eq!(spotlight_date("2000-01-01 00:00:10 +0000"), 946_684_810_000);
        assert_eq!(spotlight_date("(null)"), 0);
        assert_eq!(spotlight_date(""), 0);
    }

    #[test]
    fn leftovers_end_with_the_id() {
        let home = std::env::temp_dir().join(format!("lustro-apps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let caches = home.join("Library/Caches");
        std::fs::create_dir_all(caches.join("com.example.app")).unwrap();
        std::fs::create_dir_all(caches.join("com.other.app")).unwrap();
        assert_eq!(
            leftovers(&home, "com.example.app"),
            vec![caches.join("com.example.app")]
        );
        assert!(leftovers(&home, "").is_empty());
        for bad in [
            "..",
            ".",
            "com..app",
            ".hidden",
            "app",
            "a/b.c",
            "com.x/../..",
        ] {
            assert!(leftovers(&home, bad).is_empty(), "{bad}");
        }
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn lists_apps_without_panic() {
        assert!(installed().iter().all(|a| !a.name.is_empty()));
    }
}

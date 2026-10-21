//! Installed apps: desktop apps from the registry, Store apps from the package manager.

#[cfg(windows)]
use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
#[cfg(windows)]
use winreg::RegKey;
#[cfg(windows)]
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};

use crate::scan::model::Tree;

#[cfg(windows)]
const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    /// Stable id: "reg:<hive>:<key>" or "store:<package full name>".
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    /// "desktop" or "store".
    pub source: &'static str,
    /// Bytes, or None when unknown.
    pub size: Option<u64>,
    /// "registry", "folder" (from a scan) or "none".
    pub size_from: &'static str,
    /// Unix time in milliseconds, or 0 when unknown.
    pub installed: i64,
    /// Unix time in milliseconds of the last start (from Prefetch, needs admin), or 0.
    pub last_used: i64,
    pub location: String,
    /// What starts the uninstall. Only Windows lists apps until the other ports exist.
    #[cfg_attr(not(windows), allow(dead_code))]
    #[serde(skip)]
    pub uninstall: String,
    /// The program file name, for the last start time from Prefetch.
    #[cfg_attr(not(windows), allow(dead_code))]
    #[serde(skip)]
    pub exe_name: Option<String>,
}

#[cfg(windows)]
fn ymd_to_ms(s: &str) -> i64 {
    // InstallDate is "YYYYMMDD".
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return 0;
    }
    let (y, m, d) = (
        s[0..4].parse::<i64>().unwrap_or(0),
        s[4..6].parse::<i64>().unwrap_or(0),
        s[6..8].parse::<i64>().unwrap_or(0),
    );
    if !(1990..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return 0;
    }
    // Days from civil (Howard Hinnant's algorithm).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400_000
}

/// The program file name from a DisplayIcon value like `"C:\App\app.exe",0`.
#[cfg(windows)]
fn exe_from_icon(icon: &str) -> Option<String> {
    let s = icon.trim().trim_matches('"');
    let s = s.split(',').next()?.trim().trim_matches('"');
    let name = s.rsplit('\\').next()?;
    name.to_ascii_lowercase()
        .ends_with(".exe")
        .then(|| name.to_ascii_uppercase())
}

#[cfg(windows)]
fn read_hive(
    hive: &RegKey,
    hive_name: &str,
    flags: u32,
    out: &mut Vec<App>,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(root) = hive.open_subkey_with_flags(UNINSTALL, KEY_READ | flags) else {
        return;
    };
    for key in root.enum_keys().flatten() {
        let Ok(k) = root.open_subkey_with_flags(&key, KEY_READ | flags) else {
            continue;
        };
        let get = |n: &str| k.get_value::<String, _>(n).unwrap_or_default();
        let name = get("DisplayName");
        let uninstall = get("UninstallString");
        if name.is_empty() || uninstall.is_empty() {
            continue;
        }
        // Skip parts of other apps and Windows updates.
        if k.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1
            || !get("ParentKeyName").is_empty()
            || matches!(
                get("ReleaseType").as_str(),
                "Update" | "Hotfix" | "Security Update"
            )
        {
            continue;
        }
        let version = get("DisplayVersion");
        if !seen.insert(format!("{}|{}", name.to_lowercase(), version)) {
            continue; // the same app in the 32-bit and 64-bit views
        }
        let size = k
            .get_value::<u32, _>("EstimatedSize")
            .ok()
            .filter(|&kb| kb > 0)
            .map(|kb| kb as u64 * 1024);
        out.push(App {
            id: format!("reg:{hive_name}:{key}"),
            name,
            publisher: get("Publisher"),
            version,
            source: "desktop",
            size_from: if size.is_some() { "registry" } else { "none" },
            size,
            installed: ymd_to_ms(&get("InstallDate")),
            last_used: 0,
            location: get("InstallLocation")
                .trim_matches('"')
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_string(),
            uninstall,
            exe_name: exe_from_icon(&get("DisplayIcon")),
        });
    }
}

#[cfg(windows)]
fn desktop_apps() -> Vec<App> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    read_hive(&hklm, "HKLM64", KEY_WOW64_64KEY, &mut out, &mut seen);
    read_hive(&hklm, "HKLM32", KEY_WOW64_32KEY, &mut out, &mut seen);
    read_hive(&hkcu, "HKCU", 0, &mut out, &mut seen);
    out
}

#[cfg(windows)]
fn store_apps() -> Vec<App> {
    use windows::ApplicationModel::PackageSignatureKind;
    use windows::Management::Deployment::PackageManager;
    use windows::core::HSTRING;

    let mut out = Vec::new();
    let Ok(pm) = PackageManager::new() else {
        return out;
    };
    let Ok(packages) = pm.FindPackagesByUserSecurityId(&HSTRING::new()) else {
        return out;
    };
    for pkg in packages {
        let skip = pkg.IsFramework().unwrap_or(true)
            || pkg.IsResourcePackage().unwrap_or(true)
            || pkg.IsBundle().unwrap_or(false)
            || pkg
                .SignatureKind()
                .map(|k| k == PackageSignatureKind::System)
                .unwrap_or(true);
        if skip {
            continue;
        }
        let Ok(id) = pkg.Id() else { continue };
        let full = id.FullName().map(|s| s.to_string()).unwrap_or_default();
        let name = pkg.DisplayName().map(|s| s.to_string()).unwrap_or_default();
        if full.is_empty() || name.is_empty() || name.starts_with("ms-resource:") {
            continue;
        }
        let version = id
            .Version()
            .map(|v| format!("{}.{}.{}.{}", v.Major, v.Minor, v.Build, v.Revision))
            .unwrap_or_default();
        // DateTime is 100 ns ticks since 1601.
        let installed = pkg
            .InstalledDate()
            .map(|d| (d.UniversalTime - 116_444_736_000_000_000) / 10_000)
            .unwrap_or(0);
        out.push(App {
            id: format!("store:{full}"),
            name,
            publisher: pkg
                .PublisherDisplayName()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            version,
            source: "store",
            size: None,
            size_from: "none",
            installed,
            last_used: 0,
            location: pkg
                .InstalledPath()
                .map(|p| p.to_string())
                .unwrap_or_default(),
            uninstall: full,
            exe_name: None,
        });
    }
    out
}

/// Last start time per program from the Prefetch folder (readable only as admin).
#[cfg(windows)]
fn prefetch_times() -> HashMap<String, i64> {
    let mut out = HashMap::new();
    let dir = std::env::var("WINDIR")
        .map(|w| format!("{w}\\Prefetch"))
        .unwrap_or_default();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_ascii_uppercase();
        // "CODE.EXE-1A2B3C4D.pf"
        let Some(exe) = name.split('-').next().filter(|n| n.ends_with(".EXE")) else {
            continue;
        };
        let Ok(m) = e.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let ms = m
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let slot = out.entry(exe.to_string()).or_insert(0);
        *slot = (*slot).max(ms);
    }
    out
}

/// All installed apps, biggest first. Missing sizes come from the scans when they cover the install folder.
pub fn list(trees: &[Arc<Tree>]) -> Vec<App> {
    let mut apps = installed();
    for a in &mut apps {
        if a.size.is_none() && !a.location.is_empty() {
            for t in trees {
                if let Some(id) = t.find_path(&a.location) {
                    a.size = Some(t.node(id).size);
                    a.size_from = "folder";
                    break;
                }
            }
        }
    }
    apps.sort_by_key(|a| std::cmp::Reverse(a.size.unwrap_or(0)));
    apps
}

/// Desktop and Store apps, with the last start time from Prefetch.
#[cfg(windows)]
fn installed() -> Vec<App> {
    let mut apps = desktop_apps();
    apps.extend(store_apps());
    let prefetch = prefetch_times();
    for a in &mut apps {
        if let Some(exe) = &a.exe_name {
            a.last_used = prefetch.get(exe).copied().unwrap_or(0);
        }
    }
    apps
}

/// App lists on Linux and macOS come in a later phase.
#[cfg(not(windows))]
fn installed() -> Vec<App> {
    Vec::new()
}

/// Splits an UninstallString into a program and its arguments.
#[cfg(windows)]
pub fn split_command(cmd: &str) -> (String, String) {
    let c = cmd.trim();
    if let Some(rest) = c.strip_prefix('"')
        && let Some(end) = rest.find('"')
    {
        return (rest[..end].to_string(), rest[end + 1..].trim().to_string());
    }
    let lower = c.to_ascii_lowercase();
    if let Some(i) = lower.find(".exe") {
        let end = i + 4;
        return (c[..end].to_string(), c[end..].trim().to_string());
    }
    match c.split_once(' ') {
        Some((a, b)) => (a.to_string(), b.trim().to_string()),
        None => (c.to_string(), String::new()),
    }
}

/// Starts the uninstaller of an app. Desktop apps run their own uninstaller (Windows asks for
/// admin rights when it needs them); Store apps are removed through the package manager.
#[cfg(windows)]
pub fn uninstall(app: &App) -> Result<(), String> {
    if app.source == "store" {
        use windows::Management::Deployment::{PackageManager, RemovalOptions};
        use windows::core::HSTRING;
        let pm = PackageManager::new().map_err(|e| e.to_string())?;
        let op = pm
            .RemovePackageWithOptionsAsync(&HSTRING::from(&app.uninstall), RemovalOptions::None)
            .map_err(|e| e.to_string())?;
        let result = op.join().map_err(|e| e.to_string())?;
        let err = result.ExtendedErrorCode().map_err(|e| e.to_string())?;
        return if err.is_ok() {
            Ok(())
        } else {
            Err(result
                .ErrorText()
                .map(|t| t.to_string())
                .unwrap_or_else(|_| err.message()))
        };
    }
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::PCWSTR;
    let (exe, args) = split_command(&app.uninstall);
    let (exe_w, args_w, verb) = (
        crate::system::wide(&exe),
        crate::system::wide(&args),
        crate::system::wide("open"),
    );
    let r = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(args_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if r.0 as usize > 32 {
        Ok(())
    } else {
        Err(format!("Windows could not start the uninstaller ({exe})."))
    }
}

#[cfg(not(windows))]
pub fn uninstall(_app: &App) -> Result<(), String> {
    Err("Uninstall is not available on this system yet.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn install_dates() {
        assert_eq!(ymd_to_ms("20000101"), 946_684_800_000);
        assert_eq!(ymd_to_ms("20260101"), 1_767_225_600_000);
        assert_eq!(ymd_to_ms("2026"), 0);
        assert_eq!(ymd_to_ms("20261341"), 0);
    }

    #[cfg(windows)]
    #[test]
    fn uninstall_commands() {
        assert_eq!(
            split_command("\"C:\\Program Files\\App\\uninst.exe\" /S"),
            ("C:\\Program Files\\App\\uninst.exe".into(), "/S".into())
        );
        assert_eq!(
            split_command("MsiExec.exe /X{1234}"),
            ("MsiExec.exe".into(), "/X{1234}".into())
        );
        assert_eq!(
            split_command("C:\\Program Files\\App\\unins000.exe /SILENT"),
            (
                "C:\\Program Files\\App\\unins000.exe".into(),
                "/SILENT".into()
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn icon_to_exe() {
        assert_eq!(
            exe_from_icon("\"C:\\App\\code.exe\",0"),
            Some("CODE.EXE".into())
        );
        assert_eq!(exe_from_icon("C:\\App\\app.ico"), None);
    }

    #[test]
    fn lists_apps_without_panic() {
        let apps = list(&[]);
        assert!(apps.iter().all(|a| !a.name.is_empty()));
    }
}

//! Install and uninstall steps. The NSIS installer inside the setup does the file work
//! (files, Start menu shortcut, uninstall entry); this module adds the options around it.

use std::cmp::Ordering;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

#[path = "../../../src-tauri/src/settings.rs"]
#[allow(dead_code)]
mod settings;

/// The Lustro NSIS installer (empty in a dev build, see build.rs).
pub const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.exe"));
pub const VERSION: &str = env!("LUSTRO_VERSION");
/// The themed uninstaller: a copy of this setup in the install folder.
pub const SETUP_NAME: &str = "lustro-setup.exe";

// Names that the Tauri NSIS template uses (PRODUCTNAME, MANUFACTURER, BUNDLEID).
const UNINST_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Lustro";
const MANU_KEY: &str = r"Software\Lustro contributors";
const BUNDLE_ID: &str = "com.anonmohak.lustro";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Installed {
    pub version: String,
    pub dir: PathBuf,
    /// For example `lustro.exe`.
    pub exe: String,
}

/// The current Lustro install, from its uninstall entry.
pub fn installed() -> Option<Installed> {
    let k = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(UNINST_KEY)
        .ok()?;
    let version: String = k.get_value("DisplayVersion").ok()?;
    let dir: String = k.get_value("InstallLocation").ok()?;
    Some(Installed {
        version,
        dir: PathBuf::from(dir.trim_matches('"')),
        exe: k
            .get_value("MainBinaryName")
            .unwrap_or_else(|_| "lustro.exe".into()),
    })
}

/// The NSIS default for a current-user install.
pub fn default_dir() -> PathBuf {
    local_app_data().join("Lustro")
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Compares versions like `1.2.3` part by part, as numbers.
pub fn compare(a: &str, b: &str) -> Ordering {
    let parts = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .take(3)
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parts(a).cmp(&parts(b))
}

/// True when a Lustro process runs (the installer then closes it).
pub fn app_running(exe: &str) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {exe}"), "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_ascii_lowercase()
                .contains(&exe.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

pub struct Options {
    pub desktop: bool,
    pub start_with_windows: bool,
    pub explorer_menu: bool,
}

/// Installs Lustro to `dir`. `update` keeps the shortcuts and options of an existing install.
pub fn install(
    dir: &Path,
    opts: &Options,
    update: bool,
    step: impl Fn(&str),
) -> Result<(), String> {
    if PAYLOAD.is_empty() {
        return Err(
            "This setup has no Lustro installer inside. Build it with npm run build:setup.".into(),
        );
    }
    step("extract");
    let tmp = std::env::temp_dir().join(format!("lustro-setup-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("Could not make a temporary folder: {e}"))?;
    let nsis = tmp.join("lustro-installer.exe");
    std::fs::write(&nsis, PAYLOAD).map_err(|e| format!("Could not write the installer: {e}"))?;

    step("install");
    let mut cmd = Command::new(&nsis);
    cmd.arg("/S").creation_flags(CREATE_NO_WINDOW);
    if update {
        cmd.arg("/UPDATE");
    }
    // NSIS wants /D= last and without quotes, also when the path has spaces.
    cmd.raw_arg(format!("/D={}", dir.display()));
    let status = cmd.status();
    let _ = std::fs::remove_dir_all(&tmp);
    let status = status.map_err(|e| format!("Could not start the installer: {e}"))?;
    if !status.success() {
        return Err(format!(
            "The installer stopped with code {}.",
            status.code().unwrap_or(-1)
        ));
    }
    let app = installed().ok_or("The installer did not register Lustro.")?;

    step("options");
    if !update {
        if !opts.desktop {
            // A silent NSIS install always makes the desktop shortcut.
            if let Some(desktop) = desktop_dir() {
                let _ = std::fs::remove_file(desktop.join("Lustro.lnk"));
            }
        }
        if opts.start_with_windows || opts.explorer_menu {
            apply_settings(&app, opts)
                .map_err(|e| format!("Lustro is installed, but its options were not saved: {e}"))?;
        }
    }

    step("uninstaller");
    set_themed_uninstaller(&app.dir)
        .map_err(|e| format!("Lustro is installed, but the uninstaller was not set up: {e}"))?;
    Ok(())
}

/// The same registry entries and settings.json values as the Settings screen makes.
fn apply_settings(app: &Installed, opts: &Options) -> std::io::Result<()> {
    let exe = app.dir.join(&app.exe).to_string_lossy().into_owned();
    let data = local_app_data().join(BUNDLE_ID);
    let mut s = settings::load(&data);
    if opts.start_with_windows {
        settings::set_start_with_windows_for(&exe, true)?;
        s.start_with_windows = true;
    }
    if opts.explorer_menu {
        settings::set_explorer_menu_for(&exe, true)?;
        s.explorer_menu = true;
    }
    settings::save(&data, &s)
}

/// Copies this setup into the install folder and makes Windows use it to uninstall.
/// The NSIS hook (src-tauri/windows/hooks.nsh) keeps this after in-app updates.
fn set_themed_uninstaller(dir: &Path) -> std::io::Result<()> {
    let me = std::env::current_exe()?;
    let target = dir.join(SETUP_NAME);
    if !same_file(&me, &target) {
        std::fs::copy(&me, &target)?;
    }
    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINST_KEY)?;
    k.set_value(
        "UninstallString",
        &format!("\"{}\" --uninstall", target.display()),
    )
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Removes Lustro from `dir`. `delete_data` also removes scans, history and settings.
pub fn uninstall(dir: &Path, delete_data: bool, step: impl Fn(&str)) -> Result<(), String> {
    step("remove");
    let nsis = dir.join("uninstall.exe");
    if nsis.exists() {
        // `_?=` runs the NSIS uninstaller in place, so this waits for it to finish.
        let status = Command::new(&nsis)
            .arg("/S")
            .raw_arg(format!("_?={}", dir.display()))
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("Could not start the uninstaller: {e}"))?;
        if !status.success() {
            return Err(format!(
                "The uninstaller stopped with code {}.",
                status.code().unwrap_or(-1)
            ));
        }
        let _ = std::fs::remove_file(&nsis);
    }
    let _ = std::fs::remove_file(dir.join(SETUP_NAME));
    let _ = std::fs::remove_dir(dir);

    if delete_data {
        step("data");
        let _ = std::fs::remove_dir_all(local_app_data().join(BUNDLE_ID));
        if let Some(roaming) = std::env::var_os("APPDATA") {
            let _ = std::fs::remove_dir_all(PathBuf::from(roaming).join(BUNDLE_ID));
        }
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(format!(r"{MANU_KEY}\Lustro"));
        let _ = hkcu.delete_subkey(MANU_KEY);
    }
    Ok(())
}

/// Starts Lustro after an install.
pub fn launch() -> Result<(), String> {
    let app = installed().ok_or("Lustro is not installed.")?;
    Command::new(app.dir.join(&app.exe))
        .current_dir(&app.dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start Lustro: {e}"))
}

fn desktop_dir() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Desktop, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    // SAFETY: the returned string is freed once, after it is copied.
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.map(PathBuf::from)
    }
}

/// Runs the NSIS uninstaller next to this exe with the same arguments and returns its exit code.
/// The NSIS installer calls the registered uninstaller this way (`/UPDATE`, `/P`, `_?=`).
pub fn pass_through(args: &[String]) -> i32 {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return 2;
    };
    let mut cmd = Command::new(dir.join("uninstall.exe"));
    for a in args {
        if a.starts_with("_?=") {
            cmd.raw_arg(a)
        } else {
            cmd.arg(a)
        };
    }
    cmd.status().ok().and_then(|s| s.code()).unwrap_or(2)
}

/// Deletes this exe a few seconds after the process ends (for the copy in %TEMP%).
pub fn delete_self_later() {
    if let Ok(me) = std::env::current_exe() {
        let _ = Command::new("cmd")
            .raw_arg(format!(
                "/c ping -n 3 127.0.0.1 >nul & del /q \"{}\"",
                me.display()
            ))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(compare("1.0.10", "1.0.9"), Ordering::Greater);
        assert_eq!(compare("1.0.1", "1.0.1"), Ordering::Equal);
        assert_eq!(compare("1.0.1", "1.1.0"), Ordering::Less);
        assert_eq!(compare("2.0.0-beta", "2.0.0"), Ordering::Equal);
    }
}

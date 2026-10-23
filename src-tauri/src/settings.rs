//! Settings that the backend needs (tray, alerts, Windows integration), kept in settings.json.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
#[cfg(windows)]
use winreg::RegKey;
#[cfg(windows)]
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Closing the window keeps Lustro in the tray.
    pub keep_in_tray: bool,
    pub low_space_alert: bool,
    /// Alert when a drive has less free space than this percent.
    pub low_space_percent: u8,
    pub start_with_windows: bool,
    pub explorer_menu: bool,
    /// Admin may delete app folders in Program Files, after a warning each time.
    pub allow_app_folders: bool,
    /// Favourite folders in the sidebar, in the order the user added them.
    pub favorites: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            keep_in_tray: true,
            low_space_alert: true,
            low_space_percent: 10,
            start_with_windows: false,
            explorer_menu: false,
            allow_app_folders: false,
            favorites: Vec::new(),
        }
    }
}

fn file(dir: &Path) -> PathBuf {
    dir.join("settings.json")
}

pub fn load(dir: &Path) -> Settings {
    std::fs::read(file(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, s: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(file(dir), serde_json::to_vec_pretty(s)?)
}

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const MENU_KEYS: [&str; 3] = [
    r"Software\Classes\Directory\shell\Lustro",
    r"Software\Classes\Directory\Background\shell\Lustro",
    r"Software\Classes\Drive\shell\Lustro",
];

#[cfg(windows)]
fn exe() -> std::io::Result<String> {
    Ok(std::env::current_exe()?.to_string_lossy().into_owned())
}

/// Adds or removes the "start with Windows" entry (current user, no admin needed).
#[cfg(windows)]
pub fn set_start_with_windows(on: bool) -> std::io::Result<()> {
    set_start_with_windows_for(&exe()?, on)
}

/// Like `set_start_with_windows`, for the Lustro exe at `exe` (the setup app uses this).
#[cfg(windows)]
pub fn set_start_with_windows_for(exe: &str, on: bool) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu.create_subkey(RUN_KEY)?;
    if on {
        run.set_value("Lustro", &format!("\"{exe}\" --tray"))
    } else {
        match run.delete_value("Lustro") {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
}

/// Adds or removes "Analyze with Lustro" in the right-click menu of folders and drives.
/// It shows under "Show more options" on Windows 11 (the top-level menu needs a signed package).
#[cfg(windows)]
pub fn set_explorer_menu(on: bool) -> std::io::Result<()> {
    set_explorer_menu_for(&exe()?, on)
}

/// Like `set_explorer_menu`, for the Lustro exe at `exe` (the setup app uses this).
#[cfg(windows)]
pub fn set_explorer_menu_for(exe: &str, on: bool) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for key in MENU_KEYS {
        if on {
            let (k, _) = hkcu.create_subkey(key)?;
            k.set_value("", &"Analyze with Lustro")?;
            k.set_value("Icon", &exe)?;
            let (cmd, _) = k.create_subkey("command")?;
            cmd.set_value("", &format!("\"{exe}\" --scan \"%V\""))?;
        } else {
            match hkcu.delete_subkey_all(key) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
    }
    Ok(())
}

/// True when the registry entries match the settings (for example after a move of the app).
#[cfg(windows)]
pub fn registry_matches(s: &Settings) -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_ok = hkcu
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .and_then(|k| k.get_value::<String, _>("Lustro"))
        .is_ok();
    let menu_ok = hkcu
        .open_subkey_with_flags(MENU_KEYS[0], KEY_READ | KEY_WRITE)
        .is_ok();
    run_ok == s.start_with_windows && menu_ok == s.explorer_menu
}

/// Start at login on macOS and Android comes in a later phase.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn set_start_with_windows(on: bool) -> std::io::Result<()> {
    unsupported(on, "Start at login")
}

/// `~/.config/autostart/lustro.desktop`: desktop sessions start the programs in this folder.
#[cfg(target_os = "linux")]
fn autostart_file() -> std::io::Result<PathBuf> {
    let config = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| Path::new(&h).join(".config"))
        })
        .ok_or_else(|| std::io::Error::other("There is no home folder."))?;
    Ok(config.join("autostart").join("lustro.desktop"))
}

/// The program to start: the AppImage file when Lustro runs from one (the running
/// program is then inside a temporary mount).
#[cfg(target_os = "linux")]
fn exe() -> std::io::Result<String> {
    match std::env::var("APPIMAGE") {
        Ok(a) if !a.is_empty() => Ok(a),
        _ => Ok(std::env::current_exe()?.to_string_lossy().into_owned()),
    }
}

/// One `Exec=` argument: quoted, with the escapes of the Desktop Entry specification.
#[cfg(target_os = "linux")]
fn exec_arg(arg: &str) -> String {
    let mut quoted = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    // The value of a key is a string too, so each backslash is written twice.
    quoted.replace('\\', "\\\\")
}

#[cfg(target_os = "linux")]
fn autostart_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Lustro\nComment=Storage manager\nExec={} --tray\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exec_arg(exe)
    )
}

/// Adds or removes the autostart entry (current user, no admin needed).
#[cfg(target_os = "linux")]
pub fn set_start_with_windows(on: bool) -> std::io::Result<()> {
    let file = autostart_file()?;
    if on {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&file, autostart_entry(&exe()?))
    } else {
        match std::fs::remove_file(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
}

/// A file manager menu on Linux and macOS comes in a later phase.
#[cfg(not(windows))]
pub fn set_explorer_menu(on: bool) -> std::io::Result<()> {
    unsupported(on, "The file manager menu")
}

/// True when the autostart entry matches the settings (for example after a move of the app).
#[cfg(target_os = "linux")]
pub fn registry_matches(s: &Settings) -> bool {
    let current = match (autostart_file(), exe()) {
        (Ok(f), Ok(e)) => std::fs::read_to_string(f).is_ok_and(|t| t == autostart_entry(&e)),
        _ => false,
    };
    current == s.start_with_windows && !s.explorer_menu
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn registry_matches(s: &Settings) -> bool {
    !s.start_with_windows && !s.explorer_menu
}

/// Turning a feature off always works; turning it on fails until the feature exists.
#[cfg(not(windows))]
fn unsupported(on: bool, what: &str) -> std::io::Result<()> {
    if on {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("{what} is not available on this system yet."),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn autostart_exec_is_quoted() {
        assert_eq!(exec_arg("/opt/Lustro/lustro"), "\"/opt/Lustro/lustro\"");
        assert_eq!(
            exec_arg("/home/me/My Apps/lustro"),
            "\"/home/me/My Apps/lustro\""
        );
        // `$` gets one backslash for Exec, and the backslash is doubled for the string.
        assert_eq!(exec_arg("/a$b"), "\"/a\\\\$b\"");
        let entry = autostart_entry("/usr/bin/lustro");
        assert!(entry.contains("Exec=\"/usr/bin/lustro\" --tray\n"));
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join(format!("lustro-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load(&dir), Settings::default());
        let s = Settings {
            low_space_percent: 5,
            explorer_menu: true,
            ..Default::default()
        };
        save(&dir, &s).unwrap();
        assert_eq!(load(&dir), s);
        // Missing fields take their default values.
        std::fs::write(file(&dir), br#"{"lowSpacePercent": 15}"#).unwrap();
        let partial = load(&dir);
        assert_eq!(partial.low_space_percent, 15);
        assert!(partial.keep_in_tray);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

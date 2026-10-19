//! Paths that Lustro never deletes. The backend checks these before every delete;
//! the UI only shows the result.

/// Folders that are protected together with everything inside them.
/// Paths are relative to a drive root, in lower case.
const PROTECTED_TREES: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata\\microsoft",
    "system volume information",
    "recovery",
    "boot",
    "efi",
    "config.msi",
    "$winreagent",
    "users\\default",
    "users\\all users",
    "documents and settings",
];

/// Single paths that are protected, but whose contents are not.
const PROTECTED_ITEMS: &[&str] = &["users", "users\\public", "programdata", "$recycle.bin"];

/// Files in a drive root that Windows needs.
const ROOT_FILES: &[&str] = &[
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "dumpstack.log",
    "dumpstack.log.tmp",
    "bootmgr",
    "bootnxt",
    "bootsect.bak",
];

/// Folders and files directly inside a user profile that must stay.
const PROFILE_ITEMS: &[&str] = &[
    "appdata",
    "appdata\\local",
    "appdata\\roaming",
    "appdata\\locallow",
];

/// Places inside protected folders where the cleaner may delete the contents (never the folder itself).
/// Paths are relative to a drive root, in lower case.
const CLEANER_EXCEPTIONS: &[&str] = &[
    "windows\\temp",
    "windows\\softwaredistribution\\download",
    "windows\\serviceprofiles\\networkservice\\appdata\\local\\microsoft\\windows\\deliveryoptimization\\cache",
    "windows\\minidump",
    "programdata\\microsoft\\windows\\wer\\reportarchive",
    "programdata\\microsoft\\windows\\wer\\reportqueue",
];

/// Single protected files that the cleaner may delete.
const CLEANER_FILES: &[&str] = &["windows\\memory.dmp"];

/// Protected folders whose contents (apps and games) an admin may delete after a warning,
/// when Settings > "Allow deleting app folders" is on. The folders themselves stay.
const APP_TREES: &[&str] = &["program files", "program files (x86)"];

/// Folders inside the app trees that Windows owns. They stay locked, like every
/// folder whose name starts with "windows" (WindowsApps, Windows Defender, ...).
const APP_LOCKED: &[&str] = &[
    "common files",
    "internet explorer",
    "microsoft update health tools",
    "modifiablewindowsapps",
    "msbuild",
    "reference assemblies",
    "uninstall information",
];

fn normalize(path: &str) -> String {
    let p = path.replace('/', "\\");
    let p = p.strip_prefix("\\\\?\\").unwrap_or(&p);
    p.trim_end_matches('\\').to_lowercase()
}

/// True when the cleaner may delete `path`: the path is not protected, or it lies
/// inside one of the cleaner exceptions (but is not the exception folder itself).
pub fn cleaner_may_delete(path: &str) -> bool {
    if !is_protected(path) {
        return true;
    }
    let p = normalize(path);
    if p.len() < 4 || p.as_bytes()[1] != b':' {
        return false;
    }
    let rel = &p[3..];
    CLEANER_FILES.contains(&rel)
        || CLEANER_EXCEPTIONS.iter().any(|e| {
            rel.len() > e.len() + 1 && rel.starts_with(e) && rel.as_bytes()[e.len()] == b'\\'
        })
}

/// True when `path` is inside Program Files or Program Files (x86), in an app or game folder
/// that is not one of Windows' own. An admin may delete it after a warning (see `APP_TREES`).
pub fn is_app_folder(path: &str) -> bool {
    let p = normalize(path);
    let b = p.as_bytes();
    if b.len() < 4 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' {
        return false;
    }
    if p.contains("\\..\\") || p.ends_with("\\..") || p.contains("\\.\\") {
        return false;
    }
    let rel = &p[3..];
    APP_TREES.iter().any(|t| {
        let Some(rest) = rel.strip_prefix(t).and_then(|r| r.strip_prefix('\\')) else {
            return false;
        };
        let first = rest.split('\\').next().unwrap_or("");
        !first.is_empty() && !first.starts_with("windows") && !APP_LOCKED.contains(&first)
    })
}

/// True when Lustro must not delete `path`.
/// `path` is an absolute Windows path, for example `C:\Windows\System32`.
pub fn is_protected(path: &str) -> bool {
    let p = normalize(path);
    let b = p.as_bytes();
    // Anything that is not a plain drive path (network shares, device paths) is outside Lustro.
    if b.len() < 2 || !b[0].is_ascii_alphabetic() || b[1] != b':' {
        return true;
    }
    if b.len() == 2 {
        return true; // drive root
    }
    if b[2] != b'\\' || p.contains("\\..\\") || p.ends_with("\\..") || p.contains("\\.\\") {
        return true; // relative or unusual paths
    }
    let rel = &p[3..];
    let first = rel.split('\\').next().unwrap_or("");

    // NTFS metafiles ($MFT, $LogFile, $Extend, ...) live in the drive root.
    if first.starts_with('$') && first != "$recycle.bin" {
        return true;
    }
    if ROOT_FILES.contains(&rel) || PROTECTED_ITEMS.contains(&rel) {
        return true;
    }
    if PROTECTED_TREES
        .iter()
        .any(|t| rel == *t || rel.starts_with(&format!("{t}\\")))
    {
        return true;
    }
    // The Recycle Bin folders of each user: delete through the Recycle Bin, not by hand.
    if let Some(rest) = rel.strip_prefix("$recycle.bin\\")
        && !rest.contains('\\')
    {
        return true;
    }
    // User profile roots and their AppData folders. Files inside stay deletable.
    if let Some(rest) = rel.strip_prefix("users\\") {
        let mut parts = rest.splitn(2, '\\');
        let _user = parts.next();
        match parts.next() {
            None => return true,
            Some(inner) => {
                if PROFILE_ITEMS.contains(&inner)
                    || inner.starts_with("ntuser.")
                    || inner.starts_with("usrclass.")
                {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{cleaner_may_delete, is_app_folder, is_protected};

    #[test]
    fn app_folders() {
        for p in [
            "C:\\Program Files (x86)\\Steam",
            "C:\\Program Files (x86)\\Steam\\steamapps\\common\\SomeGame",
            "C:\\Program Files\\Epic Games\\Fortnite",
            "D:\\Program Files\\App\\app.exe",
            "\\\\?\\C:\\Program Files\\App",
        ] {
            assert!(is_app_folder(p), "{p} is an app folder");
            assert!(is_protected(p), "{p} stays protected without the switch");
        }
        for p in [
            "C:\\Program Files",
            "C:\\Program Files (x86)",
            "C:\\Program Files\\",
            "C:\\Program Files\\WindowsApps\\Some.App",
            "C:\\Program Files\\Windows Defender",
            "C:\\Program Files (x86)\\Windows Kits\\10",
            "C:\\Program Files\\Common Files\\System",
            "C:\\Program Files (x86)\\Internet Explorer",
            "C:\\Program Files\\App\\..\\..\\Windows",
            "C:\\Windows\\System32",
            "C:\\ProgramData\\Microsoft\\Windows",
            "C:\\Program Files2\\App",
            "C:\\Users\\me\\Downloads",
        ] {
            assert!(!is_app_folder(p), "{p} is not an app folder");
        }
    }

    #[test]
    fn system_paths_are_protected() {
        for p in [
            "C:\\",
            "C:",
            "c:\\windows",
            "C:\\Windows\\System32\\drivers\\etc\\hosts",
            "C:\\Program Files",
            "C:\\Program Files (x86)\\Steam",
            "D:\\Program Files\\App",
            "C:\\ProgramData",
            "C:\\ProgramData\\Microsoft\\Windows",
            "C:\\pagefile.sys",
            "C:\\hiberfil.sys",
            "C:\\$MFT",
            "C:\\$Extend\\$UsnJrnl",
            "C:\\System Volume Information",
            "C:\\$Recycle.Bin",
            "C:\\$Recycle.Bin\\S-1-5-21-1",
            "C:\\Users",
            "C:\\Users\\mohak",
            "C:\\Users\\mohak\\",
            "C:\\Users\\mohak\\AppData",
            "C:\\Users\\mohak\\AppData\\Local",
            "C:\\Users\\mohak\\NTUSER.DAT",
            "C:\\Users\\Default\\Desktop",
            "C:\\Users\\Public",
            "\\\\?\\C:\\Windows\\explorer.exe",
            "\\\\server\\share\\file.txt",
            "C:\\Users\\mohak\\..\\other",
            "relative\\path",
        ] {
            assert!(is_protected(p), "{p} must be protected");
        }
    }

    #[test]
    fn user_data_is_not_protected() {
        for p in [
            "C:\\Users\\mohak\\Downloads",
            "C:\\Users\\mohak\\Downloads\\ubuntu.iso",
            "C:\\Users\\mohak\\AppData\\Local\\Temp",
            "C:\\Users\\mohak\\AppData\\Local\\Temp\\x.tmp",
            "C:\\Users\\Public\\Videos\\clip.mp4",
            "C:\\ProgramData\\Package Cache",
            "C:\\$Recycle.Bin\\S-1-5-21-1\\$R123.mp4",
            "C:\\XboxGames\\Halo",
            "D:\\Photos",
            "C:\\Windows.old",
            "C:\\WindowsApps-backup",
        ] {
            assert!(!is_protected(p), "{p} must not be protected");
        }
    }

    #[test]
    fn cleaner_exceptions() {
        assert!(cleaner_may_delete("C:\\Windows\\Temp\\abc.tmp"));
        assert!(cleaner_may_delete(
            "C:\\Windows\\SoftwareDistribution\\Download\\x\\y.cab"
        ));
        assert!(cleaner_may_delete("C:\\Windows\\MEMORY.DMP"));
        assert!(cleaner_may_delete("C:\\Users\\me\\AppData\\Local\\Temp\\a"));
        assert!(
            !cleaner_may_delete("C:\\Windows\\Temp"),
            "the folder itself stays"
        );
        assert!(!cleaner_may_delete("C:\\Windows\\System32\\kernel32.dll"));
        assert!(!cleaner_may_delete("C:\\Windows\\Temp2\\x"));
        assert!(!cleaner_may_delete("C:\\"));
        assert!(!cleaner_may_delete("C:\\Users\\me"));
    }
}

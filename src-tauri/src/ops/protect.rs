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

/// True when Lustro must not delete `path`.
/// `path` is an absolute Windows path, for example `C:\Windows\System32`.
pub fn is_protected(path: &str) -> bool {
    let p = path.replace('/', "\\");
    let p = p.strip_prefix("\\\\?\\").unwrap_or(&p);
    let p = p.trim_end_matches('\\').to_lowercase();
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
    use super::is_protected;

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
}

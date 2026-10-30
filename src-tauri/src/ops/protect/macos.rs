//! macOS paths that Lustro never deletes. The file system ignores case, so the tables are
//! in lower case. `/etc`, `/tmp` and `/var` are links into `/private`; both forms are listed.

use super::unix::Rules;

pub const RULES: Rules = Rules {
    // The SIP paths, the system and app folders, and `/usr/local` and `/opt` (Homebrew).
    system_trees: &[
        "applications",
        "bin",
        "dev",
        "etc",
        "library",
        "opt",
        "private",
        "sbin",
        "system",
        "usr",
        "var",
    ],
    system_items: &["cores", "network", "tmp", "users/shared", "volumes"],
    // Each disk is mounted at /Volumes/<name>.
    mount_parents: &["volumes"],
    homes: "users",
    root_home: "/var/root",
    home_trees: &[".ssh", ".gnupg", "library/keychains"],
    // The user folders use their English names, as on disk (the Finder translates them).
    home_items: &[
        ".cache",
        ".config",
        ".local",
        ".local/share",
        ".trash",
        "applications",
        "desktop",
        "documents",
        "downloads",
        "library",
        "library/application support",
        "library/caches",
        "library/containers",
        "library/group containers",
        "library/logs",
        "library/preferences",
        "movies",
        "music",
        "pictures",
        "public",
    ],
    // The caches and logs of the system, and the per-user temp folders ($TMPDIR).
    cleaner_exceptions: &[
        "library/caches",
        "library/logs",
        "private/var/folders",
        "var/folders",
    ],
    app_trees: &["applications"],
    ignore_case: true,
};

#[cfg(test)]
mod tests {
    use super::super::unix;
    use super::RULES;

    const HOME: Option<&str> = Some("/Users/me");

    fn protected(path: &str) -> bool {
        unix::protected(&RULES, path, HOME)
    }

    #[test]
    fn system_paths_are_protected() {
        for p in [
            "/",
            "/System",
            "/System/Library/CoreServices/Finder.app",
            "/system/library",
            "/System/Volumes/Data",
            "/usr/bin/zip",
            "/usr/local/bin/brew",
            "/opt/homebrew/bin/brew",
            "/private/etc/hosts",
            "/etc/hosts",
            "/var/db/receipts",
            "/Library",
            "/Library/Preferences/com.apple.x.plist",
            "/Applications",
            "/Applications/Safari.app",
            "/Users",
            "/Users/Shared",
            "/Volumes",
            "/Volumes/Backup",
            "/cores",
            "/tmp",
            "/var/root",
            "relative/path",
            "/Users/me/../../etc",
        ] {
            assert!(protected(p), "{p} is protected");
        }
    }

    #[test]
    fn home_folders_stay_but_their_contents_do_not() {
        for p in [
            "/Users/me",
            "/users/ME",
            "/Users/other",
            "/Users/me/Downloads",
            "/Users/me/Library",
            "/Users/me/Library/Caches",
            "/Users/me/library/application support",
            "/Users/me/.Trash",
            "/Users/me/.ssh/id_ed25519",
            "/Users/me/Library/Keychains/login.keychain-db",
        ] {
            assert!(protected(p), "{p} is protected");
        }
        for p in [
            "/Users/me/Downloads/big.dmg",
            "/Users/me/Library/Caches/com.apple.Safari",
            "/Users/me/Library/Developer/Xcode/DerivedData",
            "/Users/me/Library/Application Support/OldApp",
            "/Users/other/Movies/clip.mov",
            "/Users/Shared/old.zip",
            "/Volumes/Backup/photo.jpg",
            "/tmp/lustro-test",
            "/cores/core.123",
        ] {
            assert!(!protected(p), "{p} is not protected");
        }
    }

    #[test]
    fn cleaner_exceptions() {
        let may = |p| unix::cleaner_may_delete(&RULES, p, HOME);
        assert!(may("/Library/Caches/com.apple.x"));
        assert!(may("/Library/Logs/DiagnosticReports/x.ips"));
        assert!(may("/private/var/folders/ab/cd/T/old"));
        assert!(may("/var/folders/ab/cd/T/old"));
        assert!(!may("/Library/Caches"));
        assert!(!may("/Library/Preferences/x.plist"));
        assert!(!may("/var/db/x"));
    }

    #[test]
    fn app_folders() {
        assert!(unix::is_app_folder(&RULES, "/Applications/Old.app"));
        assert!(unix::is_app_folder(
            &RULES,
            "/applications/Old.app/Contents"
        ));
        assert!(!unix::is_app_folder(&RULES, "/Applications"));
        assert!(!unix::is_app_folder(
            &RULES,
            "/System/Applications/Mail.app"
        ));
    }
}

//! Linux paths that Lustro never deletes.

use super::unix::Rules;

pub const RULES: Rules = Rules {
    system_trees: &[
        "bin",
        "boot",
        "dev",
        "efi",
        "etc",
        "lib",
        "lib32",
        "lib64",
        "libx32",
        "lost+found",
        "opt",
        "proc",
        "run",
        "sbin",
        "snap",
        "sys",
        "usr",
        "var",
    ],
    system_items: &[
        "home", "media", "mnt", "root", "srv", "tmp", "swapfile", "swap.img",
    ],
    // The folder of each user under /media, where removable disks are mounted.
    mount_parents: &["media"],
    homes: "home",
    root_home: "/root",
    home_trees: &[".ssh", ".gnupg", ".pki"],
    // The user folders use their English names (localized names come later).
    home_items: &[
        ".cache",
        ".config",
        ".local",
        ".local/bin",
        ".local/share",
        ".local/state",
        "Desktop",
        "Documents",
        "Downloads",
        "Music",
        "Pictures",
        "Public",
        "Templates",
        "Videos",
    ],
    cleaner_exceptions: &[
        "var/tmp",
        "var/crash",
        "var/cache/apt/archives",
        "var/lib/systemd/coredump",
    ],
    app_trees: &["opt"],
    ignore_case: false,
};

#[cfg(test)]
mod tests {
    use super::super::unix;
    use super::RULES;

    const HOME: Option<&str> = Some("/home/me");

    fn protected(path: &str, home: Option<&str>) -> bool {
        unix::protected(&RULES, path, home)
    }

    fn cleaner_may_delete(path: &str) -> bool {
        unix::cleaner_may_delete(&RULES, path, HOME)
    }

    fn is_app_folder(path: &str) -> bool {
        unix::is_app_folder(&RULES, path)
    }

    #[test]
    fn system_paths_are_protected() {
        for p in [
            "/",
            "/usr",
            "/usr/lib/libc.so.6",
            "/etc/",
            "/boot/vmlinuz",
            "/var/log/syslog",
            "/home",
            "/tmp",
            "/swapfile",
            "/media/me",
            "/opt",
            "/opt/app",
            "relative/path",
            "/home/me/../../etc",
            "/home//me",
            "",
        ] {
            assert!(protected(p, HOME), "{p} is protected");
        }
    }

    #[test]
    fn home_folders_stay_but_their_contents_do_not() {
        for p in [
            "/home/me",
            "/home/me/",
            "/home/other",
            "/root",
            "/home/me/Downloads",
            "/home/me/.config",
            "/home/me/.local/share",
            "/home/me/.ssh",
            "/home/me/.ssh/id_ed25519",
            "/home/other/.gnupg/pubring.kbx",
        ] {
            assert!(protected(p, HOME), "{p} is protected");
        }
        for p in [
            "/home/me/Downloads/big.iso",
            "/home/me/.cache/thumbnails",
            "/home/me/.config/app/cache",
            "/home/me/code/target",
            "/home/other/Videos/clip.mp4",
            "/root/old.tar",
            "/tmp/lustro-test",
            "/media/me/Disk/photo.jpg",
            "/mnt/data/backup",
            "/srv/www",
            "/data/projects",
            "/homes/me",
        ] {
            assert!(!protected(p, HOME), "{p} is not protected");
        }
    }

    #[test]
    fn case_matters() {
        assert!(!protected("/USR/lib", HOME));
        assert!(!protected("/home/me/downloads", HOME));
    }

    #[test]
    fn home_outside_of_home() {
        let home = Some("/var/lib/me");
        assert!(protected("/var/lib/me", home));
        assert!(protected("/var/lib/me/file", home), "/var stays protected");
        assert!(protected("/data/me", Some("/data/me")));
        assert!(!protected("/data/me/file", Some("/data/me")));
        assert!(protected("/data/me/.ssh/key", Some("/data/me")));
    }

    #[test]
    fn cleaner_exceptions() {
        assert!(cleaner_may_delete("/var/tmp/old"));
        assert!(cleaner_may_delete("/var/cache/apt/archives/x.deb"));
        assert!(!cleaner_may_delete("/var/tmp"));
        assert!(!cleaner_may_delete("/var/lib/dpkg/status"));
        assert!(!cleaner_may_delete("/var/tmpx/a"));
    }

    #[test]
    fn app_folders() {
        assert!(is_app_folder("/opt/app"));
        assert!(is_app_folder("/opt/app/bin/app"));
        assert!(!is_app_folder("/opt"));
        assert!(!is_app_folder("/opt/../etc"));
        assert!(!is_app_folder("/optx/app"));
        assert!(!is_app_folder("/usr/lib/app"));
    }
}

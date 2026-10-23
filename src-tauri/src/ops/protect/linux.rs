//! Linux paths that Lustro never deletes.

/// Folders that are protected together with everything inside them.
/// Paths are relative to `/`. Case matters on Linux.
const SYSTEM_TREES: &[&str] = &[
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
];

/// Single paths that are protected, but whose contents are not.
const SYSTEM_ITEMS: &[&str] = &[
    "home", "media", "mnt", "root", "srv", "tmp", "swapfile", "swap.img",
];

/// Folders in a home folder that are protected with everything inside: keys and certificates.
const HOME_TREES: &[&str] = &[".ssh", ".gnupg", ".pki"];

/// Folders in a home folder that must stay, but whose contents are not protected.
/// The user folders use their English names (localized names come later).
const HOME_ITEMS: &[&str] = &[
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
];

/// Places inside protected folders where the cleaner may delete the contents (never the folder itself).
const CLEANER_EXCEPTIONS: &[&str] = &[
    "var/tmp",
    "var/crash",
    "var/cache/apt/archives",
    "var/lib/systemd/coredump",
];

/// Protected folders whose contents (apps) an admin may delete after a warning,
/// when Settings > "Allow deleting app folders" is on. The folders themselves stay.
const APP_TREES: &[&str] = &["opt"];

/// `path` without a trailing `/`, or None for a path that is not plain and absolute
/// (relative, with `.`, `..` or an empty part).
fn normalize(path: &str) -> Option<&str> {
    let p = if path == "/" {
        path
    } else {
        path.strip_suffix('/').unwrap_or(path)
    };
    let rest = p.strip_prefix('/')?;
    if !rest.is_empty()
        && rest
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
    {
        return None;
    }
    Some(p)
}

/// `rel` lies inside `tree` (and is not `tree` itself).
fn inside(rel: &str, tree: &str) -> bool {
    rel.strip_prefix(tree).is_some_and(|r| r.starts_with('/'))
}

/// The part of `p` below a home folder: `""` for the home folder itself.
fn in_home<'a>(p: &'a str, home: Option<&str>) -> Option<&'a str> {
    if let Some(h) = home
        .map(|h| h.trim_end_matches('/'))
        .filter(|h| !h.is_empty())
    {
        if p == h {
            return Some("");
        }
        if let Some(r) = p.strip_prefix(h).and_then(|r| r.strip_prefix('/')) {
            return Some(r);
        }
    }
    if p == "/root" {
        return Some("");
    }
    if let Some(r) = p.strip_prefix("/root/") {
        return Some(r);
    }
    let rest = p.strip_prefix("/home/")?;
    Some(rest.split_once('/').map_or("", |(_, inner)| inner))
}

fn protected(path: &str, home: Option<&str>) -> bool {
    let Some(p) = normalize(path) else {
        return true;
    };
    let rel = &p[1..];
    if rel.is_empty() || SYSTEM_ITEMS.contains(&rel) {
        return true;
    }
    if SYSTEM_TREES.iter().any(|t| rel == *t || inside(rel, t)) {
        return true;
    }
    // The folder of each user under /media, where removable disks are mounted.
    if rel.strip_prefix("media/").is_some_and(|r| !r.contains('/')) {
        return true;
    }
    if let Some(h) = in_home(p, home) {
        return h.is_empty()
            || HOME_ITEMS.contains(&h)
            || HOME_TREES.iter().any(|t| h == *t || inside(h, t));
    }
    false
}

/// True when Lustro must not delete `path`, an absolute path like `/usr/lib`.
pub fn is_protected(path: &str) -> bool {
    protected(path, std::env::var("HOME").ok().as_deref())
}

/// True when the cleaner may delete `path`: the path is not protected, or it lies
/// inside one of the cleaner exceptions (but is not the exception folder itself).
pub fn cleaner_may_delete(path: &str) -> bool {
    if !is_protected(path) {
        return true;
    }
    normalize(path).is_some_and(|p| CLEANER_EXCEPTIONS.iter().any(|e| inside(&p[1..], e)))
}

/// True when `path` is inside `/opt`, in an app folder. An admin may delete it after a warning.
pub fn is_app_folder(path: &str) -> bool {
    normalize(path).is_some_and(|p| APP_TREES.iter().any(|t| inside(&p[1..], t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: Option<&str> = Some("/home/me");

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
        ] {
            assert!(!protected(p, HOME), "{p} is not protected");
        }
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

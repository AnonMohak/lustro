//! The protected path check for Linux, macOS and Android. Each platform gives its own tables.

/// The protected paths of one platform. Paths are relative to `/`.
pub struct Rules {
    /// Folders that are protected together with everything inside them.
    pub system_trees: &'static [&'static str],
    /// Single paths that are protected, but whose contents are not.
    pub system_items: &'static [&'static str],
    /// Folders whose direct subfolders are protected (mount points: `/media/<user>`, `/Volumes/<disk>`).
    pub mount_parents: &'static [&'static str],
    /// Folders inside each mounted volume (a child of a mount parent) that are protected with
    /// everything inside (Android: `Android/data` on an SD card).
    pub volume_trees: &'static [&'static str],
    /// The folder that holds the home folders (`home`, `Users`).
    pub homes: &'static str,
    /// The home folder of root.
    pub root_home: &'static str,
    /// Folders in a home folder that are protected with everything inside: keys and certificates.
    pub home_trees: &'static [&'static str],
    /// Folders in a home folder that must stay, but whose contents are not protected.
    pub home_items: &'static [&'static str],
    /// Places inside protected folders where the cleaner may delete the contents (never the folder itself).
    pub cleaner_exceptions: &'static [&'static str],
    /// Protected folders whose contents (apps) an admin may delete after a warning,
    /// when Settings > "Allow deleting app folders" is on. The folders themselves stay.
    pub app_trees: &'static [&'static str],
    /// True when the file system ignores case. Then the tables must be in lower case.
    pub ignore_case: bool,
}

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

/// The normalized path, in lower case when the platform ignores case.
fn prepare(r: &Rules, path: &str) -> Option<String> {
    let p = normalize(path)?;
    Some(if r.ignore_case {
        p.to_lowercase()
    } else {
        p.to_string()
    })
}

/// `rel` lies inside `tree` (and is not `tree` itself).
fn inside(rel: &str, tree: &str) -> bool {
    rel.strip_prefix(tree).is_some_and(|r| r.starts_with('/'))
}

/// The part of `p` below a home folder: `""` for the home folder itself.
fn in_home<'a>(r: &Rules, p: &'a str, home: Option<&str>) -> Option<&'a str> {
    if let Some(h) = home
        .map(|h| h.trim_end_matches('/'))
        .filter(|h| !h.is_empty())
    {
        if p == h {
            return Some("");
        }
        if let Some(rest) = p.strip_prefix(h).and_then(|rest| rest.strip_prefix('/')) {
            return Some(rest);
        }
    }
    if p == r.root_home {
        return Some("");
    }
    if let Some(rest) = p
        .strip_prefix(r.root_home)
        .and_then(|rest| rest.strip_prefix('/'))
    {
        return Some(rest);
    }
    let rest = p
        .strip_prefix('/')?
        .strip_prefix(r.homes)?
        .strip_prefix('/')?;
    Some(rest.split_once('/').map_or("", |(_, inner)| inner))
}

/// True when Lustro must not delete `path`. `home` is the home folder of the user.
pub fn protected(r: &Rules, path: &str, home: Option<&str>) -> bool {
    let Some(p) = prepare(r, path) else {
        return true;
    };
    let home = home.map(|h| {
        if r.ignore_case {
            h.to_lowercase()
        } else {
            h.to_string()
        }
    });
    let rel = &p[1..];
    if rel.is_empty() || rel == r.homes || r.system_items.contains(&rel) {
        return true;
    }
    if r.system_trees.iter().any(|t| rel == *t || inside(rel, t)) {
        return true;
    }
    for m in r.mount_parents {
        if let Some(in_parent) = rel.strip_prefix(m).and_then(|x| x.strip_prefix('/')) {
            match in_parent.split_once('/') {
                None => return true,
                Some((_, in_volume)) => {
                    if r.volume_trees
                        .iter()
                        .any(|t| in_volume == *t || inside(in_volume, t))
                    {
                        return true;
                    }
                }
            }
        }
    }
    if let Some(h) = in_home(r, &p, home.as_deref()) {
        return h.is_empty()
            || r.home_items.contains(&h)
            || r.home_trees.iter().any(|t| h == *t || inside(h, t));
    }
    false
}

/// True when the cleaner may delete `path`: the path is not protected, or it lies
/// inside one of the cleaner exceptions (but is not the exception folder itself).
pub fn cleaner_may_delete(r: &Rules, path: &str, home: Option<&str>) -> bool {
    if !protected(r, path, home) {
        return true;
    }
    prepare(r, path).is_some_and(|p| r.cleaner_exceptions.iter().any(|e| inside(&p[1..], e)))
}

/// True when `path` is inside an app tree (`/opt`, `/Applications`), in an app folder.
pub fn is_app_folder(r: &Rules, path: &str) -> bool {
    prepare(r, path).is_some_and(|p| r.app_trees.iter().any(|t| inside(&p[1..], t)))
}

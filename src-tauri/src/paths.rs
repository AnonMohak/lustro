//! Path text rules of each platform: the separator and whether case matters.

/// The path separator: `\` on Windows, `/` on Linux, macOS and Android.
#[cfg(windows)]
pub const SEP: char = '\\';
#[cfg(not(windows))]
pub const SEP: char = '/';

/// True where the file system ignores case: Windows, and macOS (APFS by default).
pub const IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));

/// True when two paths or names are the same file system entry by text.
pub fn same(a: &str, b: &str) -> bool {
    if IGNORE_CASE {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// A form of `path` for map keys and prefix checks: lower case where case does not matter.
pub fn key(path: &str) -> String {
    if IGNORE_CASE {
        path.to_lowercase()
    } else {
        path.to_string()
    }
}

/// True when `path` is `folder` or lies inside it (a whole folder name must match).
pub fn inside(path: &str, folder: &str) -> bool {
    let (p, f) = (key(path), key(folder.trim_end_matches(SEP)));
    p == f
        || p.strip_prefix(f.as_str())
            .is_some_and(|r| r.starts_with(SEP))
}

/// True when `path` is in one of the folders of the ignore list (Settings).
pub fn ignored(list: &[String], path: &str) -> bool {
    list.iter().any(|f| inside(path, f))
}

/// Writes a Windows test path in the form of this platform. On other platforms, `C:\` becomes
/// `/` and other drives become a folder (`D:\x` becomes `/d/x`), so drives stay different.
#[cfg(test)]
pub fn os(path: &str) -> String {
    if cfg!(windows) {
        return path.to_string();
    }
    let b = path.as_bytes();
    let rest = if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        let drive = path[..1].to_ascii_lowercase();
        let rest = &path[2..];
        if drive == "c" {
            rest.to_string()
        } else {
            format!("/{drive}{rest}")
        }
    } else {
        path.to_string()
    };
    let p = rest.replace('\\', "/");
    if p.is_empty() { "/".into() } else { p }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_needs_a_whole_folder_name() {
        let (photos, file) = (os(r"D:\Photos"), os(r"D:\Photos\a.jpg"));
        assert!(inside(&file, &photos));
        assert!(inside(&photos, &photos));
        assert!(inside(&file, &format!("{photos}{SEP}")));
        assert!(!inside(&os(r"D:\Photos2\a.jpg"), &photos));
        assert!(ignored(&[os(r"E:\x"), photos.clone()], &file));
        assert!(!ignored(&[], &file));
    }

    #[test]
    fn test_paths_follow_the_platform() {
        if cfg!(windows) {
            assert_eq!(os("C:\\Users\\me"), "C:\\Users\\me");
        } else {
            assert_eq!(os("C:\\"), "/");
            assert_eq!(os("C:"), "/");
            assert_eq!(os("C:\\Users\\me"), "/Users/me");
            assert_eq!(os("D:\\Photos\\"), "/d/Photos/");
        }
    }

    #[test]
    fn case_rules() {
        assert!(same("Users", "Users"));
        assert_eq!(same("Users", "users"), IGNORE_CASE);
        assert_eq!(key("ABC") == "abc", IGNORE_CASE);
    }
}

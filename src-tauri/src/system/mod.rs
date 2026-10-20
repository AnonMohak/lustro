pub mod drives;
pub mod elevation;

/// Converts a Rust string to a null-terminated UTF-16 buffer for Win32 calls.
#[cfg(windows)]
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Converts a null-terminated UTF-16 buffer to a Rust string.
#[cfg(windows)]
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub const ATTR_READONLY: u32 = 0x1;
pub const ATTR_DIRECTORY: u32 = 0x10;
pub const ATTR_REPARSE_POINT: u32 = 0x400;

/// The Windows file attributes of `meta`.
#[cfg(windows)]
pub fn file_attributes(meta: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

/// The same bits as on Windows for folders and symbolic links, so callers need no `cfg`.
/// A symbolic link is never a folder here: `remove_file` removes it.
/// Read-only stays off: on Unix it does not stop a delete.
#[cfg(not(windows))]
pub fn file_attributes(meta: &std::fs::Metadata) -> u32 {
    let t = meta.file_type();
    if t.is_symlink() {
        ATTR_REPARSE_POINT
    } else if t.is_dir() {
        ATTR_DIRECTORY
    } else {
        0
    }
}

/// Stops a console program from opening a window on Windows.
pub fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn wide_round_trip() {
        let w = wide("C:\\Users\\Ünïcode");
        assert_eq!(*w.last().unwrap(), 0);
        assert_eq!(from_wide(&w), "C:\\Users\\Ünïcode");
    }

    #[test]
    fn from_wide_without_terminator() {
        let w: Vec<u16> = "NTFS".encode_utf16().collect();
        assert_eq!(from_wide(&w), "NTFS");
    }
}

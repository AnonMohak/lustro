//! macOS drives: the mounted local volumes from `getmntinfo`.
//!
//! APFS volumes of one container share its space, so `statfs` gives the container's
//! size and free space. The system volume (`/`) stands for the Data volume too: the
//! Data volume and the VM, Preboot and Update volumes are "nobrowse" and do not show.

use std::ffi::CStr;

use super::{DriveInfo, DriveKind};

/// Flags from `<sys/mount.h>`.
const MNT_RDONLY: u32 = 0x0000_0001;
const MNT_REMOVABLE: u32 = 0x0000_0200;
const MNT_LOCAL: u32 = 0x0000_1000;
const MNT_DONTBROWSE: u32 = 0x0010_0000;

/// File systems that hold no user files.
const SKIPPED_TYPES: &[&str] = &["autofs", "devfs", "nullfs", "lifs"];

#[derive(Debug, PartialEq)]
struct Mount {
    point: String,
    fs_type: String,
    flags: u32,
    total: u64,
    free: u64,
}

/// True for volumes that the user sees in the Finder: local, not hidden, and not a
/// read-only image (a mounted `.dmg` installer is read-only).
fn is_drive(m: &Mount) -> bool {
    m.flags & MNT_LOCAL != 0
        && m.flags & MNT_DONTBROWSE == 0
        && !SKIPPED_TYPES.contains(&m.fs_type.as_str())
        && (m.point == "/" || m.flags & MNT_RDONLY == 0)
        && !m.point.starts_with("/System/Volumes/")
}

fn kind_of(m: &Mount) -> DriveKind {
    if m.flags & MNT_REMOVABLE != 0 {
        DriveKind::Removable
    } else if m.point == "/" {
        // Every Mac with Apple silicon has an internal SSD.
        DriveKind::Ssd
    } else {
        DriveKind::Unknown
    }
}

fn text(chars: &[libc::c_char]) -> String {
    // SAFETY: the kernel ends each name with a NUL inside the array.
    unsafe { CStr::from_ptr(chars.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn mounts() -> Vec<Mount> {
    let mut buf: *mut libc::statfs = std::ptr::null_mut();
    // SAFETY: getmntinfo points `buf` at a static array of `n` entries that stays valid
    // until the next call on this thread.
    let n = unsafe { libc::getmntinfo(&mut buf, libc::MNT_NOWAIT) };
    if n <= 0 || buf.is_null() {
        return Vec::new();
    }
    let all = unsafe { std::slice::from_raw_parts(buf, n as usize) };
    all.iter()
        .map(|s| {
            let unit = s.f_bsize as u64;
            Mount {
                point: text(&s.f_mntonname),
                fs_type: text(&s.f_fstypename),
                flags: s.f_flags,
                total: s.f_blocks * unit,
                free: s.f_bavail * unit,
            }
        })
        .collect()
}

pub fn list_drives() -> Vec<DriveInfo> {
    mounts()
        .into_iter()
        .filter(is_drive)
        .map(|m| {
            let label = match m.point.as_str() {
                "/" => String::new(),
                p => p.rsplit('/').next().unwrap_or(p).to_string(),
            };
            DriveInfo {
                letter: m.point.clone(),
                root: crate::state::normalize_root(&m.point),
                label,
                kind: kind_of(&m),
                file_system: m.fs_type,
                total_bytes: m.total,
                free_bytes: m.free,
                supports_fast_scan: false,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(point: &str, fs_type: &str, flags: u32) -> Mount {
        Mount {
            point: point.into(),
            fs_type: fs_type.into(),
            flags,
            total: 1,
            free: 1,
        }
    }

    #[test]
    fn keeps_volumes_that_the_finder_shows() {
        let all = [
            mount("/", "apfs", MNT_LOCAL | MNT_RDONLY),
            mount("/dev", "devfs", MNT_LOCAL | MNT_DONTBROWSE),
            mount("/System/Volumes/Data", "apfs", MNT_LOCAL | MNT_DONTBROWSE),
            mount("/System/Volumes/VM", "apfs", MNT_LOCAL | MNT_DONTBROWSE),
            mount("/System/Volumes/Data/home", "autofs", MNT_DONTBROWSE),
            mount("/Volumes/USB", "exfat", MNT_LOCAL | MNT_REMOVABLE),
            mount("/Volumes/Backup", "apfs", MNT_LOCAL),
            mount("/Volumes/Installer", "hfs", MNT_LOCAL | MNT_RDONLY),
            mount("/Volumes/share", "smbfs", 0),
        ];
        let points: Vec<&str> = all
            .iter()
            .filter(|m| is_drive(m))
            .map(|m| m.point.as_str())
            .collect();
        assert_eq!(points, ["/", "/Volumes/USB", "/Volumes/Backup"]);
        assert_eq!(kind_of(&all[0]), DriveKind::Ssd);
        assert_eq!(kind_of(&all[5]), DriveKind::Removable);
        assert_eq!(kind_of(&all[6]), DriveKind::Unknown);
    }

    #[test]
    fn lists_the_system_volume() {
        let drives = list_drives();
        let root = drives
            .iter()
            .find(|d| d.letter == "/")
            .expect("/ is listed");
        assert!(root.total_bytes > 0 && root.free_bytes <= root.total_bytes);
    }
}

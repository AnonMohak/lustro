//! Linux drives: the mounted block devices from `/proc/self/mountinfo`.

use std::collections::HashSet;
use std::ffi::CString;

use super::{DriveInfo, DriveKind};

/// One line of `/proc/self/mountinfo`.
#[derive(Debug, PartialEq)]
struct Mount {
    /// `major:minor` of the device.
    dev: String,
    /// The folder of the file system that is mounted (not `/` for bind mounts and btrfs subvolumes).
    root: String,
    point: String,
    fs_type: String,
    source: String,
}

/// Undoes the octal escapes of mountinfo: `\040` is a space.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 4], 8)
        {
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            // id parent major:minor root point options [optional...] - type source super-options
            let (head, tail) = line.split_once(" - ")?;
            let h: Vec<&str> = head.split(' ').collect();
            let t: Vec<&str> = tail.split(' ').collect();
            Some(Mount {
                dev: h.get(2)?.to_string(),
                root: unescape(h.get(3)?),
                point: unescape(h.get(4)?),
                fs_type: t.first()?.to_string(),
                source: unescape(t.get(1)?),
            })
        })
        .collect()
}

/// True for file systems on a disk that the user can scan. Network, memory, kernel,
/// loop (snap) and container mounts are skipped, like network drives on Windows.
fn is_drive(m: &Mount) -> bool {
    let on_disk = m.source.starts_with("/dev/") && !m.source.starts_with("/dev/loop");
    let zfs = m.fs_type == "zfs";
    let skipped_place = ["/snap/", "/var/lib/docker/", "/var/snap/"]
        .iter()
        .any(|p| m.point.starts_with(p))
        || m.point.starts_with("/proc")
        || m.point.starts_with("/sys");
    (on_disk || zfs) && m.fs_type != "squashfs" && !skipped_place
}

/// The drives in mount order. A device that is mounted twice at the same folder
/// (a bind mount) shows once.
fn drives_of(mounts: &[Mount]) -> Vec<&Mount> {
    let mut seen = HashSet::new();
    mounts
        .iter()
        .filter(|m| is_drive(m) && seen.insert((m.dev.clone(), m.root.clone())))
        .collect()
}

/// SSD, HDD or removable, from `/sys/dev/block/<major:minor>`. A partition has no
/// `queue` folder, so its disk (the parent folder) answers.
fn kind_of(dev: &str) -> DriveKind {
    let base = format!("/sys/dev/block/{dev}");
    let read = |name: &str| {
        std::fs::read_to_string(format!("{base}/{name}"))
            .or_else(|_| std::fs::read_to_string(format!("{base}/../{name}")))
            .map(|s| s.trim().to_string())
            .ok()
    };
    if read("removable").as_deref() == Some("1") {
        return DriveKind::Removable;
    }
    match read("queue/rotational").as_deref() {
        Some("0") => DriveKind::Ssd,
        Some("1") => DriveKind::Hdd,
        _ => DriveKind::Unknown,
    }
}

/// Total and free bytes. Free is what a normal user can use (without the root reserve).
fn space(point: &str) -> Option<(u64, u64)> {
    let c = CString::new(point).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let unit = s.f_frsize as u64;
    Some((s.f_blocks as u64 * unit, s.f_bavail as u64 * unit))
}

pub fn list_drives() -> Vec<DriveInfo> {
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return Vec::new();
    };
    let mounts = parse(&text);
    drives_of(&mounts)
        .into_iter()
        .filter_map(|m| {
            let (total, free) = space(&m.point)?;
            let label = match m.point.as_str() {
                "/" => String::new(),
                p => p.rsplit('/').next().unwrap_or(p).to_string(),
            };
            Some(DriveInfo {
                letter: m.point.clone(),
                root: crate::state::normalize_root(&m.point),
                label,
                file_system: m.fs_type.clone(),
                kind: kind_of(&m.dev),
                total_bytes: total,
                free_bytes: free,
                supports_fast_scan: false,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
22 1 259:2 / / rw,relatime shared:1 - ext4 /dev/nvme0n1p2 rw
23 22 0:5 / /proc rw,nosuid shared:12 - proc proc rw
24 22 0:21 / /sys rw,nosuid shared:2 - sysfs sysfs rw
25 22 0:24 / /run rw,nosuid shared:5 - tmpfs tmpfs rw,size=1600m
26 22 259:1 / /boot/efi rw,relatime shared:3 - vfat /dev/nvme0n1p1 rw
27 22 7:0 / /snap/core22/1380 ro,nodev shared:4 - squashfs /dev/loop0 ro
28 22 8:17 / /media/me/My\\040Disk rw,nosuid shared:6 - exfat /dev/sdb1 rw
29 22 0:40 / /mnt/nas rw shared:7 - nfs4 nas:/share rw
30 22 259:2 /srv /srv rw,relatime shared:1 - ext4 /dev/nvme0n1p2 rw
31 22 259:2 /srv /srv rw,relatime shared:1 - ext4 /dev/nvme0n1p2 rw
32 22 0:50 / /tank rw shared:8 - zfs tank rw
";

    #[test]
    fn parses_mountinfo_lines() {
        let m = parse(SAMPLE);
        assert_eq!(m.len(), 11);
        assert_eq!(
            m[6],
            Mount {
                dev: "8:17".into(),
                root: "/".into(),
                point: "/media/me/My Disk".into(),
                fs_type: "exfat".into(),
                source: "/dev/sdb1".into(),
            }
        );
    }

    #[test]
    fn keeps_disks_only() {
        let mounts = parse(SAMPLE);
        let points: Vec<&str> = drives_of(&mounts)
            .iter()
            .map(|m| m.point.as_str())
            .collect();
        assert_eq!(
            points,
            ["/", "/boot/efi", "/media/me/My Disk", "/srv", "/tank"]
        );
    }

    #[test]
    fn unescapes_octal() {
        assert_eq!(unescape("a\\040b\\011c"), "a b\tc");
        assert_eq!(unescape("plain"), "plain");
        assert_eq!(unescape("end\\04"), "end\\04");
    }

    #[test]
    fn lists_the_root_file_system() {
        let drives = list_drives();
        let root = drives.iter().find(|d| d.letter == "/");
        // Containers can mount `/` from an overlay; then no disk is listed for it.
        if let Some(r) = root {
            assert_eq!(r.root, "/");
            assert!(r.total_bytes > 0 && r.free_bytes <= r.total_bytes);
        }
    }
}

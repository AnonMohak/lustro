//! Live updates from the NTFS change journal (USN journal). Needs admin rights.

use std::fs::File;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;

use windows::Win32::Foundation::{
    ERROR_JOURNAL_DELETE_IN_PROGRESS, ERROR_JOURNAL_ENTRY_DELETED, ERROR_JOURNAL_NOT_ACTIVE, HANDLE,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0, USN_JOURNAL_DATA_V0,
};

use super::model::{NONE, Tree, flags};

const REASON_DATA: u32 = 0x1 | 0x2 | 0x4;
const REASON_DELETE: u32 = 0x200;
const REASON_BASIC_INFO: u32 = 0x8000;
const REASON_HARD_LINK: u32 = 0x10000;
const ATTR_HIDDEN: u32 = 0x2;
const ATTR_SYSTEM: u32 = 0x4;
const ATTR_DIRECTORY: u32 = 0x10;
const RECORD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// The final state of one file after it was closed.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub record: u64,
    pub parent: u64,
    pub reason: u32,
    pub attrs: u32,
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum UsnError {
    #[error("The change journal was reset; a full scan is needed.")]
    Reset,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Windows(#[from] windows::core::Error),
}

pub fn open_volume(letter: char) -> std::io::Result<File> {
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(format!("\\\\.\\{letter}:"))
}

/// Journal id and the next USN of a volume.
pub fn query(vol: &File) -> Option<(u64, i64)> {
    let mut data = USN_JOURNAL_DATA_V0::default();
    let mut returned = 0u32;
    unsafe {
        DeviceIoControl(
            HANDLE(vol.as_raw_handle()),
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(&mut data as *mut _ as *mut _),
            size_of::<USN_JOURNAL_DATA_V0>() as u32,
            Some(&mut returned),
            None,
        )
    }
    .ok()?;
    Some((data.UsnJournalID, data.NextUsn))
}

/// Parses the output of FSCTL_READ_USN_JOURNAL: the next USN, then USN_RECORD_V2 entries.
pub fn parse(buf: &[u8]) -> (i64, Vec<Change>) {
    if buf.len() < 8 {
        return (0, Vec::new());
    }
    let next = i64::from_le_bytes(buf[0..8].try_into().unwrap());
    let mut out = Vec::new();
    let mut off = 8;
    while off + 60 <= buf.len() {
        let len = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        if len < 60 || off + len > buf.len() {
            break;
        }
        let r = &buf[off..off + len];
        let major = u16::from_le_bytes([r[4], r[5]]);
        if major == 2 {
            let u64_at = |o: usize| u64::from_le_bytes(r[o..o + 8].try_into().unwrap());
            let u32_at = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
            let name_len = u16::from_le_bytes([r[56], r[57]]) as usize;
            let name_off = u16::from_le_bytes([r[58], r[59]]) as usize;
            if name_off + name_len <= r.len() {
                let units: Vec<u16> = r[name_off..name_off + name_len]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                out.push(Change {
                    record: u64_at(8) & RECORD_MASK,
                    parent: u64_at(16) & RECORD_MASK,
                    reason: u32_at(40),
                    attrs: u32_at(52),
                    name: String::from_utf16_lossy(&units),
                });
            }
        }
        off += (len + 7) & !7;
    }
    (next, out)
}

/// Reads the close records since `start`. Returns the changes and the USN to read next.
pub fn read(vol: &File, journal: u64, start: i64) -> Result<(Vec<Change>, i64), UsnError> {
    let mut all = Vec::new();
    let mut usn = start;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let req = READ_USN_JOURNAL_DATA_V0 {
            StartUsn: usn,
            ReasonMask: 0xFFFF_FFFF,
            ReturnOnlyOnClose: 1,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: journal,
        };
        let mut returned = 0u32;
        let r = unsafe {
            DeviceIoControl(
                HANDLE(vol.as_raw_handle()),
                FSCTL_READ_USN_JOURNAL,
                Some(&req as *const _ as *const _),
                size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
                Some(buf.as_mut_ptr() as *mut _),
                buf.len() as u32,
                Some(&mut returned),
                None,
            )
        };
        if let Err(e) = r {
            let c = e.code();
            if c == ERROR_JOURNAL_ENTRY_DELETED.to_hresult()
                || c == ERROR_JOURNAL_NOT_ACTIVE.to_hresult()
                || c == ERROR_JOURNAL_DELETE_IN_PROGRESS.to_hresult()
            {
                return Err(UsnError::Reset);
            }
            return Err(e.into());
        }
        let (next, changes) = parse(&buf[..returned as usize]);
        let done = changes.is_empty() || next == usn;
        all.extend(changes);
        usn = next;
        if done {
            break;
        }
    }
    Ok((all, usn))
}

/// File facts for `apply`: size, size on disk, FILETIME, folder or not.
pub type Stat = (u64, u64, u64, bool);

pub fn stat_path(path: &str) -> Option<Stat> {
    let m = std::fs::symlink_metadata(path).ok()?;
    let dir = m.file_attributes() & ATTR_DIRECTORY != 0;
    let size = if dir { 0 } else { m.len() };
    Some((size, size.div_ceil(4096) * 4096, m.last_write_time(), dir))
}

fn record_node(t: &Tree, record: u64) -> u32 {
    let n = t.frn.get(record as usize).copied().unwrap_or(NONE);
    // A removed node keeps no parent; treat it as unknown.
    if n == NONE || (n != Tree::ROOT && t.node(n).parent == NONE) {
        NONE
    } else {
        n
    }
}

/// Applies changes to a tree. Returns how many nodes changed.
pub fn apply(t: &mut Tree, changes: &[Change], stat: &dyn Fn(&str) -> Option<Stat>) -> usize {
    let mut changed = 0;
    for c in changes {
        let node = record_node(t, c.record);
        if c.reason & REASON_DELETE != 0 {
            if node != NONE && node != Tree::ROOT && t.remove(node) {
                changed += 1;
            }
            if let Some(slot) = t.frn.get_mut(c.record as usize) {
                *slot = NONE;
            }
            continue;
        }
        let parent = record_node(t, c.parent);
        if parent == NONE || !t.node(parent).is_dir() {
            continue; // outside the scanned tree (for example $Extend)
        }
        let path = format!("{}\\{}", t.path(parent).trim_end_matches('\\'), c.name);
        if node != NONE {
            let n = *t.node(node);
            let moved = n.parent != parent || t.name(node) != c.name;
            if moved && c.reason & REASON_HARD_LINK == 0 && t.move_node(node, parent, &c.name) {
                changed += 1;
            }
            if !n.is_dir()
                && c.reason & (REASON_DATA | REASON_BASIC_INFO) != 0
                && let Some((size, alloc, modified, _)) = stat(&path)
            {
                t.set_file_size(node, size, alloc, modified);
                changed += 1;
            }
        } else if let Some((size, alloc, modified, dir)) = stat(&path) {
            let mut f = if dir { flags::DIR } else { 0 };
            if c.attrs & ATTR_HIDDEN != 0 {
                f |= flags::HIDDEN;
            }
            if c.attrs & ATTR_SYSTEM != 0 {
                f |= flags::SYSTEM;
            }
            let id = t.add_child(parent, &c.name, f, size, alloc, modified);
            let rec = c.record as usize;
            if rec >= t.frn.len() {
                t.frn.resize(rec + 1, NONE);
            }
            t.frn[rec] = id;
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::model::tests::sample;

    fn record(rec: u64, parent: u64, reason: u32, attrs: u32, name: &str) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        let len = (60 + units.len() * 2 + 7) & !7;
        let mut r = vec![0u8; len];
        r[0..4].copy_from_slice(&(len as u32).to_le_bytes());
        r[4..6].copy_from_slice(&2u16.to_le_bytes());
        r[8..16].copy_from_slice(&(rec | (3 << 48)).to_le_bytes());
        r[16..24].copy_from_slice(&(parent | (1 << 48)).to_le_bytes());
        r[40..44].copy_from_slice(&reason.to_le_bytes());
        r[52..56].copy_from_slice(&attrs.to_le_bytes());
        r[56..58].copy_from_slice(&((units.len() * 2) as u16).to_le_bytes());
        r[58..60].copy_from_slice(&60u16.to_le_bytes());
        for (i, u) in units.iter().enumerate() {
            r[60 + i * 2..62 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        r
    }

    /// Reads the real journal: `--ignored usn_sees_new_file` in an elevated terminal.
    #[test]
    #[ignore]
    fn usn_sees_new_file() {
        let dir = std::env::temp_dir();
        let letter = dir.to_string_lossy().chars().next().unwrap();
        let vol = open_volume(letter).expect("open volume (run as admin)");
        let (journal, start) = query(&vol).expect("journal");
        let name = format!("lustro-usn-{}.txt", std::process::id());
        std::fs::write(dir.join(&name), b"x").unwrap();
        std::fs::remove_file(dir.join(&name)).unwrap();
        let (changes, next) = read(&vol, journal, start).unwrap();
        assert!(next >= start);
        let mine: Vec<&Change> = changes.iter().filter(|c| c.name == name).collect();
        println!(
            "{} changes, {} for the test file: {:?}",
            changes.len(),
            mine.len(),
            mine
        );
        assert!(mine.iter().any(|c| c.reason & REASON_DELETE != 0));
    }

    #[test]
    fn parses_records() {
        let mut buf = 777i64.to_le_bytes().to_vec();
        buf.extend(record(42, 5, 0x8000_0100, 0x20, "new.txt"));
        buf.extend(record(43, 42, 0x8000_0200, 0x10, "gone"));
        let (next, changes) = parse(&buf);
        assert_eq!(next, 777);
        assert_eq!(changes.len(), 2);
        assert_eq!(
            changes[0],
            Change {
                record: 42,
                parent: 5,
                reason: 0x8000_0100,
                attrs: 0x20,
                name: "new.txt".into()
            }
        );
        assert_eq!(changes[1].name, "gone");
    }

    /// The sample tree with a record map: 5 = root, 10 = Users, 11 = me, 12 = a.mp4, 20 = Windows.
    fn mapped() -> Tree {
        let mut t = sample();
        let mut frn = vec![NONE; 64];
        frn[5] = Tree::ROOT;
        frn[10] = t.find_path("C:\\Users").unwrap();
        frn[11] = t.find_path("C:\\Users\\me").unwrap();
        frn[12] = t.find_path("C:\\Users\\me\\a.mp4").unwrap();
        frn[20] = t.find_path("C:\\Windows").unwrap();
        t.frn = frn;
        t
    }

    #[test]
    fn applies_create_resize_move_and_delete() {
        let mut t = mapped();
        let stat = |p: &str| -> Option<Stat> {
            match p {
                "C:\\Users\\me\\new.iso" => Some((5000, 8192, 9, false)),
                "C:\\Users\\me\\a.mp4" => Some((150, 4096, 9, false)),
                "C:\\Windows\\a-renamed.mp4" => Some((150, 4096, 9, false)),
                _ => None,
            }
        };
        let c = |rec, parent, reason, name: &str| Change {
            record: rec,
            parent,
            reason,
            attrs: 0x20,
            name: name.into(),
        };

        assert_eq!(
            apply(&mut t, &[c(30, 11, 0x8000_0100, "new.iso")], &stat),
            1
        );
        assert_eq!(t.total_size(), 5200);
        assert_eq!(t.path(t.frn[30]), "C:\\Users\\me\\new.iso");

        apply(&mut t, &[c(12, 11, 0x8000_0002, "a.mp4")], &stat);
        assert_eq!(t.total_size(), 5250);

        apply(&mut t, &[c(12, 20, 0x8000_2000, "a-renamed.mp4")], &stat);
        assert_eq!(t.path(t.frn[12]), "C:\\Windows\\a-renamed.mp4");
        assert_eq!(t.node(t.frn[20]).size, 200);

        apply(&mut t, &[c(30, 11, 0x8000_0200, "new.iso")], &stat);
        assert_eq!(t.total_size(), 250);
        assert_eq!(t.frn[30], NONE);

        // Changes outside the tree are ignored.
        assert_eq!(apply(&mut t, &[c(50, 63, 0x8000_0100, "x")], &stat), 0);
    }
}

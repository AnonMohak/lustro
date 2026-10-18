//! Fast scan: reads the NTFS Master File Table straight from the volume.
//! Needs admin rights. The caller falls back to the folder walk on any error.

use std::fs::File;
use std::os::windows::fs::{FileExt, OpenOptionsExt};
use std::sync::atomic::Ordering;
use std::time::Instant;

use rayon::prelude::*;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{FSCTL_GET_NTFS_VOLUME_DATA, NTFS_VOLUME_DATA_BUFFER};

use super::model::{NONE, ScanMethod, Tree, TreeBuilder, flags};
use super::{Progress, ScanError};

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_END: u32 = 0xFFFF_FFFF;

const RECORD_IN_USE: u16 = 0x01;
const RECORD_IS_DIR: u16 = 0x02;

const NAMESPACE_DOS: u8 = 2;
const ROOT_RECORD: u64 = 5;

const FILE_ATTR_HIDDEN: u32 = 0x2;
const FILE_ATTR_SYSTEM: u32 = 0x4;
const FILE_ATTR_REPARSE: u32 = 0x400;
const FILE_ATTR_OFFLINE: u32 = 0x1000;
const FILE_ATTR_RECALL_ON_OPEN: u32 = 0x40000;
const FILE_ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x400000;

const CHUNK: usize = 16 * 1024 * 1024;

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// Applies the update sequence array ("fixup") to a FILE record.
/// Returns false when the record is damaged or not a FILE record.
pub fn apply_fixup(rec: &mut [u8], sector: usize) -> bool {
    if rec.len() < 48 || &rec[0..4] != b"FILE" {
        return false;
    }
    let usa_off = u16_at(rec, 4) as usize;
    let usa_count = u16_at(rec, 6) as usize;
    if usa_count == 0 || usa_off + usa_count * 2 > rec.len() || (usa_count - 1) * sector > rec.len()
    {
        return false;
    }
    let usn = [rec[usa_off], rec[usa_off + 1]];
    for i in 1..usa_count {
        let end = i * sector - 2;
        if rec[end..end + 2] != usn {
            return false;
        }
        rec[end] = rec[usa_off + i * 2];
        rec[end + 1] = rec[usa_off + i * 2 + 1];
    }
    true
}

/// One data run: a length in clusters and an absolute LCN (None for a sparse run).
#[derive(Debug, PartialEq, Eq)]
pub struct Run {
    pub lcn: Option<i64>,
    pub clusters: u64,
}

/// Decodes an NTFS runlist (mapping pairs).
pub fn decode_runs(b: &[u8]) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut i = 0;
    let mut lcn: i64 = 0;
    while i < b.len() && b[i] != 0 {
        let len_size = (b[i] & 0x0F) as usize;
        let off_size = (b[i] >> 4) as usize;
        i += 1;
        if len_size == 0 || len_size > 8 || off_size > 8 || i + len_size + off_size > b.len() {
            break;
        }
        let mut clusters = 0u64;
        for k in 0..len_size {
            clusters |= (b[i + k] as u64) << (8 * k);
        }
        i += len_size;
        if off_size == 0 {
            runs.push(Run {
                lcn: None,
                clusters,
            });
            continue;
        }
        let mut delta = 0i64;
        for k in 0..off_size {
            delta |= (b[i + k] as i64) << (8 * k);
        }
        // Sign-extend.
        let shift = 64 - 8 * off_size as u32;
        delta = (delta << shift) >> shift;
        i += off_size;
        lcn += delta;
        runs.push(Run {
            lcn: Some(lcn),
            clusters,
        });
    }
    runs
}

/// What one FILE record says about a file.
#[derive(Debug, Default, Clone)]
pub struct Parsed {
    pub record: u64,
    /// Record number of the base record, or 0 when this is a base record.
    pub base: u64,
    pub is_dir: bool,
    pub parent: u64,
    pub name: Option<String>,
    /// Size and allocated size from the unnamed $DATA attribute (first segment only).
    pub data: Option<(u64, u64)>,
    pub modified: u64,
    pub attrs: u32,
    pub has_si: bool,
    /// Clusters used by named data streams (bytes).
    pub stream_alloc: u64,
}

/// Parses one FILE record after the fixup. Returns None for unused records.
pub fn parse_record(rec: &[u8], record: u64) -> Option<Parsed> {
    let rflags = u16_at(rec, 0x16);
    if rflags & RECORD_IN_USE == 0 {
        return None;
    }
    let mut p = Parsed {
        record,
        base: u64_at(rec, 0x20) & 0x0000_FFFF_FFFF_FFFF,
        is_dir: rflags & RECORD_IS_DIR != 0,
        ..Default::default()
    };
    let used = (u32_at(rec, 0x18) as usize).min(rec.len());
    let mut off = u16_at(rec, 0x14) as usize;
    let mut best_ns: Option<u8> = None;

    while off + 16 <= used {
        let atype = u32_at(rec, off);
        if atype == ATTR_END {
            break;
        }
        let alen = u32_at(rec, off + 4) as usize;
        if alen < 16 || off + alen > used {
            break;
        }
        let a = &rec[off..off + alen];
        let non_resident = a[8] != 0;
        let name_len = a[9];

        match atype {
            ATTR_STANDARD_INFORMATION if !non_resident => {
                let voff = u16_at(a, 0x14) as usize;
                if voff + 0x24 <= a.len() {
                    p.modified = u64_at(a, voff + 0x08);
                    p.attrs = u32_at(a, voff + 0x20);
                    p.has_si = true;
                }
            }
            ATTR_FILE_NAME if !non_resident => {
                let voff = u16_at(a, 0x14) as usize;
                if voff + 0x42 <= a.len() {
                    let v = &a[voff..];
                    let ns = v[0x41];
                    let nlen = v[0x40] as usize;
                    // Skip short 8.3 names when a long name exists.
                    let better = match best_ns {
                        None => true,
                        Some(prev) => prev == NAMESPACE_DOS && ns != NAMESPACE_DOS,
                    };
                    if better && 0x42 + nlen * 2 <= v.len() {
                        let units: Vec<u16> = (0..nlen).map(|k| u16_at(v, 0x42 + k * 2)).collect();
                        p.name = Some(String::from_utf16_lossy(&units));
                        p.parent = u64_at(v, 0) & 0x0000_FFFF_FFFF_FFFF;
                        best_ns = Some(ns);
                    }
                }
            }
            ATTR_DATA if name_len == 0 => {
                if non_resident {
                    if a.len() >= 0x40 && u64_at(a, 0x10) == 0 {
                        let allocated = u64_at(a, 0x28);
                        let size = u64_at(a, 0x30);
                        let compressed = u16_at(a, 0x0C) & 0x8001 != 0
                            && u16_at(a, 0x22) != 0
                            && a.len() >= 0x48;
                        let on_disk = if compressed {
                            u64_at(a, 0x40)
                        } else {
                            allocated
                        };
                        p.data = Some((size, on_disk));
                    }
                } else {
                    // Resident data lives inside the MFT record and uses no clusters.
                    p.data = Some((u32_at(a, 0x10) as u64, 0));
                }
            }
            // Named streams. WOF-compressed system files keep their real data in
            // "WofCompressedData", so the clusters of every named stream count as size on disk.
            ATTR_DATA if non_resident && a.len() >= 0x40 && u64_at(a, 0x10) == 0 => {
                p.stream_alloc += u64_at(a, 0x28);
            }
            _ => {}
        }
        off += alen;
    }
    Some(p)
}

struct Volume {
    file: File,
    cluster: u64,
    record_size: usize,
    sector: usize,
    mft_start: u64,
    mft_valid: u64,
}

fn open_volume(letter: char) -> Result<Volume, ScanError> {
    let path = format!("\\\\.\\{letter}:");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2) // FILE_SHARE_READ | FILE_SHARE_WRITE
        .open(&path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                ScanError::NeedsAdmin
            } else {
                ScanError::Io(e)
            }
        })?;
    let mut data = NTFS_VOLUME_DATA_BUFFER::default();
    let mut returned = 0u32;
    unsafe {
        use std::os::windows::io::AsRawHandle;
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            FSCTL_GET_NTFS_VOLUME_DATA,
            None,
            0,
            Some(&mut data as *mut _ as *mut _),
            size_of::<NTFS_VOLUME_DATA_BUFFER>() as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(|_| ScanError::NotNtfs)?;
    Ok(Volume {
        file,
        cluster: data.BytesPerCluster as u64,
        record_size: data.BytesPerFileRecordSegment as usize,
        sector: data.BytesPerSector as usize,
        mft_start: data.MftStartLcn as u64,
        mft_valid: data.MftValidDataLength as u64,
    })
}

/// Extents of the $MFT file, from the $DATA attribute in record 0.
fn mft_extents(vol: &Volume) -> Result<Vec<(u64, u64)>, ScanError> {
    let read = vol.record_size.max(vol.sector).next_multiple_of(vol.sector);
    let mut rec = vec![0u8; read];
    vol.file.seek_read(&mut rec, vol.mft_start * vol.cluster)?;
    let rec = &mut rec[..vol.record_size];
    if !apply_fixup(rec, vol.sector) {
        return Err(ScanError::Corrupt("record 0"));
    }
    let used = (u32_at(rec, 0x18) as usize).min(rec.len());
    let mut off = u16_at(rec, 0x14) as usize;
    while off + 16 <= used {
        let atype = u32_at(rec, off);
        let alen = u32_at(rec, off + 4) as usize;
        if atype == ATTR_END || alen < 16 || off + alen > used {
            break;
        }
        let a = &rec[off..off + alen];
        if atype == ATTR_DATA && a[8] != 0 && a[9] == 0 {
            let runs_off = u16_at(a, 0x20) as usize;
            let runs = decode_runs(&a[runs_off..]);
            let mut extents = Vec::new();
            for r in runs {
                let lcn = r.lcn.ok_or(ScanError::Corrupt("sparse $MFT"))?;
                extents.push((lcn as u64 * vol.cluster, r.clusters * vol.cluster));
            }
            return Ok(extents);
        }
        off += alen;
    }
    Err(ScanError::Corrupt("no $MFT data"))
}

#[derive(Clone, Copy, Default)]
struct Rec {
    parent: u64,
    stream_alloc: u64,
    size: u64,
    alloc: u64,
    modified: u64,
    attrs: u32,
    name_off: u32,
    name_len: u16,
    in_use: bool,
    is_dir: bool,
    has_name: bool,
}

/// Scans a whole NTFS volume. `letter` is the drive letter, for example 'C'.
pub fn scan_volume(letter: char, progress: &Progress) -> Result<Tree, ScanError> {
    let started = Instant::now();
    let vol = open_volume(letter)?;
    // The journal position before the read: live updates replay what changed during the scan.
    let usn = super::usn::query(&vol.file);
    let extents = mft_extents(&vol)?;
    let total_records = (vol.mft_valid / vol.record_size as u64) as usize;

    let mut recs: Vec<Rec> = vec![Rec::default(); total_records];
    let mut names = String::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut next_record: u64 = 0;
    let mut remaining = vol.mft_valid;

    'outer: for (start, len) in extents {
        let mut pos = 0u64;
        while pos < len && remaining > 0 {
            if progress.cancelled() {
                return Err(ScanError::Cancelled);
            }
            let want = (len - pos)
                .min(CHUNK as u64)
                .min(remaining.next_multiple_of(vol.sector as u64));
            let mut buf = vec![0u8; want as usize];
            vol.file.seek_read(&mut buf, start + pos)?;
            pos += want;
            remaining = remaining.saturating_sub(want);
            pending.extend_from_slice(&buf);

            let whole = pending.len() / vol.record_size * vol.record_size;
            let first = next_record;
            let parsed: Vec<Parsed> = pending[..whole]
                .par_chunks_exact_mut(vol.record_size)
                .enumerate()
                .filter_map(|(i, rec)| {
                    if !apply_fixup(rec, vol.sector) {
                        return None;
                    }
                    parse_record(rec, first + i as u64)
                })
                .collect();
            next_record += (whole / vol.record_size) as u64;
            pending.drain(..whole);

            for p in parsed {
                let target = if p.base != 0 { p.base } else { p.record } as usize;
                if target >= recs.len() {
                    continue;
                }
                let r = &mut recs[target];
                if p.base == 0 {
                    r.in_use = true;
                    r.is_dir = p.is_dir;
                    if p.has_si {
                        r.modified = p.modified;
                        r.attrs = p.attrs;
                    }
                }
                r.stream_alloc += p.stream_alloc;
                if let Some((size, alloc)) = p.data {
                    r.size = size;
                    r.alloc = alloc;
                }
                if let Some(name) = p.name
                    && !r.has_name
                {
                    r.parent = p.parent;
                    r.name_off = names.len() as u32;
                    r.name_len = name.len().min(u16::MAX as usize) as u16;
                    names.push_str(&name[..r.name_len as usize]);
                    r.has_name = true;
                    if p.is_dir {
                        progress.dirs.fetch_add(1, Ordering::Relaxed);
                    } else {
                        progress.files.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            progress
                .bytes
                .store(next_record * vol.record_size as u64, Ordering::Relaxed);
            if next_record as usize >= total_records {
                break 'outer;
            }
        }
    }

    // Build the tree. Record 5 is the root folder.
    let root_name = format!("{letter}:");
    let mut b = TreeBuilder::new(&root_name, total_records);
    let mut node_of: Vec<u32> = vec![NONE; recs.len()];
    if recs.len() <= ROOT_RECORD as usize {
        return Err(ScanError::Corrupt("tiny MFT"));
    }
    node_of[ROOT_RECORD as usize] = Tree::ROOT;
    for (i, r) in recs.iter().enumerate() {
        if !r.in_use || !r.has_name || i as u64 == ROOT_RECORD {
            continue;
        }
        let name = &names[r.name_off as usize..r.name_off as usize + r.name_len as usize];
        let mut f = if r.is_dir { flags::DIR } else { 0 };
        if r.attrs & FILE_ATTR_HIDDEN != 0 {
            f |= flags::HIDDEN;
        }
        if r.attrs & FILE_ATTR_SYSTEM != 0 {
            f |= flags::SYSTEM;
        }
        if r.attrs & FILE_ATTR_REPARSE != 0 && r.is_dir {
            f |= flags::LINK;
        }
        let online = r.attrs
            & (FILE_ATTR_OFFLINE | FILE_ATTR_RECALL_ON_OPEN | FILE_ATTR_RECALL_ON_DATA_ACCESS)
            != 0;
        if online {
            f |= flags::ONLINE_ONLY;
        }
        let (size, alloc) = if r.is_dir {
            (0, 0)
        } else {
            (r.size, if online { 0 } else { r.alloc + r.stream_alloc })
        };
        node_of[i] = b.push(name, f, size, alloc, r.modified);
    }
    for (i, r) in recs.iter().enumerate() {
        let id = node_of[i];
        if id == NONE || id == Tree::ROOT {
            continue;
        }
        let parent = r.parent as usize;
        if parent < node_of.len() && node_of[parent] != NONE && recs[parent].is_dir {
            b.link(id, node_of[parent]);
        }
    }
    drop(recs);
    drop(names);
    let mut tree = b.finish(
        format!("{letter}:\\"),
        ScanMethod::Mft,
        started.elapsed().as_millis() as u64,
        0,
    );
    tree.frn = node_of;
    tree.usn = usn;
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a 1024-byte FILE record with the given attributes and applies a "reverse fixup",
    /// so `apply_fixup` must restore it.
    fn record(attrs: &[Vec<u8>], in_use_flags: u16, base: u64) -> Vec<u8> {
        let mut r = vec![0u8; 1024];
        r[0..4].copy_from_slice(b"FILE");
        r[4..6].copy_from_slice(&0x30u16.to_le_bytes()); // USA offset
        r[6..8].copy_from_slice(&3u16.to_le_bytes()); // USN + 2 sectors
        r[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes()); // first attribute
        r[0x16..0x18].copy_from_slice(&in_use_flags.to_le_bytes());
        r[0x20..0x28].copy_from_slice(&base.to_le_bytes());
        let mut off = 0x38;
        for a in attrs {
            r[off..off + a.len()].copy_from_slice(a);
            off += a.len();
        }
        r[off..off + 4].copy_from_slice(&ATTR_END.to_le_bytes());
        r[0x18..0x1C].copy_from_slice(&((off + 8) as u32).to_le_bytes());
        // Fixup: move the last two bytes of each sector into the USA and write the USN there.
        let usn = [0xAB, 0xCD];
        r[0x30..0x32].copy_from_slice(&usn);
        for i in 1..3 {
            let end = i * 512 - 2;
            r[0x30 + i * 2] = r[end];
            r[0x30 + i * 2 + 1] = r[end + 1];
            r[end..end + 2].copy_from_slice(&usn);
        }
        r
    }

    fn resident(atype: u32, value: &[u8]) -> Vec<u8> {
        let len = (0x18 + value.len()).next_multiple_of(8);
        let mut a = vec![0u8; len];
        a[0..4].copy_from_slice(&atype.to_le_bytes());
        a[4..8].copy_from_slice(&(len as u32).to_le_bytes());
        a[0x10..0x14].copy_from_slice(&(value.len() as u32).to_le_bytes());
        a[0x14..0x16].copy_from_slice(&0x18u16.to_le_bytes());
        a[0x18..0x18 + value.len()].copy_from_slice(value);
        a
    }

    fn file_name(parent: u64, name: &str, ns: u8) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut v = vec![0u8; 0x42 + units.len() * 2];
        v[0..8].copy_from_slice(&(parent | (1 << 48)).to_le_bytes()); // sequence number in high bits
        v[0x40] = units.len() as u8;
        v[0x41] = ns;
        for (k, u) in units.iter().enumerate() {
            v[0x42 + k * 2..0x44 + k * 2].copy_from_slice(&u.to_le_bytes());
        }
        resident(ATTR_FILE_NAME, &v)
    }

    fn std_info(modified: u64, attrs: u32) -> Vec<u8> {
        let mut v = vec![0u8; 0x48];
        v[0x08..0x10].copy_from_slice(&modified.to_le_bytes());
        v[0x20..0x24].copy_from_slice(&attrs.to_le_bytes());
        resident(ATTR_STANDARD_INFORMATION, &v)
    }

    fn non_resident_data(start_vcn: u64, allocated: u64, size: u64) -> Vec<u8> {
        let mut a = vec![0u8; 0x48];
        a[0..4].copy_from_slice(&ATTR_DATA.to_le_bytes());
        a[4..8].copy_from_slice(&0x48u32.to_le_bytes());
        a[8] = 1;
        a[0x10..0x18].copy_from_slice(&start_vcn.to_le_bytes());
        a[0x20..0x22].copy_from_slice(&0x40u16.to_le_bytes());
        a[0x28..0x30].copy_from_slice(&allocated.to_le_bytes());
        a[0x30..0x38].copy_from_slice(&size.to_le_bytes());
        a
    }

    #[test]
    fn fixup_restores_sector_ends() {
        let mut r = record(&[std_info(7, 0)], RECORD_IN_USE, 0);
        assert!(apply_fixup(&mut r, 512));
        assert_eq!(&r[510..512], &[0, 0]);
    }

    #[test]
    fn fixup_rejects_torn_records() {
        let mut r = record(&[], RECORD_IN_USE, 0);
        r[1022] ^= 0xFF;
        assert!(!apply_fixup(&mut r, 512));
        assert!(!apply_fixup(&mut [0u8; 1024], 512));
    }

    #[test]
    fn parses_file_with_resident_data() {
        let mut r = record(
            &[
                std_info(132_000_000_000_000_000, FILE_ATTR_HIDDEN),
                file_name(42, "notes.txt", 1),
                resident(ATTR_DATA, b"hello"),
            ],
            RECORD_IN_USE,
            0,
        );
        assert!(apply_fixup(&mut r, 512));
        let p = parse_record(&r, 100).unwrap();
        assert_eq!(p.name.as_deref(), Some("notes.txt"));
        assert_eq!(p.parent, 42);
        assert_eq!(p.data, Some((5, 0)));
        assert_eq!(p.modified, 132_000_000_000_000_000);
        assert_eq!(p.attrs, FILE_ATTR_HIDDEN);
        assert!(!p.is_dir);
    }

    #[test]
    fn prefers_long_name_over_dos_name() {
        let mut r = record(
            &[
                file_name(5, "PROGRA~1", NAMESPACE_DOS),
                file_name(5, "Program Files", 1),
            ],
            RECORD_IN_USE | RECORD_IS_DIR,
            0,
        );
        assert!(apply_fixup(&mut r, 512));
        let p = parse_record(&r, 7).unwrap();
        assert_eq!(p.name.as_deref(), Some("Program Files"));
        assert!(p.is_dir);
    }

    #[test]
    fn reads_non_resident_sizes_from_first_segment_only() {
        let mut r = record(&[non_resident_data(0, 8192, 5000)], RECORD_IN_USE, 0);
        assert!(apply_fixup(&mut r, 512));
        assert_eq!(parse_record(&r, 1).unwrap().data, Some((5000, 8192)));

        let mut ext = record(&[non_resident_data(16, 0, 0)], RECORD_IN_USE, 77);
        assert!(apply_fixup(&mut ext, 512));
        let p = parse_record(&ext, 2).unwrap();
        assert_eq!(p.base, 77);
        assert_eq!(p.data, None);
    }

    #[test]
    fn counts_named_stream_clusters_as_size_on_disk() {
        let mut wof = non_resident_data(0, 4096, 0);
        wof[9] = 17; // name length: "WofCompressedData"
        let mut r = record(&[non_resident_data(0, 0, 9000), wof], RECORD_IN_USE, 0);
        assert!(apply_fixup(&mut r, 512));
        let p = parse_record(&r, 3).unwrap();
        assert_eq!(p.data, Some((9000, 0)));
        assert_eq!(p.stream_alloc, 4096);
    }

    #[test]
    fn skips_unused_records() {
        let mut r = record(&[file_name(5, "deleted.txt", 1)], 0, 0);
        assert!(apply_fixup(&mut r, 512));
        assert!(parse_record(&r, 9).is_none());
    }

    #[test]
    fn decodes_runlists() {
        // 0x21: 1 length byte, 2 offset bytes. Run 1: 0x18 clusters at LCN 0x5634.
        // Run 2: 0x10 clusters at delta -0x10 (0xFFF0). Run 3: sparse, 0x08 clusters.
        let runs = decode_runs(&[
            0x21, 0x18, 0x34, 0x56, 0x21, 0x10, 0xF0, 0xFF, 0x01, 0x08, 0x00,
        ]);
        assert_eq!(
            runs,
            vec![
                Run {
                    lcn: Some(0x5634),
                    clusters: 0x18
                },
                Run {
                    lcn: Some(0x5624),
                    clusters: 0x10
                },
                Run {
                    lcn: None,
                    clusters: 0x08
                },
            ]
        );
    }

    /// Compares the MFT scan with the folder walk on the system drive.
    /// Needs admin: `cargo test --release -- --ignored mft_matches_walk --nocapture` in an elevated terminal.
    #[test]
    #[ignore]
    fn mft_matches_walk() {
        let letter = std::env::var("SystemDrive")
            .unwrap_or("C:".into())
            .chars()
            .next()
            .unwrap();
        let t0 = Instant::now();
        let mft = scan_volume(letter, &Progress::default()).expect("MFT scan (run as admin)");
        let mft_time = t0.elapsed();
        let t1 = Instant::now();
        let walk = super::super::walk::walk(&format!("{letter}:\\"), &Progress::default()).unwrap();
        let walk_time = t1.elapsed();
        println!(
            "MFT : {} files, {} bytes, on disk {} bytes in {:?}",
            mft.file_count(),
            mft.total_size(),
            mft.node(0).alloc,
            mft_time
        );
        println!(
            "Walk: {} files, {} bytes, on disk {} bytes in {:?} ({} denied)",
            walk.file_count(),
            walk.total_size(),
            walk.node(0).alloc,
            walk_time,
            walk.denied
        );
    }
}

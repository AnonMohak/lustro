use serde::Serialize;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW,
    GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
    STORAGE_PROPERTY_QUERY, StorageDeviceSeekPenaltyProperty,
};
use windows::core::PCWSTR;

use super::{from_wide, wide};

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DriveKind {
    Ssd,
    Hdd,
    Removable,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    /// Drive letter without the colon, for example "C".
    pub letter: String,
    /// Root path, for example "C:\".
    pub root: String,
    pub label: String,
    pub file_system: String,
    pub kind: DriveKind,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// True for NTFS volumes, where the fast MFT scan works.
    pub supports_fast_scan: bool,
}

/// Lists fixed and removable drives that have a mounted volume.
/// Network drives and CD drives are skipped.
pub fn list_drives() -> Vec<DriveInfo> {
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| drive_info((b'A' + i) as char))
        .collect()
}

fn drive_info(letter: char) -> Option<DriveInfo> {
    let root = format!("{letter}:\\");
    let root_w = wide(&root);
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(root_w.as_ptr())) };
    if drive_type != DRIVE_FIXED && drive_type != DRIVE_REMOVABLE {
        return None;
    }

    let mut label = [0u16; 261];
    let mut fs = [0u16; 261];
    unsafe {
        GetVolumeInformationW(
            PCWSTR(root_w.as_ptr()),
            Some(&mut label),
            None,
            None,
            None,
            Some(&mut fs),
        )
    }
    .ok()?; // No media in the drive.

    let (mut free, mut total) = (0u64, 0u64);
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(root_w.as_ptr()),
            None,
            Some(&mut total),
            Some(&mut free),
        )
    }
    .ok()?;

    let file_system = from_wide(&fs);
    let kind = if drive_type == DRIVE_REMOVABLE {
        DriveKind::Removable
    } else {
        match seek_penalty(letter) {
            Some(false) => DriveKind::Ssd,
            Some(true) => DriveKind::Hdd,
            None => DriveKind::Unknown,
        }
    };
    Some(DriveInfo {
        letter: letter.to_string(),
        root,
        label: from_wide(&label),
        supports_fast_scan: file_system.eq_ignore_ascii_case("NTFS"),
        file_system,
        kind,
        total_bytes: total,
        free_bytes: free,
    })
}

/// Asks the storage driver whether the disk has a seek penalty (spinning disk).
/// This query works without admin rights because the volume is opened with no access.
fn seek_penalty(letter: char) -> Option<bool> {
    let path = wide(&format!("\\\\.\\{letter}:"));
    unsafe {
        let handle = CreateFileW(
            PCWSTR(path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
        .ok()?;
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceSeekPenaltyProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut out = DEVICE_SEEK_PENALTY_DESCRIPTOR::default();
        let mut returned = 0u32;
        let ok = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const _),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(&mut out as *mut _ as *mut _),
            size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            Some(&mut returned),
            None,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        ok.then_some(out.IncursSeekPenalty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_drive_is_listed() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let drives = list_drives();
        let d = drives
            .iter()
            .find(|d| format!("{}:", d.letter).eq_ignore_ascii_case(&system))
            .expect("system drive missing");
        assert!(d.total_bytes > 0);
        assert!(d.free_bytes <= d.total_bytes);
        assert!(!d.file_system.is_empty());
    }
}

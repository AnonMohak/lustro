//! Windows calls that the cleaner needs: running programs, services, the Recycle Bin.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, QueryServiceStatus,
    SC_MANAGER_CONNECT, SERVICE_CONTROL_STOP, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START,
    SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED, StartServiceW,
};
use windows::Win32::UI::Shell::{
    SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW, SHQUERYRBINFO,
    SHQueryRecycleBinW,
};
use windows::core::PCWSTR;

use super::Measure;
use crate::system::{from_wide, wide};

/// Lower-case names of all running programs, for example `chrome.exe`.
pub fn running_processes() -> HashSet<String> {
    let mut out = HashSet::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut e = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                out.insert(from_wide(&e.szExeFile).to_lowercase());
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

pub fn recycle_bin_size() -> Measure {
    let mut info = SHQUERYRBINFO {
        cbSize: size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    if unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.is_ok() {
        Measure {
            bytes: info.i64Size.max(0) as u64,
            files: info.i64NumItems.max(0) as u64,
        }
    } else {
        Measure::default()
    }
}

pub fn empty_recycle_bin() -> bool {
    unsafe {
        SHEmptyRecycleBinW(
            None,
            PCWSTR::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    }
    .is_ok()
}

/// Stops a service and waits up to 30 s. Returns true when this call stopped it
/// (so the caller must start it again).
pub fn stop_service(name: &str) -> bool {
    let n = wide(name);
    unsafe {
        let Ok(scm) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) else {
            return false;
        };
        let Ok(svc) = OpenServiceW(scm, PCWSTR(n.as_ptr()), SERVICE_STOP | SERVICE_QUERY_STATUS)
        else {
            let _ = CloseServiceHandle(scm);
            return false;
        };
        let mut status = SERVICE_STATUS::default();
        let mut stopped_now = false;
        if QueryServiceStatus(svc, &mut status).is_ok()
            && status.dwCurrentState == SERVICE_RUNNING
            && ControlService(svc, SERVICE_CONTROL_STOP, &mut status).is_ok()
        {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(300));
                if QueryServiceStatus(svc, &mut status).is_ok()
                    && status.dwCurrentState == SERVICE_STOPPED
                {
                    break;
                }
            }
            stopped_now = true;
        }
        let _ = CloseServiceHandle(svc);
        let _ = CloseServiceHandle(scm);
        stopped_now
    }
}

pub fn start_service(name: &str) {
    let n = wide(name);
    unsafe {
        let Ok(scm) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) else {
            return;
        };
        if let Ok(svc) = OpenServiceW(scm, PCWSTR(n.as_ptr()), SERVICE_START) {
            let _ = StartServiceW(svc, None);
            let _ = CloseServiceHandle(svc);
        }
        let _ = CloseServiceHandle(scm);
    }
}

/// Gives the Administrators group ownership and full access, so an admin can delete
/// Windows.old (its files belong to TrustedInstaller).
pub fn take_ownership(path: &Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let p = path.to_string_lossy().to_string();
    let _ = std::process::Command::new("takeown.exe")
        .args(["/F", &p, "/R", "/A", "/D", "Y"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let _ = std::process::Command::new("icacls.exe")
        .args([&p, "/grant", "*S-1-5-32-544:F", "/T", "/C", "/Q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_this_test_process() {
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_string_lossy().to_lowercase();
        assert!(running_processes().contains(&name));
    }

    #[test]
    fn recycle_bin_query_works() {
        let _ = recycle_bin_size();
    }
}

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE};
#[cfg(windows)]
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
#[cfg(windows)]
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
#[cfg(windows)]
use windows::Win32::UI::Shell::ShellExecuteW;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
#[cfg(windows)]
use windows::core::PCWSTR;

#[cfg(windows)]
use super::wide;
#[cfg(windows)]
use crate::error::Error;
use crate::error::Result;

/// True when the current process runs with an elevated (admin) token.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

/// True when the process runs as root. macOS Full Disk Access comes in a later phase.
#[cfg(unix)]
pub fn is_elevated() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Quotes one command-line argument so that `CommandLineToArgvW` reads it back unchanged.
#[cfg(windows)]
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// Starts a new elevated copy of this executable with the same arguments.
/// Windows shows the UAC prompt. The caller exits the current process on success.
#[cfg(windows)]
pub fn relaunch_elevated() -> Result<()> {
    let exe = std::env::current_exe()?;
    let mut args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--restarted")
        .map(|a| quote_arg(&a))
        .collect();
    args.push("--restarted".into());
    let exe_w = wide(&exe.to_string_lossy());
    let args_w = wide(&args.join(" "));
    let verb = wide("runas");
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(args_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value greater than 32 on success.
    if result.0 as usize > 32 {
        return Ok(());
    }
    let err = windows::core::Error::from_thread();
    if err.code() == ERROR_CANCELLED.to_hresult() {
        Err(Error::ElevationCancelled)
    } else {
        Err(err.into())
    }
}

#[cfg(not(windows))]
pub fn relaunch_elevated() -> Result<()> {
    Err(crate::error::Error::Unsupported("Run as admin"))
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::quote_arg;

    #[cfg(windows)]
    #[test]
    fn plain_args_stay_unquoted() {
        assert_eq!(quote_arg("--scan"), "--scan");
    }

    #[cfg(windows)]
    #[test]
    fn spaces_and_quotes_are_escaped() {
        assert_eq!(quote_arg("C:\\My Files"), "\"C:\\My Files\"");
        assert_eq!(quote_arg("C:\\My Files\\"), "\"C:\\My Files\\\\\"");
        assert_eq!(quote_arg("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(quote_arg(""), "\"\"");
    }

    #[test]
    fn elevation_check_does_not_panic() {
        let _ = super::is_elevated();
    }
}

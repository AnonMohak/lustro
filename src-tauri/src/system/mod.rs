pub mod drives;
pub mod elevation;

/// Converts a Rust string to a null-terminated UTF-16 buffer for Win32 calls.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Converts a null-terminated UTF-16 buffer to a Rust string.
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[cfg(test)]
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

//! macOS clean actions that run a system command: simulators of old Xcode versions.

use std::path::Path;
use std::process::Command;

use super::{CleanOutcome, Measure};

/// The udids of the simulators whose runtime is gone, from
/// `xcrun simctl list devices unavailable --json`.
pub fn parse_unavailable(json: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(runtimes) = v["devices"].as_object() else {
        return Vec::new();
    };
    runtimes
        .values()
        .filter_map(|d| d.as_array())
        .flatten()
        .filter(|d| d["isAvailable"].as_bool() == Some(false))
        .filter_map(|d| d["udid"].as_str().map(String::from))
        .collect()
}

/// The unavailable simulators, or None when Xcode is not installed.
fn unavailable() -> Option<Vec<String>> {
    let out = Command::new("xcrun")
        .args(["simctl", "list", "devices", "unavailable", "--json"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| parse_unavailable(&String::from_utf8_lossy(&out.stdout)))
}

/// The size of the unavailable simulators, or None when Xcode is not installed.
pub fn simulators_size() -> Option<Measure> {
    let home = std::env::var("HOME").ok()?;
    let devices = Path::new(&home).join("Library/Developer/CoreSimulator/Devices");
    let m = unavailable()?
        .iter()
        .map(|udid| super::measure_dir(&devices.join(udid), None))
        .fold(Measure::default(), |a, b| Measure {
            bytes: a.bytes + b.bytes,
            files: a.files + b.files,
        });
    Some(m)
}

/// Runs `xcrun simctl delete unavailable`.
pub fn delete_simulators() -> CleanOutcome {
    let before = simulators_size().unwrap_or_default();
    let ok = Command::new("xcrun")
        .args(["simctl", "delete", "unavailable"])
        .status()
        .is_ok_and(|s| s.success());
    let mut out = CleanOutcome::default();
    if ok {
        out.freed_bytes = before.bytes;
        out.deleted_files = before.files;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_unavailable_simulators() {
        let json = r#"{"devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-16-0": [
                {"udid": "AAA", "isAvailable": false, "name": "iPhone 14"},
                {"udid": "BBB", "isAvailable": true, "name": "iPhone 14 Pro"}
            ],
            "com.apple.CoreSimulator.SimRuntime.iOS-15-5": [
                {"udid": "CCC", "isAvailable": false, "name": "iPhone 13"}
            ]
        }}"#;
        let mut ids = parse_unavailable(json);
        ids.sort();
        assert_eq!(ids, ["AAA", "CCC"]);
        assert!(parse_unavailable("not json").is_empty());
        assert!(parse_unavailable(r#"{"devices": {}}"#).is_empty());
    }
}

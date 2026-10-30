//! Linux clean actions that run a system command: old snap revisions and the old journal.
//! Both need root. The caller checks admin rights first.

use std::path::Path;
use std::process::Command;

use super::{CleanOutcome, Measure, Rule};

/// The folder where snapd keeps one `<name>_<revision>.snap` file for each revision.
const SNAPS: &str = "/var/lib/snapd/snaps";

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The disabled revisions in the output of `snap list --all`, as (name, revision).
/// snapd keeps old revisions to roll back; it marks them "disabled" in the last column.
pub fn parse_disabled(list: &str) -> Vec<(String, String)> {
    list.lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            let disabled =
                cols.len() >= 6 && cols[cols.len() - 1].split(',').any(|n| n == "disabled");
            disabled.then(|| (cols[0].to_string(), cols[2].to_string()))
        })
        .collect()
}

/// The old snap revisions, or None when snap is not installed.
fn disabled_snaps() -> Option<Vec<(String, String)>> {
    run("snap", &["list", "--all"]).map(|text| parse_disabled(&text))
}

/// The size of the old snap revisions, or None when snap is not installed.
pub fn snap_old_size() -> Option<Measure> {
    let snaps = disabled_snaps()?;
    let bytes = snaps
        .iter()
        .filter_map(|(name, rev)| {
            std::fs::metadata(Path::new(SNAPS).join(format!("{name}_{rev}.snap"))).ok()
        })
        .map(|m| m.len())
        .sum();
    Some(Measure {
        bytes,
        files: snaps.len() as u64,
    })
}

/// Removes each old snap revision with `snap remove <name> --revision=<rev>`.
pub fn snap_remove_old() -> CleanOutcome {
    let mut out = CleanOutcome::default();
    for (name, rev) in disabled_snaps().unwrap_or_default() {
        let file = Path::new(SNAPS).join(format!("{name}_{rev}.snap"));
        let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        if run("snap", &["remove", &name, &format!("--revision={rev}")]).is_some() {
            out.freed_bytes += size;
            out.deleted_files += 1;
        } else {
            out.skipped_files += 1;
        }
    }
    out
}

/// Runs `journalctl --vacuum-time=<age>`. journald removes the archived journal files
/// that the rule's paths match; the size before and after gives the freed space.
pub fn vacuum_journal(rule: &Rule) -> CleanOutcome {
    let before = super::measure(rule);
    let days = (rule.min_age_h / 24).max(1);
    let _ = run("journalctl", &[&format!("--vacuum-time={days}d")]);
    let after = super::measure(rule);
    CleanOutcome {
        freed_bytes: before.bytes.saturating_sub(after.bytes),
        deleted_files: before.files.saturating_sub(after.files),
        skipped_files: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_disabled_revisions() {
        let list = "\
Name    Version   Rev    Tracking       Publisher   Notes
core20  20230801  2015   latest/stable  canonical✓  base,disabled
core20  20240111  2182   latest/stable  canonical✓  base
firefox 120.0     3504   latest/stable  mozilla✓    disabled
firefox 121.0     3600   latest/stable  mozilla✓    -
";
        assert_eq!(
            parse_disabled(list),
            vec![
                ("core20".to_string(), "2015".to_string()),
                ("firefox".to_string(), "3504".to_string()),
            ]
        );
    }

    #[test]
    fn empty_list_has_no_revisions() {
        assert!(parse_disabled("").is_empty());
        assert!(parse_disabled("No snaps are installed yet.\n").is_empty());
    }
}

//! Tauri commands for the cleaner.

use rayon::prelude::*;
use serde::Serialize;
use tauri::AppHandle;

use crate::clean::{self, system};
use crate::commands::with_history;
use crate::error::{Error, Result};
use crate::ops::history::Action;
use crate::system::elevation;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleResult {
    id: String,
    group: String,
    name: String,
    desc: String,
    bytes: u64,
    files: u64,
    safe: bool,
    admin: bool,
    warn: bool,
    /// Programs from the rule that run now. The rule is skipped while they run.
    running: Vec<String>,
    service: Option<String>,
}

/// Measures every rule. This can take a while on a large temp folder.
#[tauri::command]
pub async fn clean_scan() -> Result<Vec<RuleResult>> {
    tauri::async_runtime::spawn_blocking(|| {
        let running = system::running_processes();
        clean::rules()
            .into_par_iter()
            .map(|r| {
                let m = clean::measure(&r);
                RuleResult {
                    running: r
                        .process
                        .iter()
                        .filter(|p| running.contains(&p.to_lowercase()))
                        .cloned()
                        .collect(),
                    bytes: m.bytes,
                    files: m.files,
                    id: r.id,
                    group: r.group,
                    name: r.name,
                    desc: r.desc,
                    safe: r.safe,
                    admin: r.admin,
                    warn: r.warn,
                    service: r.service,
                }
            })
            .collect()
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleOutcome {
    id: String,
    name: String,
    freed_bytes: u64,
    deleted_files: u64,
    skipped_files: u64,
    /// Why the rule did not run, if it did not.
    skipped: Option<String>,
}

/// Runs the chosen rules and logs each one in the history.
#[tauri::command]
pub async fn clean_run(app: AppHandle, ids: Vec<String>) -> Result<Vec<RuleOutcome>> {
    tauri::async_runtime::spawn_blocking(move || {
        let admin = elevation::is_elevated();
        let running = system::running_processes();
        let rules = clean::rules();
        let mut out = Vec::new();
        for id in ids {
            let Some(rule) = rules.iter().find(|r| r.id == id) else {
                continue;
            };
            let skip = if rule.admin && !admin {
                Some("Needs admin rights.".to_string())
            } else {
                rule.process
                    .iter()
                    .find(|p| running.contains(&p.to_lowercase()))
                    .map(|p| format!("Close {p} first."))
            };
            if let Some(reason) = skip {
                out.push(RuleOutcome {
                    id: rule.id.clone(),
                    name: rule.name.clone(),
                    freed_bytes: 0,
                    deleted_files: 0,
                    skipped_files: 0,
                    skipped: Some(reason),
                });
                continue;
            }
            let r = clean::clean(rule);
            with_history(&app, |h| {
                h.add(Action::Cleaner, &rule.name, r.freed_bytes, Ok(()))
            });
            out.push(RuleOutcome {
                id: rule.id.clone(),
                name: rule.name.clone(),
                freed_bytes: r.freed_bytes,
                deleted_files: r.deleted_files,
                skipped_files: r.skipped_files,
                skipped: None,
            });
        }
        out
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

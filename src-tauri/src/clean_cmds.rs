//! Tauri commands for the cleaner.

use std::collections::HashMap;

use rayon::prelude::*;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::clean::{self, RuleItem, system};
use crate::commands::with_history;
use crate::error::{Error, Result};
use crate::ops::history::Action;
use crate::state::AppState;
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
    /// Folders that the user can pick one by one.
    items: Vec<RuleItem>,
    /// Why the rule cannot run now.
    unavailable: Option<String>,
}

fn ctx(app: &AppHandle, dev_age_days: Option<u32>) -> clean::Ctx {
    let state = app.state::<AppState>();
    let trees = state
        .trees
        .read()
        .map(|t| t.values().cloned().collect())
        .unwrap_or_default();
    clean::Ctx {
        trees,
        dev_age_days: dev_age_days.unwrap_or(30),
    }
}

/// Measures every rule. This can take a while on a large temp folder.
#[tauri::command]
pub async fn clean_scan(app: AppHandle, dev_age_days: Option<u32>) -> Result<Vec<RuleResult>> {
    let ctx = ctx(&app, dev_age_days);
    tauri::async_runtime::spawn_blocking(move || {
        let running = system::running_processes();
        clean::rules()
            .into_par_iter()
            .map(|r| {
                let s = clean::scan_rule(&r, &ctx);
                RuleResult {
                    running: r
                        .process
                        .iter()
                        .filter(|p| running.contains(&p.to_lowercase()))
                        .cloned()
                        .collect(),
                    bytes: s.measure.bytes,
                    files: s.measure.files,
                    items: s.items,
                    unavailable: s.unavailable,
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
/// `items` holds the picked paths of rules that have items.
#[tauri::command]
pub async fn clean_run(
    app: AppHandle,
    ids: Vec<String>,
    items: Option<HashMap<String, Vec<String>>>,
    dev_age_days: Option<u32>,
) -> Result<Vec<RuleOutcome>> {
    let ctx = ctx(&app, dev_age_days);
    let items = items.unwrap_or_default();
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
            let r = clean::clean_rule(rule, &ctx, items.get(&rule.id).map(|v| v.as_slice()));
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

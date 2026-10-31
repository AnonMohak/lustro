//! Tauri commands and the timer for the user's own clean rules (`clean/custom.rs`).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::clean::RuleItem;
use crate::clean::custom::{self, CustomRule};
use crate::commands::with_history;
use crate::desktop::data_dir;
use crate::error::{Error, Result};
use crate::ops::delete;
use crate::ops::history::{Action, now_ms};
use crate::state::AppState;

/// One run at a time, and no save while a run updates `last_run`.
static LOCK: Mutex<()> = Mutex::new(());

const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);
const FIRST_CHECK: Duration = Duration::from_secs(90);

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomOutcome {
    pub freed_bytes: u64,
    pub folders: u64,
    pub errors: u64,
}

fn find(app: &AppHandle, id: &str) -> Result<CustomRule> {
    custom::load(&data_dir(app))
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| Error::Rule("This rule does not exist anymore.".into()))
}

/// Moves the selected matches of `rule` to the Recycle Bin, logs them in History, and
/// takes them out of the scans. Then it saves the time of the run.
pub fn run_rule(app: &AppHandle, rule: &CustomRule) -> CustomOutcome {
    let _guard = LOCK.lock();
    let mut out = CustomOutcome::default();
    let ignore = crate::desktop::current_settings(app).ignore;
    for item in custom::scan(rule, now_ms())
        .into_iter()
        .filter(|i| i.selected && !crate::paths::ignored(&ignore, &i.path))
    {
        let result = delete::recycle(std::path::Path::new(&item.path), false);
        let action = match &result {
            Err(delete::DeleteError::Protected(_) | delete::DeleteError::AppFolder(_)) => {
                Action::Blocked
            }
            _ => Action::Recycle,
        };
        let logged = result.as_ref().map(|_| ()).map_err(|e| e.to_string());
        with_history(app, |h| h.add(action, &item.path, item.bytes, logged));
        if result.is_ok() {
            out.freed_bytes += item.bytes;
            out.folders += 1;
            if let Ok(mut trees) = app.state::<AppState>().trees.write() {
                for t in trees.values_mut() {
                    if let Some(id) = t.find_path(&item.path) {
                        Arc::make_mut(t).remove(id);
                    }
                }
            }
        } else {
            out.errors += 1;
        }
    }
    let dir = data_dir(app);
    let mut rules = custom::load(&dir);
    if let Some(r) = rules.iter_mut().find(|r| r.id == rule.id) {
        r.last_run = now_ms();
        if let Err(e) = custom::save(&dir, &rules) {
            log::warn!("Could not save the clean rules: {e}");
        }
    }
    out
}

#[tauri::command]
pub async fn custom_rules(app: AppHandle) -> Vec<CustomRule> {
    custom::load(&data_dir(&app))
}

/// Saves the whole list. New rules get an id; the time of the last run stays.
#[tauri::command]
pub async fn set_custom_rules(app: AppHandle, rules: Vec<CustomRule>) -> Result<Vec<CustomRule>> {
    let _guard = LOCK.lock();
    let dir = data_dir(&app);
    let old = custom::load(&dir);
    let mut next_id = now_ms();
    let mut out = Vec::with_capacity(rules.len());
    for mut r in rules {
        r.name = r.name.trim().to_string();
        r.targets = r
            .targets
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if let Some(why) = custom::invalid(&r) {
            return Err(Error::Rule(why));
        }
        match old.iter().find(|o| !r.id.is_empty() && o.id == r.id) {
            Some(o) => r.last_run = o.last_run,
            None => {
                r.id = format!("r{next_id}");
                next_id += 1;
                r.last_run = 0;
            }
        }
        out.push(r);
    }
    custom::save(&dir, &out)?;
    Ok(out)
}

/// The matches of a rule, for the preview.
#[tauri::command]
pub async fn custom_rule_scan(app: AppHandle, id: String) -> Result<Vec<RuleItem>> {
    let rule = find(&app, &id)?;
    let ignore = crate::desktop::current_settings(&app).ignore;
    tauri::async_runtime::spawn_blocking(move || {
        let mut items = custom::scan(&rule, now_ms());
        items.retain(|i| !crate::paths::ignored(&ignore, &i.path));
        items
    })
    .await
    .map_err(|e| Error::Rule(e.to_string()))
}

#[tauri::command]
pub async fn custom_rule_run(app: AppHandle, id: String) -> Result<CustomOutcome> {
    let rule = find(&app, &id)?;
    tauri::async_runtime::spawn_blocking(move || run_rule(&app, &rule))
        .await
        .map_err(|e| Error::Rule(e.to_string()))
}

/// Installer and disk image types of each platform.
const INSTALLERS: &[&str] = if cfg!(windows) {
    &["*.exe", "*.msi", "*.msix", "*.iso"]
} else if cfg!(target_os = "macos") {
    &["*.dmg", "*.pkg", "*.iso"]
} else if cfg!(target_os = "android") {
    &["*.apk", "*.apks", "*.xapk"]
} else {
    &["*.deb", "*.rpm", "*.AppImage", "*.iso"]
};

/// The "Old installers in Downloads" rule of the Free space page. It is not saved, and the
/// UI sends no paths: the backend builds it from the Downloads folder.
fn installers_rule(app: &AppHandle) -> Result<CustomRule> {
    let root = app
        .path()
        .download_dir()
        .map_err(|_| Error::Rule("There is no Downloads folder.".into()))?;
    Ok(CustomRule {
        id: "free-space-installers".into(),
        name: "Old installers in Downloads".into(),
        root: root.to_string_lossy().into_owned(),
        kind: custom::Target::Files,
        targets: INSTALLERS.iter().map(|s| s.to_string()).collect(),
        age_days: 14,
        schedule: custom::Schedule::Off,
        last_run: 0,
    })
}

/// Installers and disk images in Downloads with no change for 14 days.
#[tauri::command]
pub async fn installers_scan(app: AppHandle) -> Result<Vec<RuleItem>> {
    let rule = installers_rule(&app)?;
    let ignore = crate::desktop::current_settings(&app).ignore;
    tauri::async_runtime::spawn_blocking(move || {
        let mut items = custom::scan(&rule, now_ms());
        items.retain(|i| !crate::paths::ignored(&ignore, &i.path));
        items
    })
    .await
    .map_err(|e| Error::Rule(e.to_string()))
}

/// Moves the selected old installers to the Recycle Bin (with History entries).
#[tauri::command]
pub async fn installers_clean(app: AppHandle) -> Result<CustomOutcome> {
    let rule = installers_rule(&app)?;
    tauri::async_runtime::spawn_blocking(move || run_rule(&app, &rule))
        .await
        .map_err(|e| Error::Rule(e.to_string()))
}

/// Runs the rules with a schedule when they are due, also while Lustro is in the tray,
/// and sends one notification for each rule that freed space.
pub fn start_timer(app: AppHandle) {
    std::thread::spawn(move || {
        let mut ran_this_start: HashSet<String> = HashSet::new();
        std::thread::sleep(FIRST_CHECK);
        loop {
            for rule in custom::load(&data_dir(&app)) {
                if !custom::is_due(&rule, now_ms(), ran_this_start.contains(&rule.id)) {
                    continue;
                }
                ran_this_start.insert(rule.id.clone());
                let out = run_rule(&app, &rule);
                if out.folders > 0 {
                    let gb = out.freed_bytes as f64 / 1024f64.powi(3);
                    let bin = if cfg!(windows) {
                        "Recycle Bin"
                    } else {
                        "Trash"
                    };
                    let _ = app
                        .notification()
                        .builder()
                        .title(format!("Lustro ran \"{}\"", rule.name))
                        .body(format!(
                            "{} folders moved to the {bin} ({gb:.1} GB). History can restore them.",
                            out.folders
                        ))
                        .show();
                }
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

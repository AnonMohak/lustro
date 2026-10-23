//! Favourite folders in the sidebar. The paths are kept in settings.json.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::desktop::{current_settings, data_dir};
use crate::error::Result;
use crate::scan::model::Tree;
use crate::settings;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteHit {
    root: String,
    id: u32,
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    path: String,
    /// The scans that hold the folder, the smallest scan first.
    hits: Vec<FavoriteHit>,
}

/// `D:/Photos/` becomes `D:\Photos`. Drive roots give None: they are not favourites.
fn normalize(path: &str) -> Option<String> {
    #[cfg(windows)]
    let p = path.replace('/', "\\");
    #[cfg(not(windows))]
    let p = path;
    let p = p.trim_end_matches(crate::paths::SEP);
    if p.is_empty() || crate::scan::is_drive_root(p).is_some() {
        return None;
    }
    Some(p.to_string())
}

/// Adds or removes a path. The order of the other paths does not change.
fn toggle(list: &mut Vec<String>, path: &str, on: bool) {
    let has = list.iter().any(|p| crate::paths::same(p, path));
    if on && !has {
        list.push(path.to_string());
    } else if !on {
        list.retain(|p| !crate::paths::same(p, path));
    }
}

fn resolve(paths: Vec<String>, trees: &[Arc<Tree>]) -> Vec<Favorite> {
    paths
        .into_iter()
        .map(|path| {
            let mut hits: Vec<_> = trees
                .iter()
                .filter_map(|t| {
                    let id = t.find_path(&path).filter(|&id| t.node(id).is_dir())?;
                    Some(FavoriteHit {
                        root: t.root_path.clone(),
                        id,
                        size: t.node(id).size,
                    })
                })
                .collect();
            hits.sort_by_key(|h| std::cmp::Reverse(h.root.len()));
            Favorite { path, hits }
        })
        .collect()
}

fn list(app: &AppHandle) -> Vec<Favorite> {
    let trees: Vec<_> = app
        .state::<AppState>()
        .trees
        .read()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default();
    resolve(current_settings(app).favorites, &trees)
}

#[tauri::command]
pub fn favorites(app: AppHandle) -> Vec<Favorite> {
    list(&app)
}

/// Stars (`on`) or unstars a folder and returns the new list.
#[tauri::command]
pub fn set_favorite(app: AppHandle, path: String, on: bool) -> Result<Vec<Favorite>> {
    if let Some(path) = normalize(&path) {
        let mut s = current_settings(&app);
        toggle(&mut s.favorites, &path, on);
        settings::save(&data_dir(&app), &s)?;
        if let Ok(mut cur) = app.state::<AppState>().settings.lock() {
            cur.favorites = s.favorites;
        }
    }
    Ok(list(&app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{IGNORE_CASE, os};

    #[cfg(windows)]
    #[test]
    fn paths_are_normalized() {
        assert_eq!(normalize("D:/Photos/"), Some("D:\\Photos".into()));
        assert_eq!(normalize("C:\\Users\\me\\"), Some("C:\\Users\\me".into()));
        assert_eq!(normalize("C:\\"), None);
        assert_eq!(normalize("c:"), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn paths_are_normalized() {
        assert_eq!(
            normalize("/home/me/Photos/"),
            Some("/home/me/Photos".into())
        );
        assert_eq!(normalize("/"), None);
    }

    #[test]
    fn toggle_keeps_order() {
        let (a, b, c) = (os("C:\\a"), os("C:\\b"), os("C:\\c"));
        let mut l = vec![a.clone(), b.clone()];
        toggle(&mut l, &a, true);
        assert_eq!(l, [a.clone(), b.clone()]);
        toggle(&mut l, &c, true);
        assert_eq!(l, [a.clone(), b.clone(), c.clone()]);
        toggle(&mut l, &b, false);
        assert_eq!(l, [a, c]);
    }

    #[test]
    fn toggle_ignores_case_where_the_file_system_does() {
        let mut l = vec![os("C:\\a")];
        toggle(&mut l, &os("C:\\A"), true);
        assert_eq!(l.len(), if IGNORE_CASE { 1 } else { 2 });
    }

    #[test]
    fn resolve_finds_folders_only() {
        let trees = [Arc::new(crate::scan::model::tests::sample())];
        let paths = ["C:\\Users\\me", "C:\\pagefile.sys", "D:\\x"].map(os);
        let f = resolve(paths.into(), &trees);
        assert_eq!(f[0].hits.len(), 1);
        assert_eq!(f[0].hits[0].root, os("C:\\"));
        assert_eq!(f[0].hits[0].size, 110);
        assert!(f[1].hits.is_empty(), "a file is not a favourite folder");
        assert!(f[2].hits.is_empty(), "no scan holds D:");
    }
}

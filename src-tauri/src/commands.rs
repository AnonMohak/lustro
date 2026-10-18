use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{Error, Result};
use crate::ops::history::{self, Action, History};
use crate::ops::{delete, protect};
use crate::scan::kind::{KIND_COUNT, Kind};
use crate::scan::model::{Tree, flags};
use crate::scan::{self, Progress, cache, query};
use crate::state::{AppState, normalize_root};
use crate::system::{drives, elevation};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    is_admin: bool,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        is_admin: elevation::is_elevated(),
    }
}

#[tauri::command]
pub async fn list_drives() -> Vec<drives::DriveInfo> {
    // Volume queries can block on a slow USB drive, so keep them off the main thread.
    tauri::async_runtime::spawn_blocking(drives::list_drives)
        .await
        .unwrap_or_default()
}

/// Starts an elevated copy of Lustro and closes this one.
#[tauri::command]
pub fn restart_as_admin(app: AppHandle) -> Result<()> {
    elevation::relaunch_elevated()?;
    app.exit(0);
    Ok(())
}

/// Returns the folder given with `--scan <path>` on the command line, once.
/// The Explorer right-click entry uses this.
#[tauri::command]
pub fn take_startup_scan(state: State<'_, AppState>) -> Option<String> {
    state.startup_scan.lock().ok()?.take()
}

/// Reads `--scan <path>` from the command line arguments.
pub fn startup_scan_arg(args: impl Iterator<Item = String>) -> Option<String> {
    let mut args = args.skip_while(|a| a != "--scan");
    args.next()?;
    args.next().filter(|p| !p.is_empty())
}

// ---------- scanning ----------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    root: String,
    files: u32,
    total_bytes: u64,
    alloc_bytes: u64,
    method: &'static str,
    /// Unix time in milliseconds.
    scanned_at: u64,
    elapsed_ms: u64,
    denied: u32,
    breakdown: [u64; KIND_COUNT],
    /// True when the change journal can keep this scan current (fast scans).
    live: bool,
}

pub fn summary(t: &Tree) -> ScanSummary {
    ScanSummary {
        root: t.root_path.clone(),
        files: t.file_count(),
        total_bytes: t.total_size(),
        alloc_bytes: t.node(Tree::ROOT).alloc,
        method: scan::method_name(t.method),
        scanned_at: t.scanned_at * 1000,
        elapsed_ms: t.elapsed_ms,
        denied: t.denied,
        breakdown: t.breakdown(Tree::ROOT),
        live: t.usn.is_some(),
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    root: String,
    files: u64,
    dirs: u64,
    bytes: u64,
    current: String,
    fast: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ScanErrorEvent {
    root: String,
    message: String,
    cancelled: bool,
}

fn cache_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join("cache"))
}

/// Starts a scan in the background. Events: `scan-progress`, `scan-done`, `scan-error`.
#[tauri::command]
pub fn start_scan(app: AppHandle, state: State<'_, AppState>, root: String) -> Result<()> {
    let root = normalize_root(&root);
    let progress = Arc::new(Progress::default());
    {
        let mut active = state.active.lock().map_err(|_| Error::Busy)?;
        if active.is_some() {
            return Err(Error::Busy);
        }
        *active = Some((root.clone(), progress.clone()));
    }
    let fast = elevation::is_elevated() && scan::is_drive_root(&root).is_some();

    // Progress reporter.
    {
        let (app, root, progress) = (app.clone(), root.clone(), progress.clone());
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            loop {
                std::thread::sleep(Duration::from_millis(150));
                let still_running = state
                    .active
                    .lock()
                    .map(|a| a.as_ref().is_some_and(|(r, _)| *r == root))
                    .unwrap_or(false);
                if !still_running {
                    break;
                }
                let _ = app.emit(
                    "scan-progress",
                    ProgressEvent {
                        root: root.clone(),
                        files: progress.files.load(Ordering::Relaxed),
                        dirs: progress.dirs.load(Ordering::Relaxed),
                        bytes: progress.bytes.load(Ordering::Relaxed),
                        current: progress.current(),
                        fast,
                    },
                );
            }
        });
    }

    // The scan itself.
    std::thread::spawn(move || {
        let result = scan::scan(&root, fast, &progress);
        let state = app.state::<AppState>();
        if let Ok(mut a) = state.active.lock() {
            *a = None;
        }
        match result {
            Ok(tree) => {
                let tree = Arc::new(tree);
                let s = summary(&tree);
                if let Ok(mut trees) = state.trees.write() {
                    trees.insert(root.clone(), tree.clone());
                }
                let _ = app.emit("scan-done", s);
                crate::live::watch(&app, &root);
                if let Some(dir) = cache_dir(&app)
                    && let Err(e) = cache::save(&tree, &dir)
                {
                    log::warn!("Could not save the scan cache: {e}");
                }
            }
            Err(e) => {
                let cancelled = matches!(e, scan::ScanError::Cancelled);
                let _ = app.emit(
                    "scan-error",
                    ScanErrorEvent {
                        root,
                        message: e.to_string(),
                        cancelled,
                    },
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    if let Ok(a) = state.active.lock()
        && let Some((_, p)) = a.as_ref()
    {
        p.cancel();
    }
}

/// Loads the saved scans from the last session and returns their summaries.
#[tauri::command]
pub async fn load_cached_scans(app: AppHandle) -> Vec<ScanSummary> {
    let Some(dir) = cache_dir(&app) else {
        return Vec::new();
    };
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "bin") {
                match cache::load(&path) {
                    Ok(t) => out.push(t),
                    Err(e) => {
                        log::warn!("Removing unreadable cache {}: {e}", path.display());
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
        out
    })
    .await
    .unwrap_or_default();

    let state = app.state::<AppState>();
    let mut out = Vec::new();
    if let Ok(mut trees) = state.trees.write() {
        for t in loaded {
            out.push(summary(&t));
            trees.insert(t.root_path.clone(), Arc::new(t));
        }
    }
    // Replay what changed since the last session, then keep the scans current.
    for s in &out {
        crate::live::watch(&app, &s.root);
    }
    out
}

// ---------- reading a scan ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    id: u32,
    name: String,
    size: u64,
    alloc: u64,
    items: u32,
    /// Unix time in milliseconds, or 0 when unknown.
    modified: i64,
    kind: Kind,
    is_dir: bool,
    has_children: bool,
    denied: bool,
    link: bool,
    hidden: bool,
    system: bool,
    online_only: bool,
}

fn filetime_to_unix_ms(ft: u64) -> i64 {
    if ft == 0 {
        0
    } else {
        (ft as i64 - 116_444_736_000_000_000) / 10_000
    }
}

fn dto(t: &Tree, id: u32) -> NodeDto {
    let n = t.node(id);
    let name = if id == Tree::ROOT {
        t.root_path.clone()
    } else {
        t.name(id).to_string()
    };
    NodeDto {
        id,
        name,
        size: n.size,
        alloc: n.alloc,
        items: n.items,
        modified: filetime_to_unix_ms(n.modified),
        kind: Kind::from_u8(n.kind),
        is_dir: n.is_dir(),
        has_children: n.first_child != crate::scan::model::NONE,
        denied: n.flags & flags::DENIED != 0,
        link: n.flags & flags::LINK != 0,
        hidden: n.flags & flags::HIDDEN != 0,
        system: n.flags & flags::SYSTEM != 0,
        online_only: n.flags & flags::ONLINE_ONLY != 0,
    }
}

fn tree_of(state: &AppState, root: &str) -> Result<Arc<Tree>> {
    state.tree(root).ok_or(Error::NoScan)
}

fn check_id(t: &Tree, id: u32) -> Result<()> {
    if (id as usize) < t.nodes.len() {
        Ok(())
    } else {
        Err(Error::NoScan)
    }
}

/// Children of a folder, biggest first.
#[tauri::command]
pub fn list_children(
    state: State<'_, AppState>,
    root: String,
    id: u32,
    limit: Option<usize>,
) -> Result<Vec<NodeDto>> {
    let t = tree_of(&state, &root)?;
    check_id(&t, id)?;
    Ok(t.children(id)
        .take(limit.unwrap_or(usize::MAX))
        .map(|c| dto(&t, c))
        .collect())
}

#[derive(Serialize)]
pub struct Crumb {
    id: u32,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail {
    node: NodeDto,
    path: String,
    /// From the root down to this node, including both.
    ancestors: Vec<Crumb>,
    /// Bytes for each kind, indexed like `Kind`.
    breakdown: [u64; KIND_COUNT],
    /// True when Lustro never deletes this path.
    protected: bool,
}

#[tauri::command]
pub fn node_detail(state: State<'_, AppState>, root: String, id: u32) -> Result<NodeDetail> {
    let t = tree_of(&state, &root)?;
    check_id(&t, id)?;
    let mut ids = vec![id];
    let mut cur = id;
    while cur != Tree::ROOT {
        cur = t.node(cur).parent;
        if cur == crate::scan::model::NONE {
            break;
        }
        ids.push(cur);
    }
    ids.reverse();
    let ancestors = ids
        .into_iter()
        .map(|a| Crumb {
            id: a,
            name: if a == Tree::ROOT {
                t.root_path.clone()
            } else {
                t.name(a).to_string()
            },
        })
        .collect();
    let path = t.path(id);
    Ok(NodeDetail {
        node: dto(&t, id),
        protected: protect::is_protected(&path),
        path,
        ancestors,
        breakdown: t.breakdown(id),
    })
}

/// Nested data for the treemap of one folder.
#[tauri::command]
pub fn treemap(
    state: State<'_, AppState>,
    root: String,
    id: u32,
    max_nodes: Option<usize>,
) -> Result<query::TreemapNode> {
    let t = tree_of(&state, &root)?;
    check_id(&t, id)?;
    Ok(query::treemap(&t, id, max_nodes.unwrap_or(3000)))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindFilesQuery {
    sort: query::FileSort,
    min_size: Option<u64>,
    /// Unix time in milliseconds.
    older_than: Option<i64>,
    kinds: Option<Vec<String>>,
    roots: Option<Vec<String>>,
    limit: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    root: String,
    id: u32,
    name: String,
    path: String,
    folder: String,
    size: u64,
    modified: i64,
    kind: Kind,
    protected: bool,
}

fn kind_from_str(s: &str) -> Option<Kind> {
    (0..KIND_COUNT as u8).map(Kind::from_u8).find(|k| {
        serde_json::to_value(k)
            .ok()
            .and_then(|v| v.as_str().map(|x| x == s))
            .unwrap_or(false)
    })
}

/// The biggest or oldest files over all scans (or the given roots).
#[tauri::command]
pub async fn find_files(app: AppHandle, query: FindFilesQuery) -> Result<Vec<FileHit>> {
    let trees: Vec<Arc<Tree>> = {
        let state = app.state::<AppState>();
        let all = state.trees.read().map_err(|_| Error::NoScan)?;
        match &query.roots {
            Some(roots) => roots
                .iter()
                .filter_map(|r| all.get(&normalize_root(r)).cloned())
                .collect(),
            None => all.values().cloned().collect(),
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        let limit = query.limit.unwrap_or(200).min(5000);
        let filter = query::FileFilter {
            sort: query.sort,
            min_size: query.min_size.unwrap_or(0),
            older_than: query.older_than.map(unix_ms_to_filetime).unwrap_or(0),
            kinds: query
                .kinds
                .iter()
                .flatten()
                .filter_map(|k| kind_from_str(k))
                .collect(),
            limit,
        };
        let mut hits: Vec<FileHit> = Vec::new();
        for t in &trees {
            for id in query::find_files(t, &filter) {
                let n = t.node(id);
                let path = t.path(id);
                hits.push(FileHit {
                    root: t.root_path.clone(),
                    id,
                    name: t.name(id).to_string(),
                    path: path.clone(),
                    folder: t.path(n.parent),
                    size: n.size,
                    modified: filetime_to_unix_ms(n.modified),
                    kind: Kind::from_u8(n.kind),
                    protected: protect::is_protected(&path),
                });
            }
        }
        match filter.sort {
            query::FileSort::Size => hits.sort_by_key(|h| std::cmp::Reverse(h.size)),
            query::FileSort::Age => hits.sort_by_key(|h| h.modified),
        }
        hits.truncate(limit);
        hits
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchArgs {
    text: String,
    kinds: Option<Vec<String>>,
    min_size: Option<u64>,
    folders: Option<bool>,
    roots: Option<Vec<String>>,
    limit: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    root: String,
    id: u32,
    name: String,
    path: String,
    folder: String,
    size: u64,
    modified: i64,
    kind: Kind,
    is_dir: bool,
    protected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    hits: Vec<SearchHit>,
    /// Matches before the limit.
    total: usize,
    /// Names searched.
    searched: usize,
    elapsed_ms: u64,
}

/// Searches file and folder names in all scans (or the given roots).
#[tauri::command]
pub async fn search(app: AppHandle, args: SearchArgs) -> Result<SearchResult> {
    let trees: Vec<Arc<Tree>> = {
        let state = app.state::<AppState>();
        let all = state.trees.read().map_err(|_| Error::NoScan)?;
        match &args.roots {
            Some(roots) => roots
                .iter()
                .filter_map(|r| all.get(&normalize_root(r)).cloned())
                .collect(),
            None => all.values().cloned().collect(),
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let limit = args.limit.unwrap_or(500).min(5000);
        let q = query::SearchQuery {
            text: args.text,
            kinds: args
                .kinds
                .iter()
                .flatten()
                .filter_map(|k| kind_from_str(k))
                .collect(),
            min_size: args.min_size.unwrap_or(0),
            folders: args.folders.unwrap_or(true),
            limit,
        };
        let mut hits = Vec::new();
        let (mut total, mut searched) = (0, 0);
        for t in &trees {
            let (ids, n) = query::search(t, &q);
            total += n;
            searched += t.nodes.len();
            for id in ids {
                let node = t.node(id);
                let path = t.path(id);
                hits.push(SearchHit {
                    root: t.root_path.clone(),
                    id,
                    name: t.name(id).to_string(),
                    folder: t.path(node.parent),
                    size: node.size,
                    modified: filetime_to_unix_ms(node.modified),
                    kind: Kind::from_u8(node.kind),
                    is_dir: node.is_dir(),
                    protected: protect::is_protected(&path),
                    path,
                });
            }
        }
        hits.sort_by_key(|h| std::cmp::Reverse(h.size));
        hits.truncate(limit);
        SearchResult {
            hits,
            total,
            searched,
            elapsed_ms: started.elapsed().as_millis() as u64,
        }
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

/// Opens File Explorer with the item selected.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<()> {
    use std::os::windows::process::CommandExt;
    if !std::path::Path::new(&path).exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{path} no longer exists."),
        )));
    }
    let mut cmd = std::process::Command::new("explorer.exe");
    if scan::is_drive_root(&path).is_some() {
        cmd.arg(&path);
    } else {
        // explorer.exe parses its own command line, so pass the quotes as they are.
        cmd.raw_arg(format!("/select,\"{}\"", path.replace('"', "")));
    }
    cmd.spawn()?;
    Ok(())
}

fn unix_ms_to_filetime(ms: i64) -> u64 {
    (ms.max(0) as u64) * 10_000 + 116_444_736_000_000_000
}

// ---------- delete, history, restore ----------

#[derive(serde::Deserialize)]
pub struct DeleteTarget {
    root: String,
    id: u32,
}

#[derive(Serialize)]
pub struct Failed {
    path: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    deleted: u32,
    freed_bytes: u64,
    failed: Vec<Failed>,
    /// New totals of the scans that changed.
    summaries: Vec<ScanSummary>,
}

fn data_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_local_data_dir().ok()
}

/// Runs `f` with the loaded delete log, then saves it.
pub fn with_history<T>(app: &AppHandle, f: impl FnOnce(&mut History) -> T) -> Option<T> {
    let state = app.state::<AppState>();
    let mut guard = state.history.lock().ok()?;
    if guard.is_none() {
        *guard = Some(History::load(&data_dir(app)?));
    }
    let h = guard.as_mut()?;
    let out = f(h);
    if let Err(e) = h.save() {
        log::warn!("Could not save the delete history: {e}");
    }
    Some(out)
}

/// Deletes scan items. Protected paths are refused and logged as blocked.
#[tauri::command]
pub async fn delete_items(
    app: AppHandle,
    items: Vec<DeleteTarget>,
    permanent: bool,
) -> Result<DeleteResult> {
    // Resolve the items to paths while the trees are stable.
    let mut targets: Vec<(String, u32, String, u64)> = Vec::new();
    {
        let state = app.state::<AppState>();
        for it in &items {
            let t = tree_of(&state, &it.root)?;
            check_id(&t, it.id)?;
            if it.id == Tree::ROOT {
                return Err(Error::ScanRoot);
            }
            if t.node(it.id).parent == crate::scan::model::NONE {
                continue; // already deleted
            }
            targets.push((
                t.root_path.clone(),
                it.id,
                t.path(it.id),
                t.node(it.id).size,
            ));
        }
    }
    // Skip items inside another selected folder: deleting the folder removes them too.
    let folders: Vec<String> = targets
        .iter()
        .map(|t| format!("{}\\", t.2.trim_end_matches('\\').to_lowercase()))
        .collect();
    targets.retain(|t| {
        let p = t.2.to_lowercase();
        !folders
            .iter()
            .any(|f| p.starts_with(f.as_str()) && p.len() > f.len())
    });

    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut result = DeleteResult {
            deleted: 0,
            freed_bytes: 0,
            failed: Vec::new(),
            summaries: Vec::new(),
        };
        let mut changed: Vec<String> = Vec::new();
        for (root, id, path, size) in targets {
            let p = std::path::Path::new(&path);
            let outcome = if permanent {
                delete::delete_permanent(p)
            } else {
                delete::recycle(p)
            };
            let action = match (&outcome, permanent) {
                (Err(delete::DeleteError::Protected(_)), _) => Action::Blocked,
                (_, true) => Action::Permanent,
                (_, false) => Action::Recycle,
            };
            let logged = outcome.as_ref().map(|_| ()).map_err(|e| e.to_string());
            with_history(&app2, |h| h.add(action, &path, size, logged));
            match outcome {
                Ok(()) | Err(delete::DeleteError::Missing(_)) => {
                    let state = app2.state::<AppState>();
                    if let Ok(mut trees) = state.trees.write()
                        && let Some(t) = trees.get_mut(&root)
                    {
                        Arc::make_mut(t).remove(id);
                    }
                    // Keep the saved duplicate result in step with the disk.
                    if let Ok(mut d) = state.dupes.lock()
                        && let Some(groups) = d.as_mut()
                    {
                        for g in groups.iter_mut() {
                            g.files.retain(|f| !(f.root == root && f.id == id));
                        }
                        groups.retain(|g| g.files.len() > 1);
                    }
                    if !changed.contains(&root) {
                        changed.push(root);
                    }
                    if outcome.is_ok() {
                        result.deleted += 1;
                        result.freed_bytes += size;
                    }
                }
                Err(e) => result.failed.push(Failed {
                    path,
                    message: e.to_string(),
                }),
            }
        }
        // New totals, and save the changed scans in the background.
        let state = app2.state::<AppState>();
        let trees: Vec<Arc<Tree>> = changed.iter().filter_map(|r| state.tree(r)).collect();
        result.summaries = trees.iter().map(|t| summary(t)).collect();
        if let Some(dir) = cache_dir(&app2) {
            std::thread::spawn(move || {
                for t in trees {
                    if let Err(e) = cache::save(&t, &dir) {
                        log::warn!("Could not save the scan cache: {e}");
                    }
                }
            });
        }
        result
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

/// Lower-case long form of a path, so short 8.3 names (`RUNNER~1`) match long ones.
/// The item itself may be gone, so only its folder is resolved.
fn long_lower(p: &str) -> String {
    let path = std::path::Path::new(p);
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
        && let Ok(full) = std::fs::canonicalize(parent)
    {
        let joined = full.join(name);
        return joined
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_lowercase();
    }
    p.to_lowercase()
}

/// The delete log, newest first. Recycle Bin entries that are gone from the bin
/// are marked as not restorable.
#[tauri::command]
pub async fn history(app: AppHandle) -> Vec<history::Entry> {
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        with_history(&app2, |h| {
            if h.entries.iter().any(|e| e.restorable) {
                let in_bin: std::collections::HashSet<String> = trash::os_limited::list()
                    .unwrap_or_default()
                    .iter()
                    .map(|i| long_lower(&i.original_path().to_string_lossy()))
                    .collect();
                for e in h.entries.iter_mut().filter(|e| e.restorable) {
                    e.restorable = in_bin.contains(&long_lower(&e.path));
                }
            }
            h.newest_first()
        })
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default()
}

/// Moves a recycled item back to where it was.
#[tauri::command]
pub async fn restore(app: AppHandle, id: u64) -> Result<()> {
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let entry = with_history(&app2, |h| h.entries.iter().find(|e| e.id == id).cloned())
            .flatten()
            .ok_or(Error::NotInBin)?;
        let items = trash::os_limited::list().map_err(|e| Error::Restore(e.to_string()))?;
        let target = long_lower(&entry.path);
        let deleted_at = entry.time / 1000;
        let item = items
            .into_iter()
            .filter(|i| long_lower(&i.original_path().to_string_lossy()) == target)
            .min_by_key(|i| (i.time_deleted - deleted_at).abs());
        let Some(item) = item else {
            with_history(&app2, |h| {
                if let Some(e) = h.entries.iter_mut().find(|e| e.id == id) {
                    e.restorable = false;
                }
            });
            return Err(Error::NotInBin);
        };
        trash::os_limited::restore_all([item]).map_err(|e| Error::Restore(e.to_string()))?;
        with_history(&app2, |h| {
            if let Some(e) = h.entries.iter_mut().find(|e| e.id == id) {
                e.restorable = false;
                e.restored = true;
            }
        });
        Ok(())
    })
    .await
    .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?
}

#[cfg(test)]
mod tests {
    use super::{filetime_to_unix_ms, kind_from_str, unix_ms_to_filetime};
    use crate::scan::kind::Kind;

    #[test]
    fn reads_the_scan_argument() {
        let args = |v: &[&str]| {
            v.iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .into_iter()
        };
        assert_eq!(
            super::startup_scan_arg(args(&["lustro.exe", "--scan", r"D:\Photos"])),
            Some(r"D:\Photos".into())
        );
        assert_eq!(super::startup_scan_arg(args(&["lustro.exe"])), None);
        assert_eq!(
            super::startup_scan_arg(args(&["lustro.exe", "--scan"])),
            None
        );
    }

    #[test]
    fn time_round_trip() {
        assert_eq!(
            filetime_to_unix_ms(unix_ms_to_filetime(1_767_225_600_000)),
            1_767_225_600_000
        );
    }

    #[test]
    fn kinds_parse_from_their_json_names() {
        assert_eq!(kind_from_str("video"), Some(Kind::Video));
        assert_eq!(kind_from_str("dev"), Some(Kind::Dev));
        assert_eq!(kind_from_str("nope"), None);
    }

    #[test]
    fn filetime_conversion() {
        assert_eq!(filetime_to_unix_ms(0), 0);
        // 2026-01-01T00:00:00Z
        assert_eq!(
            filetime_to_unix_ms(134_116_992_000_000_000),
            1_767_225_600_000
        );
    }
}

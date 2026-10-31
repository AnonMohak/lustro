//! The command line: `lustro scan | clean | dupes | rules ...` runs without a window and
//! prints text or JSON, for scripts, Task Scheduler, cron and launchd. Other arguments
//! (`--tray`, `--scan <path>`) start the window as before.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::clean::{self, custom};
use crate::dupes::{self, DupeProgress, Mode};
use crate::ops::history::{Action, History, now_ms};
use crate::scan::{self, Progress, cache, model::Tree};
use crate::system::elevation;

const USAGE: &str = "Lustro storage manager - command line

Usage:
  lustro scan <path>... [--top N]           Scan, show the biggest items, save the scan
  lustro clean --list                       Clean-up items and their sizes
  lustro clean <id>... | --safe [--dry-run] Run clean-up items (permanent delete)
  lustro dupes <path>... [--mode exact|name|similar] [--min-size 1MB] [--top N]
                                            Find duplicates (reports only, deletes nothing)
  lustro rules                              Your rules
  lustro rules run <id or name> [--dry-run] Run one of your rules (to the Recycle Bin)
  lustro --help | --version

Options:
  --json      Print JSON instead of text
  --dry-run   Show what would happen; change nothing

Exit codes: 0 ok, 1 error, 2 wrong arguments.
On Windows, pipe the output (for example: lustro scan D:\\ | more) so that the shell waits.";

/// Runs a command line command and returns its exit code, or None when the arguments
/// start the window.
pub fn run(args: &[String]) -> Option<i32> {
    let cmd = args.get(1)?.as_str();
    if !matches!(
        cmd,
        "scan" | "clean" | "dupes" | "rules" | "help" | "--help" | "-h" | "--version" | "-V"
    ) {
        return None;
    }
    attach_console();
    let code = match parse(&args[2..]) {
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            2
        }
        Ok(o) => {
            let r = match cmd {
                "scan" => scan_cmd(&o),
                "clean" => clean_cmd(&o),
                "dupes" => dupes_cmd(&o),
                "rules" => rules_cmd(&o),
                "--version" | "-V" => {
                    println!("lustro {}", env!("CARGO_PKG_VERSION"));
                    Ok(())
                }
                _ => {
                    println!("{USAGE}");
                    Ok(())
                }
            };
            match r {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("lustro: {e}");
                    1
                }
            }
        }
    };
    Some(code)
}

/// A window program on Windows has no console; write to the one of the shell, if any.
#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: fails harmlessly when the process has a console already.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(not(windows))]
fn attach_console() {}

#[derive(Debug, Default, PartialEq)]
struct Opts {
    json: bool,
    dry_run: bool,
    list: bool,
    safe: bool,
    top: usize,
    min_size: u64,
    mode: Mode,
    args: Vec<String>,
}

/// `1MB`, `500k`, `2G` or plain bytes.
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_ascii_lowercase();
    let s = s.strip_suffix('b').unwrap_or(&s);
    let (num, mult) = match s.chars().last()? {
        'k' => (&s[..s.len() - 1], 1u64 << 10),
        'm' => (&s[..s.len() - 1], 1 << 20),
        'g' => (&s[..s.len() - 1], 1 << 30),
        't' => (&s[..s.len() - 1], 1 << 40),
        _ => (s, 1),
    };
    let n: f64 = num.trim().parse().ok()?;
    (n >= 0.0).then_some((n * mult as f64) as u64)
}

fn parse(rest: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        top: 10,
        min_size: 1 << 20,
        ..Default::default()
    };
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value."));
        match a.as_str() {
            "--json" => o.json = true,
            "--dry-run" => o.dry_run = true,
            "--list" => o.list = true,
            "--safe" => o.safe = true,
            "--top" => {
                o.top = value("--top")?
                    .parse()
                    .map_err(|_| "--top needs a number.")?
            }
            "--min-size" => {
                o.min_size =
                    parse_size(&value("--min-size")?).ok_or("--min-size needs a size like 1MB.")?
            }
            "--mode" => {
                o.mode = match value("--mode")?.as_str() {
                    "exact" => Mode::Exact,
                    "name" => Mode::Name,
                    "similar" => Mode::Similar,
                    m => return Err(format!("Unknown mode {m}. Use exact, name or similar.")),
                }
            }
            s if s.starts_with("--") => return Err(format!("Unknown option {s}.")),
            s => o.args.push(s.to_string()),
        }
    }
    Ok(o)
}

/// The app data folder of the window app, so that both share scans, rules and History.
fn data_dir() -> Result<PathBuf, String> {
    const ID: &str = "com.anonmohak.lustro";
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        env("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")))
    };
    base.map(|b| b.join(ID))
        .ok_or("There is no home folder.".into())
}

fn size(b: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", units[u])
    }
}

fn print_json(v: &serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn scan_path(path: &str) -> Result<Tree, String> {
    let root = crate::state::normalize_root(path);
    let fast = elevation::is_elevated() && scan::is_drive_root(&root).is_some();
    scan::scan(&root, fast, &Progress::default()).map_err(|e| format!("{path}: {e}"))
}

fn scan_cmd(o: &Opts) -> Result<(), String> {
    if o.args.is_empty() {
        return Err("Name a drive or folder to scan.".into());
    }
    let cache_dir = data_dir()?.join("cache");
    let mut out = Vec::new();
    for path in &o.args {
        let t = scan_path(path)?;
        if let Err(e) = cache::save(&t, &cache_dir) {
            eprintln!("lustro: could not save the scan: {e}");
        }
        let mut kids: Vec<u32> = t.children(Tree::ROOT).collect();
        kids.sort_by_key(|&id| std::cmp::Reverse(t.node(id).size));
        let top: Vec<_> = kids
            .iter()
            .take(o.top)
            .map(|&id| json!({ "path": t.path(id), "bytes": t.node(id).size, "folder": t.node(id).is_dir() }))
            .collect();
        if !o.json {
            println!(
                "{}  {}  {} files",
                t.root_path,
                size(t.total_size()),
                t.file_count()
            );
            for &id in kids.iter().take(o.top) {
                println!("  {:>10}  {}", size(t.node(id).size), t.name(id));
            }
        }
        out.push(json!({ "root": t.root_path, "bytes": t.total_size(), "files": t.file_count(), "top": top }));
    }
    if o.json {
        print_json(&json!(out));
    }
    Ok(())
}

/// The saved scans of the window app (the "dev projects" rule needs them).
fn saved_scans() -> Vec<Arc<Tree>> {
    let Ok(dir) = data_dir() else {
        return Vec::new();
    };
    std::fs::read_dir(dir.join("cache"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| cache::load(&e.path()).ok().map(Arc::new))
        .collect()
}

fn log(action: Action, what: &str, bytes: u64, r: Result<(), String>) {
    if let Ok(dir) = data_dir() {
        let mut h = History::load(&dir);
        h.add(action, what, bytes, r);
        let _ = h.save();
    }
}

fn clean_cmd(o: &Opts) -> Result<(), String> {
    let dir = data_dir()?;
    let ctx = clean::Ctx {
        trees: saved_scans(),
        dev_age_days: 30,
        ignore: crate::settings::load(&dir).ignore,
    };
    let rules = clean::rules();
    if o.list {
        let rows: Vec<_> = rules
            .iter()
            .map(|r| {
                let s = clean::scan_rule(r, &ctx);
                json!({ "id": r.id, "name": r.name, "bytes": s.measure.bytes, "safe": r.safe, "admin": r.admin })
            })
            .collect();
        if o.json {
            print_json(&json!(rows));
        } else {
            for r in &rows {
                let mark = if r["safe"].as_bool() == Some(true) {
                    "safe"
                } else {
                    "    "
                };
                println!(
                    "{:>10}  {mark}  {:<22} {}",
                    size(r["bytes"].as_u64().unwrap_or(0)),
                    r["id"].as_str().unwrap_or(""),
                    r["name"].as_str().unwrap_or("")
                );
            }
        }
        return Ok(());
    }
    let chosen: Vec<&clean::Rule> = if o.safe {
        rules.iter().filter(|r| r.safe && !r.warn).collect()
    } else {
        o.args
            .iter()
            .map(|id| {
                rules
                    .iter()
                    .find(|r| &r.id == id)
                    .ok_or(format!("No clean-up item {id}. See lustro clean --list."))
            })
            .collect::<Result<_, _>>()?
    };
    if chosen.is_empty() {
        return Err("Name clean-up items, or use --safe.".into());
    }
    let admin = elevation::is_elevated();
    let running = clean::system::running_processes();
    let mut rows = Vec::new();
    for r in chosen {
        let skip = if r.admin && !admin {
            Some("needs admin rights".to_string())
        } else {
            r.process
                .iter()
                .find(|p| running.contains(&p.to_lowercase()))
                .map(|p| format!("close {p} first"))
        };
        let bytes = match (&skip, o.dry_run) {
            (Some(_), _) => 0,
            (None, true) => clean::scan_rule(r, &ctx).measure.bytes,
            (None, false) => {
                let out = clean::clean_rule(r, &ctx, None);
                log(Action::Cleaner, &r.name, out.freed_bytes, Ok(()));
                out.freed_bytes
            }
        };
        if !o.json {
            match &skip {
                Some(why) => println!("  skipped  {} ({why})", r.id),
                None => println!("{:>10}  {}", size(bytes), r.id),
            }
        }
        rows.push(json!({ "id": r.id, "bytes": bytes, "skipped": skip }));
    }
    if o.json {
        print_json(&json!({ "dryRun": o.dry_run, "rules": rows }));
    } else if o.dry_run {
        println!("(dry run: nothing deleted)");
    }
    Ok(())
}

fn dupes_cmd(o: &Opts) -> Result<(), String> {
    if o.args.is_empty() {
        return Err("Name the drives or folders to search.".into());
    }
    let trees: Vec<Arc<Tree>> = o
        .args
        .iter()
        .map(|p| scan_path(p).map(Arc::new))
        .collect::<Result<_, _>>()?;
    let dir = data_dir()?;
    let cache = Mutex::new(dupes::cache::HashCache::load(&dir));
    let opts = dupes::Options {
        min_size: o.min_size,
        include_app_data: true,
        mode: o.mode,
        not_in: crate::settings::load(&dir).ignore,
        ..Default::default()
    };
    let groups = dupes::find(&trees, &opts, &DupeProgress::default(), &cache);
    if let Ok(mut c) = cache.lock() {
        let _ = c.save();
    }
    if o.json {
        print_json(&serde_json::to_value(&groups).unwrap_or_default());
        return Ok(());
    }
    let extra = |g: &dupes::DupeGroup| {
        g.files.iter().map(|f| f.size).sum::<u64>()
            - g.files.iter().map(|f| f.size).max().unwrap_or(0)
    };
    println!(
        "{} groups, up to {} to free",
        groups.len(),
        size(groups.iter().map(extra).sum())
    );
    for g in groups.iter().take(o.top) {
        println!("\n{} ({}, {} files)", g.kind, size(g.size), g.files.len());
        for f in &g.files {
            println!("  {}", f.path);
        }
    }
    Ok(())
}

fn rules_cmd(o: &Opts) -> Result<(), String> {
    let dir = data_dir()?;
    let mut rules = custom::load(&dir);
    match o.args.first().map(String::as_str) {
        None => {
            if o.json {
                print_json(&serde_json::to_value(&rules).unwrap_or_default());
            } else if rules.is_empty() {
                println!("No rules yet. Add them in Lustro: Clean up > Your rules.");
            } else {
                for r in &rules {
                    println!(
                        "{}  {}  ({})  {}",
                        r.id,
                        r.name,
                        r.targets.join(", "),
                        r.root
                    );
                }
            }
            Ok(())
        }
        Some("run") => {
            let want = o
                .args
                .get(1)
                .ok_or("Name the rule: lustro rules run <id or name>.")?;
            let idx = rules
                .iter()
                .position(|r| &r.id == want || r.name.eq_ignore_ascii_case(want))
                .ok_or(format!("No rule {want}. See lustro rules."))?;
            let ignore = crate::settings::load(&dir).ignore;
            let items: Vec<_> = custom::scan(&rules[idx], now_ms())
                .into_iter()
                .filter(|i| i.selected && !crate::paths::ignored(&ignore, &i.path))
                .collect();
            let mut freed = 0;
            for i in &items {
                if o.dry_run {
                    freed += i.bytes;
                } else {
                    let r = crate::ops::delete::recycle(std::path::Path::new(&i.path), false);
                    let ok = r.is_ok();
                    log(
                        Action::Recycle,
                        &i.path,
                        i.bytes,
                        r.map_err(|e| e.to_string()),
                    );
                    if ok {
                        freed += i.bytes;
                    }
                }
                if !o.json {
                    println!("{:>10}  {}", size(i.bytes), i.path);
                }
            }
            if !o.dry_run {
                rules[idx].last_run = now_ms();
                custom::save(&dir, &rules).map_err(|e| e.to_string())?;
            }
            if o.json {
                print_json(&json!({ "dryRun": o.dry_run, "items": items.len(), "bytes": freed }));
            } else {
                let verb = if o.dry_run { "would move" } else { "moved" };
                println!("{verb} {} items, {}", items.len(), size(freed));
            }
            Ok(())
        }
        Some(x) => Err(format!(
            "Unknown rules command {x}. Use: lustro rules [run <id>]."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1MB"), Some(1 << 20));
        assert_eq!(parse_size("500k"), Some(500 << 10));
        assert_eq!(parse_size("1.5G"), Some(3 << 29));
        assert_eq!(parse_size("42"), Some(42));
        assert_eq!(parse_size("lots"), None);
        assert_eq!(size(0), "0 B");
        assert_eq!(size(1536), "1.5 KB");
    }

    #[test]
    fn options() {
        let o = parse(&args(&[
            "D:",
            "--json",
            "--top",
            "5",
            "--mode",
            "name",
            "--min-size",
            "64k",
        ]))
        .unwrap();
        assert!(o.json);
        assert_eq!((o.top, o.min_size, o.mode), (5, 64 << 10, Mode::Name));
        assert_eq!(o.args, ["D:"]);
        assert!(parse(&args(&["--top"])).is_err());
        assert!(parse(&args(&["--mode", "fuzzy"])).is_err());
        assert!(parse(&args(&["--nope"])).is_err());
    }

    #[test]
    fn window_arguments_start_the_window() {
        assert_eq!(run(&args(&["lustro"])), None);
        assert_eq!(run(&args(&["lustro", "--tray"])), None);
        assert_eq!(run(&args(&["lustro", "--scan", "D:\\"])), None);
        assert_eq!(run(&args(&["lustro", "scan", "--bad"])), Some(2));
        assert_eq!(run(&args(&["lustro", "--version"])), Some(0));
    }
}

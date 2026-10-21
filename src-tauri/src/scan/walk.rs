//! Parallel folder walk. Works on every file system and without admin rights.
//! `win` and `unix` list one folder each; this file does the rest.

use std::sync::atomic::Ordering;
use std::time::Instant;

use rayon::prelude::*;

use super::model::{ScanMethod, Tree, TreeBuilder, flags};
use super::{Progress, ScanError};
use crate::paths::SEP;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod win;
#[cfg(unix)]
use unix::{Ctx, list_dir};
#[cfg(windows)]
use win::{Ctx, list_dir};

struct WFile {
    name: String,
    size: u64,
    alloc: u64,
    modified: u64,
    flags: u8,
}

#[derive(Default)]
struct WDir {
    name: String,
    modified: u64,
    flags: u8,
    files: Vec<WFile>,
    dirs: Vec<WDir>,
}

fn fill(path: &str, ctx: &Ctx, progress: &Progress, dir: &mut WDir) {
    if progress.cancelled() {
        return;
    }
    list_dir(path, ctx, progress, dir);
    progress.set_current(path);
    dir.dirs.par_iter_mut().for_each(|sub| {
        if sub.flags & flags::LINK == 0 {
            let child = format!("{}{SEP}{}", path.trim_end_matches(SEP), sub.name);
            fill(&child, ctx, progress, sub);
        }
    });
}

fn add_to_tree(b: &mut TreeBuilder, parent: u32, dir: WDir) {
    for f in dir.files {
        let id = b.push(&f.name, f.flags, f.size, f.alloc, f.modified);
        b.link(id, parent);
    }
    for d in dir.dirs {
        let id = b.push(&d.name, d.flags, 0, 0, d.modified);
        b.link(id, parent);
        add_to_tree(b, id, d);
    }
}

/// Walks `root` (a drive root like `C:\`, `/`, or any folder) and builds a tree.
pub fn walk(root: &str, progress: &Progress) -> Result<Tree, ScanError> {
    let started = Instant::now();
    let root_path = if root.ends_with(SEP) {
        root.to_string()
    } else {
        format!("{root}{SEP}")
    };
    let ctx = Ctx::new(&root_path);
    let mut top = WDir {
        flags: flags::DIR,
        ..Default::default()
    };
    fill(&root_path, &ctx, progress, &mut top);
    if progress.cancelled() {
        return Err(ScanError::Cancelled);
    }
    if top.flags & flags::DENIED != 0 && top.files.is_empty() && top.dirs.is_empty() {
        return Err(ScanError::AccessDenied(root_path));
    }

    let count = progress.files.load(Ordering::Relaxed) + progress.dirs.load(Ordering::Relaxed);
    let root_name = match root_path.trim_end_matches(SEP).rsplit(SEP).next() {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => root_path.clone(), // `/`
    };
    let mut b = TreeBuilder::new(&root_name, count as usize + 1);
    add_to_tree(&mut b, Tree::ROOT, top);
    let denied = progress.denied.load(Ordering::Relaxed) as u32;
    Ok(b.finish(
        root_path,
        ScanMethod::Walk,
        started.elapsed().as_millis() as u64,
        denied,
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;

    pub fn temp_dir(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("lustro-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn walks_a_folder_tree() {
        let root = temp_dir("walk");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/one.txt"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("a/b/two.bin"), vec![0u8; 5000]).unwrap();
        fs::write(root.join("Ünïcødé 文件.mp4"), vec![0u8; 300]).unwrap();

        let tree = walk(root.to_str().unwrap(), &Progress::default()).unwrap();
        assert_eq!(tree.total_size(), 6300);
        assert_eq!(tree.file_count(), 3);
        let first = tree.children(Tree::ROOT).next().unwrap();
        assert_eq!(tree.name(first), "a");
        assert!(
            tree.children(Tree::ROOT)
                .any(|c| tree.name(c) == "Ünïcødé 文件.mp4")
        );
        let two = tree
            .find_path(&root.join("a").join("b").join("two.bin").to_string_lossy())
            .expect("paths of the walk resolve");
        assert_eq!(tree.node(two).size, 5000);
        fs::remove_dir_all(root).unwrap();
    }
}

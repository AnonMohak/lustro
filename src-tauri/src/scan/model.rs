//! The scan result: one flat arena of nodes and one string pool for the names.

use bytemuck::{Pod, Zeroable};

use super::kind::{KIND_COUNT, Kind, apply_zone, kind_of_name, zone_of_dir};

pub const NONE: u32 = u32::MAX;

pub mod flags {
    pub const DIR: u8 = 1;
    /// Windows refused to list this folder.
    pub const DENIED: u8 = 2;
    /// Junction or symbolic link. Lustro does not follow it.
    pub const LINK: u8 = 4;
    pub const HIDDEN: u8 = 8;
    pub const SYSTEM: u8 = 16;
    /// OneDrive or other cloud file that is not on the disk.
    pub const ONLINE_ONLY: u8 = 32;
}

/// One file or folder. 48 bytes, so 2 million nodes use about 96 MB.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct Node {
    /// Logical size. For folders: the sum of everything below.
    pub size: u64,
    /// Size on disk. For folders: the sum of everything below.
    pub alloc: u64,
    /// Last write time as a Windows FILETIME (100 ns since 1601).
    pub modified: u64,
    pub parent: u32,
    pub first_child: u32,
    pub next_sibling: u32,
    pub name_off: u32,
    /// Number of files below a folder (1 for a file).
    pub items: u32,
    pub name_len: u16,
    pub flags: u8,
    /// `Kind` of a file, or the biggest `Kind` inside a folder.
    pub kind: u8,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.flags & flags::DIR != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum ScanMethod {
    Mft = 1,
    Walk = 2,
}

#[derive(Clone)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub names: String,
    /// Root path with a trailing backslash, for example `C:\` or `D:\Photos\`.
    pub root_path: String,
    pub method: ScanMethod,
    /// Unix time in seconds.
    pub scanned_at: u64,
    pub elapsed_ms: u64,
    /// Number of folders that Windows refused to list.
    pub denied: u32,
}

impl Tree {
    pub const ROOT: u32 = 0;

    pub fn node(&self, id: u32) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn name(&self, id: u32) -> &str {
        let n = self.node(id);
        &self.names[n.name_off as usize..n.name_off as usize + n.name_len as usize]
    }

    pub fn children(&self, id: u32) -> Children<'_> {
        Children {
            tree: self,
            next: self.node(id).first_child,
        }
    }

    /// Full path. Folders have no trailing backslash, except a drive root.
    pub fn path(&self, id: u32) -> String {
        if id == Self::ROOT {
            return self.root_path.clone();
        }
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != Self::ROOT && cur != NONE {
            parts.push(self.name(cur));
            cur = self.node(cur).parent;
        }
        let mut out = self.root_path.clone();
        for (i, p) in parts.iter().rev().enumerate() {
            if i > 0 {
                out.push('\\');
            }
            out.push_str(p);
        }
        out
    }

    /// Bytes for each `Kind` below a node.
    pub fn breakdown(&self, id: u32) -> [u64; KIND_COUNT] {
        let mut out = [0u64; KIND_COUNT];
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let n = self.node(cur);
            if n.is_dir() {
                stack.extend(self.children(cur));
            } else {
                out[n.kind as usize] += n.size;
            }
        }
        out
    }

    /// Removes a node (after a delete): unlinks it and subtracts its totals from every ancestor.
    /// Returns false for the root or a node that is already removed.
    pub fn remove(&mut self, id: u32) -> bool {
        if id == Self::ROOT || id as usize >= self.nodes.len() {
            return false;
        }
        let n = self.nodes[id as usize];
        if n.parent == NONE {
            return false;
        }
        // Unlink from the parent's child list.
        let parent = n.parent as usize;
        if self.nodes[parent].first_child == id {
            self.nodes[parent].first_child = n.next_sibling;
        } else {
            let mut c = self.nodes[parent].first_child;
            while c != NONE {
                let next = self.nodes[c as usize].next_sibling;
                if next == id {
                    self.nodes[c as usize].next_sibling = n.next_sibling;
                    break;
                }
                c = next;
            }
        }
        // Subtract the totals up to the root.
        let mut cur = n.parent;
        while cur != NONE {
            let a = &mut self.nodes[cur as usize];
            a.size = a.size.saturating_sub(n.size);
            a.alloc = a.alloc.saturating_sub(n.alloc);
            a.items = a.items.saturating_sub(n.items);
            cur = a.parent;
        }
        let r = &mut self.nodes[id as usize];
        r.parent = NONE;
        r.next_sibling = NONE;
        true
    }

    pub fn file_count(&self) -> u32 {
        self.node(Self::ROOT).items
    }

    pub fn total_size(&self) -> u64 {
        self.node(Self::ROOT).size
    }
}

pub struct Children<'a> {
    tree: &'a Tree,
    next: u32,
}

impl Iterator for Children<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.next == NONE {
            return None;
        }
        let cur = self.next;
        self.next = self.tree.node(cur).next_sibling;
        Some(cur)
    }
}

/// Builds a tree. Nodes can be added in any order; `link` attaches them.
pub struct TreeBuilder {
    pub nodes: Vec<Node>,
    pub names: String,
}

impl TreeBuilder {
    pub fn new(root_name: &str, capacity: usize) -> Self {
        let mut b = TreeBuilder {
            nodes: Vec::with_capacity(capacity),
            names: String::new(),
        };
        b.push(root_name, flags::DIR, 0, 0, 0);
        b
    }

    /// Adds an unlinked node and returns its id.
    pub fn push(
        &mut self,
        name: &str,
        node_flags: u8,
        size: u64,
        alloc: u64,
        modified: u64,
    ) -> u32 {
        let id = self.nodes.len() as u32;
        let name_off = self.names.len() as u32;
        // Names longer than u16::MAX bytes cannot exist on NTFS (255 UTF-16 units).
        let name = &name[..name.len().min(u16::MAX as usize)];
        self.names.push_str(name);
        let kind = if node_flags & flags::DIR != 0 {
            Kind::Other
        } else {
            kind_of_name(name)
        };
        self.nodes.push(Node {
            size,
            alloc,
            modified,
            parent: NONE,
            first_child: NONE,
            next_sibling: NONE,
            name_off,
            items: 0,
            name_len: name.len() as u16,
            flags: node_flags,
            kind: kind as u8,
        });
        id
    }

    pub fn link(&mut self, child: u32, parent: u32) {
        let p = parent as usize;
        self.nodes[child as usize].parent = parent;
        self.nodes[child as usize].next_sibling = self.nodes[p].first_child;
        self.nodes[p].first_child = child;
    }

    pub fn name(&self, id: u32) -> &str {
        let n = &self.nodes[id as usize];
        &self.names[n.name_off as usize..n.name_off as usize + n.name_len as usize]
    }

    /// Applies folder zones, adds up sizes, sorts children by size (biggest first)
    /// and sets the main `Kind` of each folder.
    pub fn finish(
        mut self,
        root_path: String,
        method: ScanMethod,
        elapsed_ms: u64,
        denied: u32,
    ) -> Tree {
        // Breadth-first order from the root. Unlinked nodes are left out.
        let mut order: Vec<u32> = Vec::with_capacity(self.nodes.len());
        let mut zones: Vec<Option<Kind>> = vec![None; self.nodes.len()];
        let mut depth: Vec<u16> = vec![0; self.nodes.len()];
        order.push(Tree::ROOT);
        let mut i = 0;
        while i < order.len() {
            let cur = order[i];
            i += 1;
            let mut c = self.nodes[cur as usize].first_child;
            while c != NONE {
                let d = depth[cur as usize] + 1;
                depth[c as usize] = d;
                let inherited = zones[cur as usize];
                if self.nodes[c as usize].is_dir() {
                    zones[c as usize] = zone_of_dir(self.name(c), d as usize).or(inherited);
                } else {
                    let k = Kind::from_u8(self.nodes[c as usize].kind);
                    self.nodes[c as usize].kind = apply_zone(k, inherited) as u8;
                }
                order.push(c);
                c = self.nodes[c as usize].next_sibling;
            }
        }
        drop(zones);
        drop(depth);

        // Bottom-up: sizes, file counts and per-folder kind totals.
        let mut dir_slot: Vec<u32> = vec![NONE; self.nodes.len()];
        let mut sums: Vec<[u64; KIND_COUNT]> = Vec::new();
        for &id in &order {
            if self.nodes[id as usize].is_dir() {
                dir_slot[id as usize] = sums.len() as u32;
                sums.push([0; KIND_COUNT]);
                let n = &mut self.nodes[id as usize];
                n.size = 0;
                n.alloc = 0;
                n.items = 0;
            } else {
                self.nodes[id as usize].items = 1;
            }
        }
        let mut kids: Vec<u32> = Vec::new();
        for &id in order.iter().rev() {
            let node = self.nodes[id as usize];
            if node.is_dir() {
                // Sort the children by size, biggest first.
                kids.clear();
                let mut c = node.first_child;
                while c != NONE {
                    kids.push(c);
                    c = self.nodes[c as usize].next_sibling;
                }
                kids.sort_by(|a, b| {
                    self.nodes[*b as usize]
                        .size
                        .cmp(&self.nodes[*a as usize].size)
                });
                let mut next = NONE;
                for &k in kids.iter().rev() {
                    self.nodes[k as usize].next_sibling = next;
                    next = k;
                }
                self.nodes[id as usize].first_child = next;
                let s = &sums[dir_slot[id as usize] as usize];
                let (best, _) = s
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, v)| **v)
                    .unwrap_or((0, &0));
                self.nodes[id as usize].kind = best as u8;
            }
            if id == Tree::ROOT {
                continue;
            }
            let parent = node.parent;
            let n = self.nodes[id as usize];
            let p = &mut self.nodes[parent as usize];
            p.size += n.size;
            p.alloc += n.alloc;
            p.items += n.items;
            let ps = dir_slot[parent as usize] as usize;
            if n.is_dir() {
                let child_sums = sums[dir_slot[id as usize] as usize];
                for (a, b) in sums[ps].iter_mut().zip(child_sums) {
                    *a += b;
                }
            } else {
                sums[ps][n.kind as usize] += n.size;
            }
        }

        let scanned_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Tree {
            nodes: self.nodes,
            names: self.names,
            root_path,
            method,
            scanned_at,
            elapsed_ms,
            denied,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// C:\ with Users\me\{a.mp4 (100), b.txt (10)}, Windows\x.dll (50), pagefile.sys (40).
    pub fn sample() -> Tree {
        let mut b = TreeBuilder::new("C:", 8);
        let users = b.push("Users", flags::DIR, 0, 0, 0);
        let me = b.push("me", flags::DIR, 0, 0, 0);
        let a = b.push("a.mp4", 0, 100, 4096, 0);
        let t = b.push("b.txt", 0, 10, 4096, 0);
        let win = b.push("Windows", flags::DIR, 0, 0, 0);
        let x = b.push("x.unknownext", 0, 50, 4096, 0);
        let pf = b.push("pagefile.sys", 0, 40, 4096, 0);
        b.link(users, 0);
        b.link(me, users);
        b.link(a, me);
        b.link(t, me);
        b.link(win, 0);
        b.link(x, win);
        b.link(pf, 0);
        b.finish("C:\\".into(), ScanMethod::Walk, 1, 0)
    }

    #[test]
    fn node_is_48_bytes() {
        assert_eq!(size_of::<Node>(), 48);
    }

    #[test]
    fn sizes_roll_up() {
        let t = sample();
        assert_eq!(t.total_size(), 200);
        assert_eq!(t.file_count(), 4);
        assert_eq!(t.node(0).alloc, 4 * 4096);
    }

    #[test]
    fn children_are_sorted_biggest_first() {
        let t = sample();
        let names: Vec<&str> = t.children(0).map(|c| t.name(c)).collect();
        assert_eq!(names, ["Users", "Windows", "pagefile.sys"]);
    }

    #[test]
    fn paths_are_built_from_parents() {
        let t = sample();
        let users = t.children(0).next().unwrap();
        let me = t.children(users).next().unwrap();
        let a = t.children(me).next().unwrap();
        assert_eq!(t.path(0), "C:\\");
        assert_eq!(t.path(a), "C:\\Users\\me\\a.mp4");
    }

    #[test]
    fn remove_updates_ancestors() {
        let mut t = sample();
        let users = t.children(0).next().unwrap();
        let me = t.children(users).next().unwrap();
        let a = t.children(me).next().unwrap();
        assert!(t.remove(a));
        assert_eq!(t.total_size(), 100);
        assert_eq!(t.file_count(), 3);
        assert_eq!(t.node(users).size, 10);
        assert_eq!(
            t.children(me).map(|c| t.name(c)).collect::<Vec<_>>(),
            ["b.txt"]
        );
        assert!(!t.remove(a), "already removed");
        assert!(!t.remove(Tree::ROOT));
        // Removing the first and last child keeps the list intact.
        let pf = t
            .children(0)
            .find(|&c| t.name(c) == "pagefile.sys")
            .unwrap();
        assert!(t.remove(pf));
        assert_eq!(t.children(0).count(), 2);
    }

    #[test]
    fn zones_and_folder_kinds() {
        let t = sample();
        let win = t.children(0).find(|&c| t.name(c) == "Windows").unwrap();
        let x = t.children(win).next().unwrap();
        assert_eq!(t.node(x).kind, Kind::System as u8);
        assert_eq!(t.node(win).kind, Kind::System as u8);
        let users = t.children(0).next().unwrap();
        assert_eq!(t.node(users).kind, Kind::Video as u8);
        let b = t.breakdown(0);
        assert_eq!(b[Kind::Video as usize], 100);
        assert_eq!(b[Kind::System as usize], 90);
    }
}

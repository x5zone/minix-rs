//! Inode tree for the VTreeFS framework subset (doc 02-vtreefs-framework).
//!
//! C: `minix3/minix/lib/libvtreefs/inode.c` (626 lines) +
//! `minix3/minix/include/minix/vtreefs.h:14` (`PNAME_MAX`) +
//! `minix3/sys/sys/syslimits.h:57` (`NAME_MAX 511`).
//!
//! devman uses a strict subset: every inode is created with `NO_INDEX`
//! (4× `add_inode(..., NO_INDEX, ...)` in `device.c`), so the
//! `<parent,index>` hash table and the indexed-slot machinery
//! (`get_inode_by_index`, `get_inode_index`, `get_inode_slots`,
//! `purge_inode` reclaim) are omitted **at the type level** —
//! inexpressible, not silently dropped (02 §3.3).
//!
//! # Single-threaded model
//!
//! The tree is owned by [`super::VTreeFs`], itself driven by the devman
//! single-threaded event loop (see `crate` docs). Plain integers and
//! `Vec` suffice; no atomics, no locks.

use alloc::string::String;
use alloc::vec::Vec;
use minix_types::Errno;

use crate::hooks::S_IFDIR;
use crate::RootStat;

/// C: `PNAME_MAX 24` (`vtreefs.h:14`) — names this long or shorter are
/// "static" in C (`i_namebuf`); longer names are heap-allocated.
/// Rust `String` is uniformly heap, so the threshold has no representation
/// here; it is documented for the 02 §2.3 name-handling analysis.
pub const PNAME_MAX_LEN: usize = 24;

/// C: `NAME_MAX 511` (`minix3/sys/sys/syslimits.h:57`) — hard name limit
/// enforced by `add_inode`/`get_inode_by_name`/`fs_lookup` asserts.
pub const NAME_MAX_LEN: usize = 511;

/// POSIX file-type mask and directory/regular bits (used for the
/// `S_ISDIR`/`S_ISREG` checks in `add_inode`/`fs_read`).
/// Upstream: `99-devm-global-concepts.md` moves these into `minix-types`.
pub const S_IFMT: u32 = 0o170_000;
pub const S_IFREG: u32 = 0o100_000;

fn is_dir(mode: u32) -> bool {
    mode & S_IFMT == S_IFDIR
}

/// External inode number: 1-based (`ino_t = i_num + 1`, inode.c:369-375).
/// `Ino(0)` is never valid (C `find_inode(0)` reads `&inode[-1]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ino(pub u32);

/// C: `struct inode_stat` (`vtreefs.h:16-22`) as an immutable value.
/// Same five fields as [`RootStat`]; a distinct type because this one
/// describes *every* inode, not just the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InodeStat {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: i64,
    pub dev: i32,
}

impl From<RootStat> for InodeStat {
    fn from(r: RootStat) -> Self {
        InodeStat {
            mode: r.mode,
            uid: r.uid,
            gid: r.gid,
            size: r.size,
            dev: r.dev,
        }
    }
}

/// One tree node.
///
/// C: `struct inode` (`inode.h:29-56`) minus `i_index`/`i_indexed`
/// (NO_INDEX-only, omitted), minus the hash links (replaced by a
/// `BTreeMap`-free linear child scan — see [`InodeTree`] for why),
/// minus `i_namebuf` (uniform `String`).
///
/// `Debug` supports `assert_eq!` on fallible constructors in tests.
#[derive(Debug, PartialEq, Eq)]
pub struct Inode {
    parent: Option<usize>,
    children: Vec<usize>,
    name: String,
    stat: InodeStat,
    refcount: u32,
    deleted: bool,
    cbdata: usize,
}

impl Inode {
    /// C: `node->i_stat.mode` (+ `S_ISREG`/`S_ISDIR` checks at use sites).
    pub fn mode(&self) -> u32 {
        self.stat.mode
    }

    /// C: `node->i_name` (valid only while live; see [`InodeTree::name`]).
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// C: `node->i_flags & I_DELETED`.
    pub fn deleted(&self) -> bool {
        self.deleted
    }

    /// C: `node->i_cbdata`.
    pub fn cbdata(&self) -> usize {
        self.cbdata
    }

    /// C: `node->i_count` (test/debug visibility).
    pub fn refcount(&self) -> u32 {
        self.refcount
    }
}

/// The inode pool + tree.
///
/// C: `inode[]` array + `unused_inodes` free list + two hash tables
/// (inode.c:6-16). Rust replaces the array+free-list with a `Vec` pool
/// plus a free stack, and both hash tables with direct child scans:
/// devman trees are tiny (bounded by the 1024 pool; realistically dozens
/// of devices), so hash tables buy nothing and cost a second index to
/// keep consistent. Lookup is O(children), allocation O(1).
#[derive(Debug, PartialEq, Eq)]
pub struct InodeTree {
    nodes: Vec<Inode>,
    free: Vec<usize>,
    capacity: u32,
}

impl InodeTree {
    /// C: `init_inodes(inodes, istat, 0)` (inode.c:31-99).
    /// Three C mallocs (node array + two hash tables, ENOMEM with
    /// reverse-order cleanup) become one `try_reserve` → `ENOMEM`.
    /// `capacity == 0` is `EINVAL` (C `assert(inodes > 0)`).
    pub fn new(capacity: u32, root_stat: InodeStat) -> Result<Self, Errno> {
        if capacity == 0 {
            return Err(Errno::EINVAL);
        }
        let mut nodes = Vec::new();
        nodes
            .try_reserve(capacity as usize)
            .map_err(|_| Errno::ENOMEM)?;
        nodes.push(Inode {
            // C: `&inode[0]`, parent NULL, count 0, flags 0,
            // index NO_INDEX, stat copy, cbdata NULL (inode.c:86-96).
            parent: None,
            children: Vec::new(),
            name: String::new(),
            stat: root_stat,
            refcount: 0,
            deleted: false,
            cbdata: 0,
        });
        let mut free = Vec::new();
        free.try_reserve(capacity as usize - 1)
            .map_err(|_| Errno::ENOMEM)?;
        // C pushes pool slots 1..nr_inodes head-first (inode.c:70-78);
        // order among free slots is unobservable, so push in any order.
        for i in 1..capacity as usize {
            free.push(i);
        }
        // Pre-size `nodes` so pool indices stay stable: index i is Ino(i+1).
        // (Unoccupied slots are filled on `add`; `free` holds their indices.)
        Ok(InodeTree {
            nodes,
            free,
            capacity,
        })
    }

    /// C: `get_root_inode()` — always pool slot 0 (inode.c:255-260).
    pub fn root(&self) -> Ino {
        Ino(1)
    }

    fn idx(&self, ino: Ino) -> Result<usize, Errno> {
        if ino.0 == 0 || ino.0 > self.capacity {
            return Err(Errno::EINVAL);
        }
        Ok((ino.0 - 1) as usize)
    }

    /// C: `find_inode()` (inode.c:454-463) — bounds-checked lookup.
    /// Like C, a deleted-but-scheduled node is still returned; callers
    /// check [`InodeTree::is_deleted`]. Unlike C, a *reaped* (freed) slot
    /// resolves to `None`: C returns the stale slot content, Rust refuses
    /// (memory-safety hardening, 02 §4.3).
    pub fn find(&self, ino: Ino) -> Option<&Inode> {
        let i = (ino.0.checked_sub(1)?) as usize;
        self.nodes.get(i).filter(|_| self.occupied(i))
    }

    fn occupied(&self, i: usize) -> bool {
        // Slot 0 (root) is always occupied; other slots are occupied
        // unless parked on the free stack.
        // NOTE: callers inside `&mut` sections evaluate this to a `bool`
        // first and only then `get_mut`, so the borrows never overlap.
        i == 0 || !self.free.contains(&i)
    }

    /// C: `get_inode_number()` = `i_num + 1` (inode.c:369-375).
    pub fn number(&self, ino: Ino) -> Result<Ino, Errno> {
        self.find(ino).map(|_| ino).ok_or(Errno::EINVAL)
    }

    /// C: `get_inode_name()` (inode.c:266-274) — asserts non-deleted and
    /// non-NULL name. Rust returns `None` instead of aborting.
    pub fn name(&self, ino: Ino) -> Option<&str> {
        let n = self.find(ino)?;
        if n.deleted {
            return None;
        }
        Some(n.name.as_str())
    }

    /// C: `get_inode_cbdata()` (inode.c:304-310).
    pub fn cbdata(&self, ino: Ino) -> Option<usize> {
        self.find(ino).map(|n| n.cbdata)
    }

    /// C: `get_inode_stat()` (inode.c:380-387).
    pub fn stat(&self, ino: Ino) -> Option<InodeStat> {
        self.find(ino).map(|n| n.stat)
    }

    /// C: `is_inode_deleted()` (inode.c:599-604).
    pub fn is_deleted(&self, ino: Ino) -> bool {
        self.find(ino).is_some_and(|n| n.deleted)
    }

    /// C: `get_parent_inode()` (inode.c:315-326) — root has no parent.
    pub fn parent_of(&self, ino: Ino) -> Option<Ino> {
        let n = self.find(ino)?;
        n.parent.map(|p| Ino(p as u32 + 1))
    }

    /// C: `ref_inode()` (inode.c:504-510).
    pub fn reference(&mut self, ino: Ino) -> Result<(), Errno> {
        let i = self.idx(ino)?;
        if !self.occupied(i) {
            return Err(Errno::EINVAL);
        }
        let n = self.nodes.get_mut(i).ok_or(Errno::EINVAL)?;
        n.refcount += 1;
        Ok(())
    }

    /// C: `put_inode()` (inode.c:484-498) — decrement; a node scheduled
    /// for deletion with count reaching 0 is deleted now.
    /// Underflow (`count == 0`, C `assert`) is `EINVAL`.
    pub fn release(&mut self, ino: Ino) -> Result<(), Errno> {
        let i = self.idx(ino)?;
        if !self.occupied(i) {
            return Err(Errno::EINVAL);
        }
        {
            let n = self.nodes.get_mut(i).ok_or(Errno::EINVAL)?;
            if n.refcount == 0 {
                return Err(Errno::EINVAL);
            }
            n.refcount -= 1;
        }
        let reap = self
            .nodes
            .get(i)
            .is_some_and(|n| n.deleted && n.refcount == 0);
        if reap {
            self.reap(i);
        }
        Ok(())
    }

    /// C: `fs_lookup` name resolution (path.c:9-59), framework half:
    /// bad slot → `EINVAL` (C asserts); non-directory → `ENOTDIR`
    /// (path.c:19-20); over-long name → `ENAMETOOLONG`; `"."` stays,
    /// `".."` goes to the parent (`ENOENT` for root — C path.c:30,
    /// "should not be possible", preserved verbatim); otherwise the
    /// `<parent,name>` search, absent → `ENOENT`.
    /// The `lookup_hook` refresh call is devman-absent (hook never
    /// registered — main.c:78-80), so no refresh slot exists.
    /// NOTE: unlike C, no reference is taken on success — VFS-side open
    /// ref management belongs to the transport (VFS stage), not the tree.
    pub fn lookup(&self, parent: Ino, name: &str) -> Result<Ino, Errno> {
        if name.len() > NAME_MAX_LEN {
            return Err(Errno::ENAMETOOLONG);
        }
        let p = self.idx(parent)?;
        let node = self
            .nodes
            .get(p)
            .filter(|_| self.occupied(p))
            .ok_or(Errno::EINVAL)?;
        if !is_dir(node.stat.mode) {
            return Err(Errno::ENOTDIR);
        }
        if name == "." {
            return Ok(parent);
        }
        if name == ".." {
            return self.parent_of(parent).ok_or(Errno::ENOENT);
        }
        for &c in &node.children {
            let Some(child) = self.nodes.get(c) else {
                continue;
            };
            if !child.deleted && child.name == name {
                return Ok(Ino(c as u32 + 1));
            }
        }
        Err(Errno::ENOENT)
    }

    /// First non-deleted child (C: `get_first_inode`, inode.c:331-345)
    /// and next non-deleted sibling (C: `get_next_inode`, inode.c:351-362).
    /// Backs the getdents traversal (file.c:236-262: skip indexed — vacuous
    /// here since every node is NO_INDEX — then skip deleted).
    pub fn first_child(&self, parent: Ino) -> Option<Ino> {
        let p = self.idx(parent).ok()?;
        let node = self.nodes.get(p)?;
        node.children.iter().find_map(|&c| {
            self.nodes
                .get(c)
                .filter(|n| !n.deleted)
                .map(|_| Ino(c as u32 + 1))
        })
    }

    pub fn next_child(&self, prev: Ino) -> Option<Ino> {
        let i = self.idx(prev).ok()?;
        let node = self.nodes.get(i)?;
        let parent = node.parent?;
        let siblings = &self.nodes.get(parent)?.children;
        let at = siblings.iter().position(|&c| c == i)?;
        siblings.iter().skip(at + 1).find_map(|&c| {
            self.nodes
                .get(c)
                .filter(|n| !n.deleted)
                .map(|_| Ino(c as u32 + 1))
        })
    }

    /// C: `add_inode()` (inode.c:185-249), devman subset (`idx` is always
    /// `NO_INDEX`, `nr_indexed_entries` always 0 — both fixed, not taken
    /// as parameters).
    ///
    /// C asserts become errors: non-dir/deleted parent → `EINVAL`;
    /// over-long name → `ENAMETOOLONG`; duplicate name → `EEXIST`
    /// (C `assert(get_inode_by_name(...) == NULL)`); exhausted pool →
    /// `ENOMEM` (C `purge_inode` only reclaims *indexed* nodes, of which
    /// devman has none, then asserts non-empty — so for devman the pool
    /// is a hard cap; Rust reports it instead of aborting, [ARCH:A-7]).
    pub fn add(
        &mut self,
        parent: Ino,
        name: &str,
        stat: InodeStat,
        cbdata: usize,
    ) -> Result<Ino, Errno> {
        if name.len() > NAME_MAX_LEN {
            return Err(Errno::ENAMETOOLONG);
        }
        let p = self.idx(parent)?;
        {
            let node = self.nodes.get(p).filter(|_| self.occupied(p));
            match node {
                Some(n) if !n.deleted && is_dir(n.stat.mode) => {}
                _ => return Err(Errno::EINVAL),
            }
            if self.lookup(parent, name).is_ok() {
                return Err(Errno::EEXIST);
            }
        }
        let i = self.free.pop().ok_or(Errno::ENOMEM)?;
        // Grow `nodes` so pool index == position (stability for Ino).
        if self.nodes.len() <= i {
            self.nodes
                .try_reserve(i + 1 - self.nodes.len())
                .map_err(|_| {
                    self.free.push(i);
                    Errno::ENOMEM
                })?;
            while self.nodes.len() <= i {
                self.nodes.push(Inode {
                    parent: None,
                    children: Vec::new(),
                    name: String::new(),
                    stat,
                    refcount: 0,
                    deleted: true,
                    cbdata: 0,
                });
            }
        }
        let node = &mut self.nodes[i];
        node.parent = Some(p);
        node.children.clear();
        node.name.clear();
        node.name.push_str(name);
        node.stat = stat;
        node.refcount = 0;
        node.deleted = false;
        node.cbdata = cbdata;
        self.nodes[p].children.push(i);
        Ok(Ino(i as u32 + 1))
    }

    /// C: `delete_inode()` (inode.c:544-594) — children first (recursively,
    /// before the flag is set), unhash (name table only — no index table
    /// exists here), free an owned name (uniform `String`, nothing to do),
    /// set `I_DELETED`; files unlink from the parent immediately while
    /// directories keep the parent link until count hits 0 with no
    /// children (so `cd ..` keeps working — inode.c:539-541).
    /// Root deletion is `EINVAL` (C asserts `node != &inode[0]`).
    /// A node with `refcount > 0` stays scheduled; [`InodeTree::release`]
    /// reaps it at zero.
    pub fn delete(&mut self, ino: Ino) -> Result<(), Errno> {
        let i = self.idx(ino)?;
        if i == 0 {
            return Err(Errno::EINVAL);
        }
        if self.nodes.get(i).filter(|_| self.occupied(i)).is_none() {
            return Err(Errno::EINVAL);
        }
        // Children first (clone the list: recursion mutates it).
        let kids: Vec<usize> = self.nodes[i].children.clone();
        for k in kids {
            self.delete(Ino(k as u32 + 1))?;
        }
        {
            let node = &mut self.nodes[i];
            if !node.deleted {
                node.deleted = true;
                // Files unlink now; directories keep the parent link
                // until reaped (C inode.c:575-580 + 583-590).
                if !is_dir(node.stat.mode) {
                    Self::unlink_from_parent_static(&mut self.nodes, i);
                }
            }
        }
        if self.nodes[i].refcount == 0 && self.nodes[i].children.is_empty() {
            self.reap(i);
        }
        Ok(())
    }

    fn unlink_from_parent_static(nodes: &mut [Inode], i: usize) {
        let p = match nodes.get_mut(i).and_then(|n| n.parent.take()) {
            Some(p) => p,
            None => return,
        };
        if let Some(parent) = nodes.get_mut(p) {
            parent.children.retain(|&c| c != i);
        }
    }

    /// Return a fully-unlinked, unreferenced node to the free stack.
    fn reap(&mut self, i: usize) {
        Self::unlink_from_parent_static(&mut self.nodes, i);
        self.nodes[i].children.clear();
        self.nodes[i].name.clear();
        self.nodes[i].cbdata = 0;
        if !self.free.contains(&i) {
            self.free.push(i);
        }
    }

    /// Live (non-deleted) node count, root included. Test/support helper.
    pub fn live_count(&self) -> usize {
        (0..self.nodes.len())
            .filter(|&i| self.occupied(i) && !self.nodes[i].deleted)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{S_IFDIR, S_IRALL};

    fn dir_stat() -> InodeStat {
        InodeStat {
            mode: S_IFDIR | S_IRALL,
            uid: 0,
            gid: 0,
            size: 0,
            dev: 0,
        }
    }

    fn file_stat() -> InodeStat {
        InodeStat {
            mode: S_IFREG | S_IRALL,
            uid: 0,
            gid: 0,
            size: 0,
            dev: 0,
        }
    }

    #[test]
    fn init_root_number_is_one() {
        // C: ino_t = i_num + 1 (inode.c:369-375); root is slot 0.
        let t = InodeTree::new(8, dir_stat()).unwrap();
        assert_eq!(t.root(), Ino(1));
        assert_eq!(t.number(Ino(1)).unwrap(), Ino(1));
        assert_eq!(t.live_count(), 1);
    }

    #[test]
    fn init_zero_capacity_is_einval() {
        // C: assert(inodes > 0) (inode.c:37).
        assert_eq!(InodeTree::new(0, dir_stat()), Err(Errno::EINVAL));
    }

    #[test]
    fn add_lookup_roundtrip() {
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        let dev = t.add(Ino(1), "devices", dir_stat(), 0xA).unwrap();
        assert_eq!(t.lookup(Ino(1), "devices").unwrap(), dev);
        assert_eq!(t.name(dev).unwrap(), "devices");
        assert_eq!(t.cbdata(dev).unwrap(), 0xA);
        assert_eq!(t.lookup(Ino(1), "nope"), Err(Errno::ENOENT));
    }

    #[test]
    fn add_duplicate_is_eexist() {
        // C: assert(get_inode_by_name(...) == NULL) (inode.c:200).
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        t.add(Ino(1), "devices", dir_stat(), 0).unwrap();
        assert_eq!(
            t.add(Ino(1), "devices", dir_stat(), 0),
            Err(Errno::EEXIST)
        );
    }

    #[test]
    fn add_under_file_is_einval() {
        // C: assert(S_ISDIR(parent->i_stat.mode)) (inode.c:194).
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        let f = t.add(Ino(1), "f", file_stat(), 0).unwrap();
        assert_eq!(t.add(f, "x", file_stat(), 0), Err(Errno::EINVAL));
    }

    #[test]
    fn lookup_dots_and_notdir() {
        // C: "." stays, ".." goes up, root ".." is ENOENT (path.c:23-31);
        // non-dir parent is ENOTDIR (path.c:19-20).
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        let d = t.add(Ino(1), "devices", dir_stat(), 0).unwrap();
        let f = t.add(Ino(1), "f", file_stat(), 0).unwrap();
        assert_eq!(t.lookup(d, ".").unwrap(), d);
        assert_eq!(t.lookup(d, "..").unwrap(), Ino(1));
        assert_eq!(t.lookup(Ino(1), ".."), Err(Errno::ENOENT));
        assert_eq!(t.lookup(f, "x"), Err(Errno::ENOTDIR));
    }

    #[test]
    fn add_long_name_is_enametoolong() {
        // C: NAME_MAX 511 (syslimits.h:57); PNAME_MAX 24 is inline-only.
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        let long = alloc::string::String::from_utf8(alloc::vec![b'n'; 512]).unwrap();
        assert_eq!(
            t.add(Ino(1), long.as_str(), file_stat(), 0),
            Err(Errno::ENAMETOOLONG)
        );
        // 511 itself is accepted.
        let edge = alloc::string::String::from_utf8(alloc::vec![b'n'; 511]).unwrap();
        assert!(t.add(Ino(1), edge.as_str(), file_stat(), 0).is_ok());
    }

    #[test]
    fn pool_exhaustion_is_enomem() {
        // C: purge_inode only reclaims *indexed* nodes (inode.c:169) —
        // devman has none, so the pool is a hard cap (then assert).
        // Rust reports ENOMEM instead of aborting ([ARCH:A-7]).
        let mut t = InodeTree::new(2, dir_stat()).unwrap();
        t.add(Ino(1), "a", dir_stat(), 0).unwrap();
        assert_eq!(t.add(Ino(1), "b", dir_stat(), 0), Err(Errno::ENOMEM));
    }

    #[test]
    fn delete_recursive_and_reuses_slot() {
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        let d = t.add(Ino(1), "devices", dir_stat(), 0).unwrap();
        let f = t.add(d, "dev_type", file_stat(), 0).unwrap();
        t.delete(d).unwrap();
        // Child went with the parent (C inode.c:556-558, before the flag);
        // both slots returned to the pool (refcounts were 0).
        assert!(t.find(f).is_none());
        assert_eq!(t.lookup(Ino(1), "devices"), Err(Errno::ENOENT));
        assert_eq!(t.live_count(), 1);
        assert_eq!(t.lookup(d, "dev_type"), Err(Errno::EINVAL));
        assert_eq!(t.live_count(), 1);
        // Freed slots are reusable.
        assert!(t.add(Ino(1), "events", dir_stat(), 0).is_ok());
    }

    #[test]
    fn delete_referenced_is_lazy() {
        // C: count > 0 keeps the node scheduled (inode.c:583-593);
        // release() at zero reaps it (inode.c:496-497).
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        let f = t.add(Ino(1), "f", file_stat(), 0).unwrap();
        t.reference(f).unwrap();
        t.delete(f).unwrap();
        assert!(t.is_deleted(f));
        assert_eq!(t.lookup(Ino(1), "f"), Err(Errno::ENOENT));
        t.release(f).unwrap();
        // Slot reusable after reap.
        assert!(t.add(Ino(1), "g", file_stat(), 0).is_ok());
    }

    #[test]
    fn release_underflow_is_einval() {
        // C: assert(node->i_count > 0) (inode.c:488).
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        let f = t.add(Ino(1), "f", file_stat(), 0).unwrap();
        assert_eq!(t.release(f), Err(Errno::EINVAL));
    }

    #[test]
    fn root_delete_and_ino_zero_are_einval() {
        // C: assert(node != &inode[0]) (inode.c:549); ino 0 reads
        // out of range (inode.c:458).
        let mut t = InodeTree::new(8, dir_stat()).unwrap();
        assert_eq!(t.delete(Ino(1)), Err(Errno::EINVAL));
        assert_eq!(t.lookup(Ino(0), "x"), Err(Errno::EINVAL));
        assert!(t.find(Ino(0)).is_none());
    }
}

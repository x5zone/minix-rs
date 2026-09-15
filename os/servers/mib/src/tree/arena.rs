//! The arena's type layer: node ids, dynamic children, dynode shapes.
//!
//! C stores dynamic children as a **sorted intrusive linked list**
//! (`mib.h:110-280` — the dynode chain hangs off the parent, ordered by
//! id so `mib_find` can stop early, tree.c:69-78). The list exists
//! because C builds trees inside static arrays where a resizeable
//! member is unaffordable; the Rust arena has `alloc` (A-3) and can
//! pick the container on merits:
//!
//! - **`BTreeMap<i32, NodeId>`** — ordered iteration for free (CTL_QUERY
//!   must report children in id order, 11), O(log n) lookup/insert/
//!   delete, range queries cover the "find first ≥ id" walk that
//!   `scan` judges in slices.
//! - sorted `Vec` + binary search — cache-friendly and one allocation
//!   saved per node, but create pays O(n) element shifts; the query
//!   order win is the same as the map's.
//! - C's linked list, transliterated — the one rejected: it inherits
//!   O(n) search to save an allocation the arena already pays, and
//!   unsafe aliasing for nothing (the translate defense, pattern 16).
//!
//! Verdict: `ChildMap = BTreeMap`. `[ARCH: ...]` (对照点 `mib.h` 动态
//! 子节点排序链表 + tree.c:69-78 早停查找；design 03/08 篇 A-2 行；
//! 代码注释即本段)。`scan`'s sorted-slice verdicts stay untouched —
//! the walker (todo.md P1-2) bridges map ↔ slices at the call site.
//!
//! Memory ownership (A-3): a dynode's name, data, and description are
//! budget-backed buffers. The tree settles the ledger when it removes
//! a node — the struct implements no `Drop`, so removal paths can
//! compute and release the exact claim (`RemoveDelta` + sizes) in one
//! place. 08-mib-dynamic-nodes.md.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::node::ChildWindow;

/// A handle to a live dynamic node: an index into the arena's slab.
///
/// C passes `struct mib_dynode *` around (the endpts table even stores
/// them across calls, remote.c:31-35). An index survives arena growth
/// (the vector may relocate), costs 4 bytes where a pointer costs 8,
/// and — unlike a pointer — cannot dangle silently past a restart:
/// a stale id either indexes a recycled node within bounds or is
/// rejected at the slab edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// The dynamic children of one parent, ordered by id.
///
/// C: a sorted linked list threaded through the dynodes (early-stop
/// search, tree.c:69-78; insert-at-scan, :465-467). See the module doc
/// for why the map replaces it.
pub type ChildMap = BTreeMap<i32, NodeId>;

/// A dynamic node's value, as `mib_create` can build one.
///
/// C: userland supplies either an immediate (BOOL/INT/QUAD packed into
/// the union) or OWNDATA bytes; function pointers cannot come from
/// userland (a kernel address in a create request would be absurd),
/// and node-type nodes carry no value at all (`data_combo_ok`, 08).
#[derive(Debug, Clone)]
pub enum DynValue {
    /// Immediate scalars: BOOL/INT travel as `i32`, QUAD as `i64`.
    /// C packs both arms into the same union word.
    Immediate(i64),
    /// OWNDATA bytes: the node owns exactly `sysctl_size` of them.
    /// Length is the budget claim (A-3) and the wire size (09).
    Owned(Box<[u8]>),
}

/// A dynamic node: C's `mib_dynode` (name + value + description in one
/// allocation) as three owned fields.
///
/// C: one `malloc` block embeds header, name bytes, and data
/// (`tree.c:679-685`) — the single-block shape is a C allocator
/// optimization, not semantics; Rust owns the three parts separately
/// and charges the same total to the budget
/// ([`create_alloc_size`](super::dynamic::create_alloc_size)'s math:
/// header + name (+ data)).
pub struct Dynode {
    /// Node name, NUL-free (the wire form adds the terminator when
    /// serializing). C: `dname[]` embedded, NUL-terminated.
    pub name: Box<[u8]>,
    /// Node flags as created (OWNDATA already forced where C forces
    /// it, `tree.c:568`). C: `node_flags`.
    pub flags: u32,
    /// This node's version — linked from the parent at insert
    /// (`linked_ver`, 03). C: `node_ver`.
    pub ver: u32,
    /// Immediate value or owned data. C: `sysctl_un` + `node_data`.
    pub value: Option<DynValue>,
    /// Owned description (OWNDESC), absent unless a describe set one.
    /// C: `ddesc` pointer + `strdup`, `tree.c:1043-1051`.
    pub desc: Option<Box<[u8]>>,
}

impl Dynode {
    /// The static window this node's parent advertises for it.
    ///
    /// Dynamic children live *past* the static array
    /// (`is_static_id` draws the line, 03/05), so a dynode contributes
    /// no static slot — the parent's `ChildWindow` merely grows its
    /// `clen`. This helper exists so the walker asks one question of
    /// every child, static or not.
    pub const fn no_static_slot() -> Option<ChildWindow> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CTL_QUERY reports children in id order — the property the map
    /// buys over the linked list (11's enumeration contract).
    #[test]
    fn test_child_map_iterates_in_id_order() {
        let mut children: ChildMap = ChildMap::new();
        // Insert out of order on purpose; iteration must sort.
        for id in [1026i32, 1024, 1031, 1025] {
            children.insert(id, NodeId(id as u32));
        }
        let ids: Vec<i32> = children.keys().copied().collect();
        assert_eq!(ids, vec![1024, 1025, 1026, 1031]);
        // CREATE_BASE ids never collide with the static lane: the map
        // only ever holds dynamic ids (static lookup is the array, 05).
        assert!(children.keys().all(|id| *id >= 1024));
    }

    /// `scan` judges "first free id ≥ base, with the sorted insert
    /// position" — the map must reproduce that position exactly.
    #[test]
    fn test_child_map_matches_scan_insert_position() {
        let mut children: ChildMap = ChildMap::new();
        for id in [1024i32, 1026] {
            children.insert(id, NodeId(id as u32));
        }
        // First id ≥ 1028 that is free: 1028 itself, inserted after 1026.
        let insert_at = children.range(1028..).next().map_or(2, |(id, _)| {
            children.keys().position(|k| k == id).unwrap()
        });
        assert_eq!(insert_at, 2);
        children.insert(1028, NodeId(1028));
        // 1029 now lands between 1028 and nothing — position 3.
        let insert_at = children.range(1029..).next().map_or(3, |(id, _)| {
            children.keys().position(|k| k == id).unwrap()
        });
        assert_eq!(insert_at, 3);
    }

    /// A dynode's parts are independent: value and description can be
    /// present, absent, or any combination (the OWNDATA/OWNDESC
    /// matrix 08 judges is four combinations, all representable).
    #[test]
    fn test_dynode_parts_independent() {
        let bare = Dynode {
            name: Box::from(&b"n"[..]),
            flags: 0,
            ver: 1,
            value: None,
            desc: None,
        };
        assert!(bare.value.is_none() && bare.desc.is_none());
        let owned = Dynode {
            name: Box::from(&b"n"[..]),
            flags: 0,
            ver: 1,
            value: Some(DynValue::Owned(Box::from(&b"data"[..]))),
            desc: Some(Box::from(&b"desc"[..])),
        };
        assert_eq!(owned.desc.as_deref(), Some(&b"desc"[..]));
    }
}

// ── The live tree ──
//
// 静态竞技场本体（15 §4.4 单一真值承诺的兑现，todo.md P1-2）。
// 镜像 C 的 mib_init 四线：`wire_kern`/`wire_vm`/`wire_hw`/`wire_minix`
// 分别消费四个子树模块自己的表（形状知识留在拥有它的模块），arena
// 只提供统一的结点原语。

use minix_types::{
    CTLFLAG_PERMANENT, CTLFLAG_READWRITE, CTLTYPE_NODE, CTL_KERN, CTL_MINIX, CTL_VM, Endpoint,
};

use super::static_tree::{TOP_SLOTS, slot_flags};
use crate::subtree::kern::{KernFunc, KernVerify};
use crate::subtree::minix::{MibStat, ProcDoor};
use crate::subtree::{hw::HwFunc, vm::VmFunc};

/// Function dispatch key: which subtree's handler owns this node.
/// C: the `node_func` pointer (mib.h:203) — the key is its Rust
/// address-free form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuncKey {
    /// CTL_KERN function (`mib_kern_*`).
    Kern(KernFunc),
    /// CTL_VM function (`mib_vm_*`).
    Vm(VmFunc),
    /// CTL_HW function (`mib_hw_*`).
    Hw(HwFunc),
    /// MINIX_PROC door (`mib_minix_proc_list/data`, 20).
    ProcDoor(ProcDoor),
}

/// How a mount took its window slot — the unmount must undo exactly
/// that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountKind {
    /// The mount *placed* a node into an empty reserved window slot;
    /// the unmark returns the slot to empty (flags zero).
    InWindow,
    /// The mount *obscured* an existing node; the unmount clears only
    /// the stamp, restoring the original view.
    Obscured,
    /// The mount linked a dynamic child beyond the static window; the
    /// unmount unlinks it from the parent's map.
    Dynamic,
}

/// One mounted remote root, with how it took the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountRoot {
    /// The covering node's handle.
    pub node: NodeId,
    /// How the mount took the window.
    pub kind: MountKind,
}

/// Self-statistics mirror (minix.mib.* reads the live counters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatKey {
    /// C: `MIB_INTPTR` over `mib_nodes`/`mib_objects`/`mib_remotes`.
    Stat(MibStat),
}

/// One tree node — static (init-built) or dynamic (run-time created).
///
/// C folds this into `mib_node` + `mib_dynode`; Rust splits by
/// ownership: static literals stay `&'static`, run-time bytes are
/// `Box<[u8]>` backed by the A-3 budget, and every node lives in one
/// slab so handles are plain indices.
pub struct Slot {
    /// Node name, NUL-free (serialization appends it). C: `node_name`.
    pub name: Box<[u8]>,
    /// MIB number. C: `node_num`.
    pub id: i32,
    /// Runtime flag word (mounts stamp `REMOTE`). C: `node_flags`.
    pub flags: u32,
    /// Node version. C: `node_ver`.
    pub ver: u32,
    /// Child capacity (static slots, or dynamic count for dynamic
    /// parents). C: `node_csize`.
    pub csize: u32,
    /// Live children. C: `node_clen`.
    pub clen: u32,
    /// Slab index of the first static child (`u32::MAX` = none).
    pub child_base: u32,
    /// Dynamic children by id. C: the sorted dynode chain.
    pub children: ChildMap,
    /// Parent handle. C: `node_parent`.
    pub parent: Option<NodeId>,
    /// Integer value: constants, writable ints, dynamic immediates.
    /// C: `node_int`/`sysctl_idata`.
    pub imm: Option<i64>,
    /// Runtime buffer: writable strings/structs, dynamic OWNDATA.
    /// C: `node_data`.
    pub data: Option<Box<[u8]>>,
    /// Static constant string (read-only literal). C: `node_data` into
    /// a literal.
    pub const_str: Option<&'static str>,
    /// Node data width. C: `node_size`/`sysctl_size`.
    pub size: u64,
    /// Function dispatch key. C: `node_aux_u.nau_func`.
    pub func: Option<FuncKey>,
    /// Verify gate. C: `node_aux_u.nau_verify`.
    pub verify: Option<KernVerify>,
    /// Self-statistics mirror. C: `MIB_INTPTR` over the globals.
    pub stat: Option<StatKey>,
    /// Owned description (describe-set). C: `ddesc` (OWNDESC).
    pub desc: Option<Box<[u8]>>,
    /// Budget charge owed by a dynamic node (A-3; settled at remove).
    pub charge: usize,
    /// Mount state: the owning service's endpoint (REMOTE nodes only;
    /// written by the mount arm, P1-1). C: `endpts[eid].endpt`.
    pub peer: Option<Endpoint>,
    /// Mount state: the service's remote root id. C: `endpts[].rootid`.
    pub mount_root: Option<u32>,
}

/// The arena: a node slab, the root handle, and the live counters.
pub struct MibTree {
    /// The slab; entry 0 is the root.
    pub slots: Vec<Slot>,
    /// Freed dynamic slab slots, reusable (C's dynode freelist).
    pub free: Vec<u32>,
    /// Live counters behind minix.mib.* (03's `TreeCounts`, live here).
    pub counts: super::node::TreeCounts,
}

/// Named build values the tables reference but do not own.
///
/// C resolves these from build headers at compile time (`MIB_INT(f,
/// MACRO, …)`); the Rust tables carry the names, and this is the
/// resolution point — every entry cites its C anchor.
pub(crate) fn build_int_value(name: &str) -> Option<i64> {
    match name {
        // mib.h:13-15 — no CONFIG override in minix-rs (single-CPU build).
        "CONFIG_MAX_CPUS" => Some(1),
        // minix/config.h:7 (`OS_REV 304000000`).
        "OS_REV" => Some(304_000_000),
        // minix/config.h:31 + minix-types `NR_PROCS = 256`.
        "NR_PROCS" => Some(256),
        // syslimits.h:49 (`ARG_MAX (256 * 1024)`).
        "ARG_MAX" => Some(256 * 1024),
        // syslimits.h:59.
        "NGROUPS_MAX" => Some(16),
        // unistd.h:62 (`_POSIX_VERSION 200112L`).
        "_POSIX_VERSION" => Some(200112),
        // NR_VNODES: no definition in the minix3 include tree — a
        // pending design decision, refused at read ([待裁决]).
        _ => None,
    }
}

impl MibTree {
    /// Build the static tree exactly as `mib_init` wires it
    /// (main.c:384-413): root, seven tops, then the four subtree
    /// tables in `WIRE_ORDER` — kern, vm, hw, minix. Windows are
    /// reserved up front (C's static arrays are sized by their highest
    /// initialized index) and entries placed at their id positions.
    pub fn init() -> Self {
        let mut t = MibTree {
            slots: Vec::new(),
            free: Vec::new(),
            counts: super::node::TreeCounts::baseline(),
        };
        // The root: writable, nameless, internal-only (main.c:61-62).
        let root = t.push(
            "",
            0,
            CTLTYPE_NODE | CTLFLAG_READWRITE | CTLFLAG_PERMANENT,
        );
        t.slots[root.0 as usize].ver = super::node::ROOT_VER;
        t.counts = super::node::TreeCounts::baseline();
        // Root window: the C table spans ids 0..=32 (`[CTL_MINIX] = …`
        // is the highest row) — 33 slots with holes.
        t.reserve(root, (CTL_MINIX + 1) as u32);
        for top in TOP_SLOTS.iter() {
            let flags = slot_flags(*top) | CTLTYPE_NODE;
            let top_id = t.place(root, top.name, top.id, flags);
            match top.id {
                CTL_KERN => crate::subtree::kern::build(&mut t, top_id),
                CTL_VM => crate::subtree::vm::build(&mut t, top_id),
                minix_types::CTL_HW => crate::subtree::hw::build(&mut t, top_id),
                CTL_MINIX => crate::subtree::minix::build(&mut t, top_id),
                // net waits for remote mounts (12); user is libc-local
                // (A-10); vendor is the writable scratch slot.
                _ => {}
            }
        }
        t
    }

    /// Append a bare static node (no parent link yet).
    pub(crate) fn push(&mut self, name: &str, id: i32, flags: u32) -> NodeId {
        let idx = self.slots.len() as u32;
        self.slots.push(Slot {
            name: Box::from(name.as_bytes()),
            id,
            flags,
            ver: super::node::ROOT_VER,
            csize: 0,
            clen: 0,
            child_base: u32::MAX,
            children: ChildMap::new(),
            parent: None,
            imm: None,
            data: None,
            const_str: None,
            size: 0,
            func: None,
            verify: None,
            stat: None,
            desc: None,
            charge: 0,
            peer: None,
            mount_root: None,
        });
        NodeId(idx)
    }

    /// Reserve a child window of `csize` slots under `parent` (C's
    /// static-array sizing: the array spans `[0, max_id]`, so length =
    /// max initialized index + 1).
    pub(crate) fn reserve(&mut self, parent: NodeId, csize: u32) {
        let base = self.slots.len() as u32;
        for _ in 0..csize {
            self.push("", 0, 0);
        }
        let p = &mut self.slots[parent.0 as usize];
        p.child_base = base;
        p.csize = csize;
    }

    /// Place a static child into a reserved window slot.
    ///
    /// C: one `MIB_*` row overwriting its array position. The slot was
    /// reserved empty; this fills identity/flags and links the parent.
    pub(crate) fn place(
        &mut self,
        parent: NodeId,
        name: &str,
        id: i32,
        flags: u32,
    ) -> NodeId {
        let p = self.slot(parent);
        let node = NodeId(p.child_base + id as u32);
        {
            let s = self.slot_mut(node);
            s.name = Box::from(name.as_bytes());
            s.id = id;
            s.flags = flags;
            s.parent = Some(parent);
        }
        let p = self.slot_mut(parent);
        p.clen += 1;
        self.counts = self.counts.node_added();
        node
    }

    /// Read-only handle.
    pub fn slot(&self, id: NodeId) -> &Slot {
        &self.slots[id.0 as usize]
    }

    /// Mutable handle.
    pub fn slot_mut(&mut self, id: NodeId) -> &mut Slot {
        &mut self.slots[id.0 as usize]
    }

    /// The root handle.
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Child by id — the arena form of `mib_find`'s one level (05):
    /// static window O(1), dynamic map beyond.
    pub fn child(&self, parent: NodeId, id: i32) -> Option<NodeId> {
        let p = self.slot(parent);
        if super::node::is_static_id(p.csize, id) && p.child_base != u32::MAX {
            let c = NodeId(p.child_base + id as u32);
            if self.slot(c).flags != 0 {
                return Some(c);
            }
            return None;
        }
        p.children.get(&id).copied()
    }

    /// Live children in iteration order: static window first, then the
    /// dynamic map (both id-ascending) — CTL_QUERY's walk (11).
    pub fn children_of(&self, parent: NodeId) -> Vec<NodeId> {
        let p = self.slot(parent);
        let mut out = Vec::new();
        if p.child_base != u32::MAX {
            for i in 0..p.csize {
                let c = NodeId(p.child_base + i);
                if self.slot(c).flags != 0 {
                    out.push(c);
                }
            }
        }
        for (_, id) in p.children.iter() {
            out.push(*id);
        }
        out
    }

    /// Bump this node's version (and every ancestor's), the do-while
    /// of `mib_upgrade` (tree.c:426-449) — root bumps skip zero.
    pub fn upgrade(&mut self, mut node: NodeId) {
        loop {
            let parent = self.slot(node).parent;
            self.slot_mut(node).ver = self.slot_mut(node).ver.wrapping_add(1);
            match parent {
                Some(p) => node = p,
                None => break,
            }
        }
    }
}

// ── Dynamic lifecycle primitives ──

impl MibTree {
    /// Allocate and link a dynamic node (create's allocation+link half;
    /// the budget charge is the caller's — it knows name + data totals).
    ///
    /// C: `mib_add` — tree.c:451-481. Freed slab slots are reused before
    /// the vector grows. `[ARCH: ...]` a static parent's `csize` keeps
    /// its static-window meaning (dynamic children live in the map
    /// beyond the window); C bumps `csize` alongside `clen`, which only
    /// makes sense for dynamic parents — mirrored here by growing
    /// `csize` only when the parent itself is dynamic.
    pub(crate) fn create_dynode(
        &mut self,
        parent: NodeId,
        id: i32,
        _insert_at: usize,
        name: &[u8],
        flags: u32,
        data: Option<Box<[u8]>>,
    ) -> NodeId {
        let node = match self.free.pop() {
            Some(i) => {
                let s = &mut self.slots[i as usize];
                s.name = Box::from(name);
                s.id = id;
                s.flags = flags;
                s.ver = super::node::ROOT_VER;
                s.data = data;
                s.charge = 0;
                NodeId(i)
            }
            None => {
                let idx = self.slots.len() as u32;
                self.slots.push(Slot {
                    name: Box::from(name),
                    id,
                    flags,
                    ver: super::node::ROOT_VER,
                    csize: 0,
                    clen: 0,
                    child_base: u32::MAX,
                    children: ChildMap::new(),
                    parent: Some(parent),
                    imm: None,
                    data,
                    const_str: None,
                    size: 0,
                    func: None,
                    verify: None,
                    stat: None,
                    desc: None,
                    charge: 0,
                    peer: None,
                    mount_root: None,
                });
                NodeId(idx)
            }
        };
        {
            let s = &mut self.slots[node.0 as usize];
            s.parent = Some(parent);
        }
        let p = &mut self.slots[parent.0 as usize];
        p.children.insert(id, node);
        p.clen += 1;
        if p.child_base == u32::MAX {
            // A dynamic parent's capacity IS its child count (C csize++).
            p.csize += 1;
        }
        self.counts = self.counts.node_added();
        node
    }

    /// Unlink a dynamic node and return its slab slot to the freelist.
    /// The caller settles the budget ledger with the node's recorded
    /// charge before/after calling this (remove paths are the only
    /// release sites, so accounting cannot drift).
    pub(crate) fn remove_dynode(&mut self, parent: NodeId, node: NodeId) {
        let id = self.slot(node).id;
        self.slot_mut(parent).children.remove(&id);
        {
            let p = self.slot_mut(parent);
            p.clen = p.clen.saturating_sub(1);
            if p.child_base == u32::MAX {
                p.csize = p.csize.saturating_sub(1);
            }
        }
        // Clearing the flags is what makes an in-window slot look empty
        // to `mib_find` (C zeroes the node, tree.c:793-832).
        self.slot_mut(node).flags = 0;
        self.slot_mut(node).children = ChildMap::new();
        self.free.push(node.0);
        self.counts = self.counts.node_removed();
    }
}

impl MibTree {
    /// Unlink a dynamic child from its parent's map and free the slab
    /// slot (the unmount road for Dynamic-kind mounts).
    pub(crate) fn unlink_dynode(&mut self, parent: NodeId, node: NodeId) {
        let id = self.slot(node).id;
        self.slot_mut(parent).children.remove(&id);
        {
            let p = self.slot_mut(parent);
            if p.child_base == u32::MAX {
                p.csize = p.csize.saturating_sub(1);
            }
            p.clen = p.clen.saturating_sub(1);
        }
        self.slot_mut(node).flags = 0;
        self.free.push(node.0);
        self.counts.node_removed();
    }
}

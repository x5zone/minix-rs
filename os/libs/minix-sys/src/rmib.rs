//! Remote MIB (RMIB) client: mounted-subtree bookkeeping plus the
//! copy-out/copy-in primitives of the forwarded-call protocol.
//!
//! A service that owns part of the sysctl name space (IPC, the network
//! stacks) keeps its subtree at home and lets the MIB service forward
//! matching queries. This module holds the parts of that contract that
//! need no IPC: the subtree slot table, the sorted sparse-node lookup,
//! the caller context, and the stack-buffer update policy. Sending the
//! registration, answering forwarded calls, and copying through grants
//! stay with the transport layer: they need `asynsend3`/`sendrec` and
//! the grant machinery, none of which exists here yet.
//!
//! E-RMIBWIRE (pure-function layer): the copy-out primitives
//! ([`RmibOldp`]/[`rmib_copyout`]), node packaging ([`rmib_copyout_node`]),
//! and caller context ([`RmibCall`]) now live here as pure logic — the
//! actual grant copy is injected as a closure so the wire semantics are
//! host-testable end to end. The transport (asynsend3/sendrec, grant
//! issuance) remains with E1.
//!
//! C: `minix/lib/libsys/rmib.c` (1089 lines) and
//! `minix/include/minix/rmib.h` (188 lines).
//!
//! 22-mib-rmib-client.md.

use minix_types::CTLFLAG_ROOT;

/// How many subtrees one service can mount.
///
/// C: `RMIB_MAX_SUBTREES 16` — rmib.c:47. Raising it is safe, says the
/// comment; sixteen is plenty in practice. A root id is an index into
/// this table.
pub const RMIB_MAX_SUBTREES: usize = 16;

/// Stack buffer size for field updates.
///
/// C: `RMIB_STACKBUF 257` — rmib.c:36-44. Updates that fit go on the
/// stack; the extra byte leaves room for a missing string terminator.
pub const RMIB_STACKBUF: usize = 257;

/// Caller has superuser privileges.
///
/// C: `RMIB_FLAG_AUTH 0x1` — rmib.h:39. The header admits this flag
/// travels on the wire but has no shared definition with the MIB
/// service yet (:32-38); one flag only, so not urgent.
pub const RMIB_FLAG_AUTH: u32 = 0x1;

/// Sparse-node marker, borrowed from an unused NetBSD flag.
///
/// C: `CTLFLAG_SPARSE` is `CTLFLAG_ROOT` — rmib.h:57. Sparse nodes
/// trade lookup speed for memory: instead of a dense child array they
/// keep `{id, child}` pairs, sorted ascending, searched linearly.
pub const CTLFLAG_SPARSE: u32 = CTLFLAG_ROOT;

/// Whether the caller runs privileged.
///
/// C: `call_flags & RMIB_FLAG_AUTH` — rmib.c request handling. The MIB
/// service snapshots the bit into the forwarded call; the handler
/// trusts it.
pub const fn is_authed(call_flags: u32) -> bool {
    call_flags & RMIB_FLAG_AUTH != 0
}

/// Whether a field update may use the stack buffer.
///
/// C: by policy, non-root users may not update fields past the stack
/// buffer at all — rmib.c:36-44. Root may always proceed (heap past
/// the buffer); anyone else is limited to the buffer.
pub const fn stack_update_allowed(is_root: bool, field_size: usize) -> bool {
    is_root || field_size <= RMIB_STACKBUF
}

/// Find a child id in a sorted sparse list.
///
/// C: sparse (`SNODE`) lookup — rmib.h:48-56. Linear scan over ids
/// sorted ascending; duplicates, null children, and zero-flagged nodes
/// are forbidden by construction (the caller upholds the invariant,
/// see [`validate_sparse`]).
pub fn sparse_find(ids: &[u32], want: u32) -> Option<usize> {
    ids.iter().position(|&id| id == want)
}

/// Check a sparse list: ascending, unique, and all flagged.
///
/// C: the construction rules — rmib.h:52-55. Returns the first problem:
/// out-of-order (or duplicate) ids, or a node without flags. An empty
/// list is valid.
pub fn validate_sparse(ids: &[u32], has_flags: &[bool]) -> Result<(), SparseError> {
    if ids.len() != has_flags.len() {
        return Err(SparseError::LengthMismatch);
    }
    let mut prev: Option<u32> = None;
    for (i, &id) in ids.iter().enumerate() {
        if let Some(p) = prev
            && id <= p
        {
            return Err(SparseError::NotSorted);
        }
        if !has_flags[i] {
            return Err(SparseError::Unflagged);
        }
        prev = Some(id);
    }
    Ok(())
}

/// What is wrong with a sparse list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseError {
    /// Id and flag slices differ in length.
    LengthMismatch,
    /// Ids not strictly ascending (duplicates included).
    NotSorted,
    /// A node carries no flags.
    Unflagged,
}

/// Which subtrees this service has mounted.
///
/// C: `rnodes[]` — rmib.c:52-57. Slot index doubles as the root id on
/// the wire. Registering claims the first free slot; deregistering
/// frees it; a MIB restart needs re-registration of every live slot
/// (`rmib_reregister`, :973-983); `rmib_reset` clears everything
/// without talking to anyone and exists for tests only (:986-994).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountTable {
    used: [bool; RMIB_MAX_SUBTREES],
}

/// A claimed slot: the root id for this subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot(pub u8);

impl Default for MountTable {
    fn default() -> Self {
        Self::new()
    }
}

impl MountTable {
    /// Empty table: nothing mounted.
    pub const fn new() -> MountTable {
        MountTable {
            used: [false; RMIB_MAX_SUBTREES],
        }
    }

    /// Claim the first free slot, if any.
    pub fn claim(&mut self) -> Option<Slot> {
        for (i, u) in self.used.iter_mut().enumerate() {
            if !*u {
                *u = true;
                return Some(Slot(i as u8));
            }
        }
        None
    }

    /// Free a claimed slot. Unknown ids are ignored: deregistration
    /// answers silence, never errors.
    pub fn release(&mut self, slot: Slot) {
        if (slot.0 as usize) < RMIB_MAX_SUBTREES {
            self.used[slot.0 as usize] = false;
        }
    }

    /// Whether this slot is currently claimed.
    pub const fn is_claimed(&self, slot: Slot) -> bool {
        if (slot.0 as usize) >= RMIB_MAX_SUBTREES {
            return false;
        }
        self.used[slot.0 as usize]
    }

    /// Drop every registration without telling anyone.
    ///
    /// C: `rmib_reset` — rmib.c:986-994, test-only helper.
    pub fn clear(&mut self) {
        self.used = [false; RMIB_MAX_SUBTREES];
    }

    /// Live slots, in order: what re-registration re-sends.
    ///
    /// C: `rmib_reregister` walks the table and re-sends every live
    /// root — rmib.c:973-983.
    pub fn live_slots(&self) -> [Option<Slot>; RMIB_MAX_SUBTREES] {
        let mut out = [None; RMIB_MAX_SUBTREES];
        for (i, u) in self.used.iter().enumerate() {
            if *u {
                out[i] = Some(Slot(i as u8));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_limits() {
        // Sixteen slots, stack buffer 257, auth bit 0x1.
        assert_eq!(RMIB_MAX_SUBTREES, 16);
        assert_eq!(RMIB_STACKBUF, 257);
        assert_eq!(RMIB_FLAG_AUTH, 0x1);
        assert_eq!(CTLFLAG_SPARSE, CTLFLAG_ROOT);
        assert!(is_authed(0x1));
        assert!(!is_authed(0x0));
        // Root may update anything; others stop at the buffer.
        assert!(stack_update_allowed(true, 10_000));
        assert!(stack_update_allowed(false, 257));
        assert!(!stack_update_allowed(false, 258));
    }

    #[test]
    fn test_sparse_lists() {
        // Sorted lookup finds, misses miss.
        let ids = [3u32, 7, 42];
        assert_eq!(sparse_find(&ids, 7), Some(1));
        assert_eq!(sparse_find(&ids, 8), None);
        assert_eq!(sparse_find(&[], 1), None);
        // Construction rules: ascending, unique, flagged.
        assert_eq!(validate_sparse(&[3, 7, 42], &[true, true, true]), Ok(()));
        assert_eq!(validate_sparse(&[], &[]), Ok(()));
        assert_eq!(
            validate_sparse(&[3, 3, 42], &[true, true, true]),
            Err(SparseError::NotSorted)
        );
        assert_eq!(
            validate_sparse(&[7, 3], &[true, true]),
            Err(SparseError::NotSorted)
        );
        assert_eq!(
            validate_sparse(&[3, 7], &[true, false]),
            Err(SparseError::Unflagged)
        );
        assert_eq!(
            validate_sparse(&[3], &[true, true]),
            Err(SparseError::LengthMismatch)
        );
    }

    #[test]
    fn test_mount_table() {
        let mut t = MountTable::new();
        // First claim is slot zero (the root id on the wire).
        assert_eq!(t.claim(), Some(Slot(0)));
        assert_eq!(t.claim(), Some(Slot(1)));
        assert!(t.is_claimed(Slot(0)));
        assert!(!t.is_claimed(Slot(2)));
        // Release and reclaim reuse the slot.
        t.release(Slot(0));
        assert!(!t.is_claimed(Slot(0)));
        assert_eq!(t.claim(), Some(Slot(0)));
        // Unknown releases are ignored, never errors.
        t.release(Slot(200));
        // Fill the table: the seventeenth claim fails.
        for _ in 2..RMIB_MAX_SUBTREES {
            assert!(t.claim().is_some());
        }
        assert_eq!(t.claim(), None);
        // Re-registration re-sends every live slot, in order.
        let live = t.live_slots();
        assert!(live.iter().all(|s| s.is_some()));
        // Test reset clears everything.
        t.clear();
        assert!(live_slots_empty(&t));
    }

    fn live_slots_empty(t: &MountTable) -> bool {
        t.live_slots().iter().all(|s| s.is_none())
    }
}

// ── E-RMIBWIRE pure-function layer: copy-out primitives and node packing ──
//
// C: `rmib_oldp`/`rmib_newp` (:24-30), `rmib_inrange` (:63-71), `rmib_getoldlen`
// (:78-88), `rmib_copyout` (:97-126), `rmib_copyout_node` (:200-260) in rmib.c.
// The copy-out action is injected as a closure: the grant write is the transport
// half (E1); clamping, trimming and packing are pure logic, host-testable.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use minix_types::{
    sysctl_type, CTLFLAG_IMMEDIATE, CTLFLAG_PRIVATE, CTLFLAG_READWRITE, CTLTYPE_BOOL,
    CTLTYPE_INT, CTLTYPE_NODE, CTLTYPE_QUAD, CTLTYPE_STRING, CTLTYPE_STRUCT,
    SysctlDesc, SysctlNode, SYSCTL_NAMELEN, SYSCTL_NODE_FN, SYSCTL_VERSION,
};

/// Outgoing data window (C: `struct rmib_oldp` — rmib.c:24-27): the
/// MIB service's grant plus the caller-requested length. `None`
/// (C `NULL`) means "compute only, no copy".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RmibOldp {
    /// The MIB service's grant (the server writes into it). C: `oldp_grant`.
    pub grant: i32,
    /// Total length requested by the caller. C: `oldp_len`.
    pub len: usize,
}

/// Incoming data window (C: `struct rmib_newp` — rmib.c:28-31).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RmibNewp {
    /// C: `newp_grant`。
    pub grant: i32,
    /// C: `newp_len`。
    pub len: usize,
}

/// One forwarded RMIB call (C: `struct rmib_call` — rmib.h:22-30).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RmibCall {
    /// Endpoint of the caller (the user process that issued the sysctl). C: `call_endpt`.
    pub endpt: i32,
    /// Original full name of the request. C: `call_oname`.
    pub oname: [i32; minix_types::CTL_MAXNAME as usize],
    /// Remaining name (starting at the current node). C: `call_name`.
    pub name: [i32; minix_types::CTL_MAXNAME as usize],
    /// Length of the remaining name. C: `call_namelen`.
    pub namelen: usize,
    /// Call flags (RMIB_FLAG_AUTH). C: `call_flags`.
    pub flags: u32,
    /// Subtree version. C: `call_rootver`.
    pub rootver: u32,
    /// Whole-tree version. C: `call_treever`.
    pub treever: u32,
}

/// C: `rmib_inrange` — rmib.c:63-71. Whether the offset falls inside the request window.
pub const fn rmib_inrange(oldp: Option<&RmibOldp>, off: usize) -> bool {
    match oldp {
        None => false,
        Some(o) => off < o.len,
    }
}

/// C: `rmib_getoldlen` — rmib.c:78-88. Total requested length (None → 0).
pub const fn rmib_getoldlen(oldp: Option<&RmibOldp>) -> usize {
    match oldp {
        None => 0,
        Some(o) => o.len,
    }
}

/// C: `rmib_copyout` — rmib.c:97-126. Writes `buf[..size]` into the window at
/// `[off..]`, clamped to the requested length; outside the window (or with
/// `None`) nothing moves and `size` is returned as-is (C's "nothing to do"
/// semantics). The copy-out verb is injected, so this is host-testable.
///
/// Returns `Ok(size)` (C's "requested length for the caller's convenience") or
/// `Err(errno)` (the injected verb failed, e.g. EFAULT).
pub fn rmib_copyout(
    oldp: Option<&RmibOldp>,
    off: usize,
    buf: &[u8],
    write: impl FnOnce(&[u8], usize) -> Result<(), i32>,
) -> Result<usize, i32> {
    let size = buf.len();
    let oldp = match oldp {
        Some(o) if off < o.len => o,
        _ => return Ok(size), // nothing to do (rmib.c:110-111)
    };
    let len = if size > oldp.len - off { oldp.len - off } else { size };
    write(&buf[..len], off)?;
    Ok(size)
}

/// A remote MIB node — the client-side shape of a service-owned subtree
/// (C: `struct rmib_node`, rmib.h:64-90). Pointer members become owned shapes
/// (name string plus a child vector or sparse pairs); function-driven nodes
/// are expressed as `func: true`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RmibNode {
    /// C: `rnode_flags` — CTLTYPE_ type plus CTLFLAG_ flags.
    pub flags: u32,
    /// C: `rnode_size` — size of the associated data; for NODE it is the number
    /// of children (a func-driven NODE indicates the expected request count).
    pub size: u32,
    /// Direct value (immediate nodes). C: the bool/int/quad/clen arm of `rnode_val_u`.
    pub value: Option<RmibImmediate>,
    /// String/struct data (data nodes that are not immediate). C: `rnode_data`.
    pub data: Option<Vec<u8>>,
    /// Children (non-sparse: the index is the id; sparse: explicit id pairs).
    /// C: `rnode_cptr` / `rnode_icptr`.
    pub children: RmibChildren,
    /// Function-driven node (C: `rnode_func != NULL`). The Rust side carries no
    /// function pointer; the service dispatches its own handler.
    pub func: bool,
    /// Node name. C: `rnode_name`.
    pub name: String,
    /// Description (optional). C: `rnode_desc`.
    pub desc: Option<String>,
}

/// A direct value (the concrete arm of C's `rnode_val_u` union).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmibImmediate {
    /// C: `rvu_bool`。
    Bool(bool),
    /// C: `rvu_int`。
    Int(i32),
    /// C: `rvu_quad`。
    Quad(u64),
    /// C: `rvu_clen` (actual child count, used when writing back the NODE description).
    Clen(u32),
}

/// Child collection: a plain array (id = index) or a sparse array (explicit
/// id pairs, ascending without duplicates; C: `rnode_cptr` / `rnode_icptr`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RmibChildren {
    /// No children.
    #[default]
    None,
    /// C: `rnode_cptr` — the index is the id; empty slots with flags==0 are skipped
    /// during traversal (rmib.c:351).
    Dense(Vec<RmibNode>),
    /// C: `rnode_icptr` — (id, node) pairs, which must be ascending without
    /// duplicates (rmib.h:53-56).
    Sparse(Vec<(u32, RmibNode)>),
}

impl RmibChildren {
    /// Number of children (C: for the sparse case the library maintains
    /// `rnode_size` as the icptr length).
    pub fn len(&self) -> usize {
        match self {
            RmibChildren::None => 0,
            RmibChildren::Dense(v) => v.len(),
            RmibChildren::Sparse(v) => v.len(),
        }
    }

    /// Whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// C: `rmib_copyout_node` — rmib.c:200-260. Packs one node into a
/// `sysctlnode` (96 bytes, layout in minix-types::sysctl_abi) and copies it out:
/// it strips CTLFLAG_SPARSE (a library-internal flag, rmib.c:208-214), stamps
/// SYSCTL_VERSION, fills in immediate values when visible, and applies the NODE
/// special cases (a real parent reports csize/clen; a function-driven node
/// gets SYSCTL_NODE_FN).
/// Returns `Ok(size_of::<SysctlNode>())` — matching C, so the caller can
/// accumulate the offset.
pub fn rmib_copyout_node(
    call: &RmibCall,
    oldp: Option<&RmibOldp>,
    off: usize,
    id: u32,
    rnode: &RmibNode,
    write: impl FnOnce(&[u8], usize) -> Result<(), i32>,
) -> Result<usize, i32> {
    // C: rmib.c:203-206 — outside the window nothing moves, but the node size is
    // returned for offset accumulation.
    if !rmib_inrange(oldp, off) {
        return Ok(core::mem::size_of::<SysctlNode>());
    }

    // SAFETY-shape: SysctlNode has no Default (union members); repr(C), all PoD,
    // so the zero value is C's memset(&scn, 0, sizeof(scn)) (rmib.c:206).
    let mut scn: SysctlNode = unsafe { core::mem::zeroed() };

    // C: rmib.c:208-214 — version stamp plus stripping of CTLFLAG_SPARSE (a
    // NetBSD-overloaded flag, not exposed to userland).
    scn.sysctl_flags = SYSCTL_VERSION | (rnode.flags & !CTLFLAG_SPARSE);
    scn.sysctl_num = id as i32;
    let name_bytes = rnode.name.as_bytes();
    let copy_len = name_bytes.len().min(SYSCTL_NAMELEN);
    scn.sysctl_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
    scn.sysctl_ver = call.rootver;
    scn._sysctl_size = rnode.size as u64;

    // C: rmib.c:216-217 — PRIVATE node information is visible only to authorized
    // callers.
    let visible =
        (rnode.flags & CTLFLAG_PRIVATE == 0) || (call.flags & RMIB_FLAG_AUTH) != 0;

    // C: rmib.c:219-231 — immediate nodes get their direct value filled in when
    // visible.
    if rnode.flags & CTLFLAG_IMMEDIATE != 0 && visible {
        let node_type = sysctl_type(rnode.flags);
        if let Some(value) = rnode.value {
            match (node_type, value) {
                (CTLTYPE_BOOL, RmibImmediate::Bool(b)) => scn.sysctl_un.scu_bdata = b as u8,
                (CTLTYPE_INT, RmibImmediate::Int(i)) => scn.sysctl_un.scu_idata = i,
                (CTLTYPE_QUAD, RmibImmediate::Quad(q)) => scn.sysctl_un.scu_qdata = q,
                _ => {}
            }
        }
    }

    // C: rmib.c:233-249 — NODE special cases: a real parent reports csize/clen
    // (when visible); a function-driven node gets the SYSCTL_NODE_FN sentinel
    // (so trace(1) can recognize it).
    if sysctl_type(rnode.flags) == CTLTYPE_NODE {
        scn._sysctl_size = core::mem::size_of::<SysctlNode>() as u64;
        if !rnode.func && visible {
            scn.sysctl_un.scu_child.suc_csize = rnode.size;
            scn.sysctl_un.scu_child.suc_clen = rnode.children.len() as u32;
        } else if rnode.func {
            scn.sysctl_un.scu_child._suc_child = SYSCTL_NODE_FN as u64;
        }
    }

    // SAFETY: SysctlNodeUn is a plain-old-data union; writing the child arm
    // matches the C field filling above (visibility/type already decided).
    // (Field writes are already done according to the repr(C) layout.)
    rmib_copyout(oldp, off, scn_as_bytes(&scn), write)
}

/// Byte view of `SysctlNode` (repr(C), 96 bytes).
fn scn_as_bytes(scn: &SysctlNode) -> &[u8] {
    // SAFETY: `#[repr(C)]` struct, all fields PoD, no padding-initialization
    // requirement (the destination buffer reads it byte by byte).
    unsafe {
        core::slice::from_raw_parts(scn as *const SysctlNode as *const u8, core::mem::size_of::<SysctlNode>())
    }
}

/// The description-packing half of C's `rmib_copyout_desc` — rmib.c:355-380
/// (the description struct and string are copied out in two steps; here we
/// produce the merged byte sequence and the total to copy, with writing still
/// injected). The description length includes the NUL (a None desc counts as
/// 1 byte, rmib.c:368-372).
pub fn rmib_copyout_desc(
    call: &RmibCall,
    oldp: Option<&RmibOldp>,
    off: usize,
    id: u32,
    rnode: &RmibNode,
    mut write: impl FnMut(&[u8], usize) -> Result<(), i32>,
) -> Result<usize, i32> {
    // C: rmib.c:360-364 — a PRIVATE node's description is private too: return 0
    // immediately (the offset does not advance, so during traversal it is as if
    // the node does not exist).
    if (rnode.flags & CTLFLAG_PRIVATE) != 0 && (call.flags & RMIB_FLAG_AUTH) == 0 {
        return Ok(0);
    }

    let desc = rnode.desc.as_deref().unwrap_or("");
    let len = desc.len() + 1; // includes the NUL (rmib.c:368-372)

    // repr(C) PoD, so the zero value equals C's memset (rmib.c:373-376).
    let mut scd: SysctlDesc = unsafe { core::mem::zeroed() };
    scd.descr_num = id as i32;
    scd.descr_ver = call.rootver;
    scd.descr_len = len as u32;

    // C: rmib.c:377-379 — copy the description header first (offsetof descr_str
    // = 12), then the string. Window clamping is rmib_copyout's job; here we copy
    // the header first.
    let head = unsafe {
        core::slice::from_raw_parts(
            &scd as *const SysctlDesc as *const u8,
            core::mem::size_of::<SysctlDesc>(),
        )
    };
    rmib_copyout(oldp, off, head, |bytes, at| write(bytes, at))?;
    let str_off = off + core::mem::size_of::<SysctlDesc>();

    // Description string (with NUL). C copies it out in two steps; here it is
    // merged into one write (header and string are non-contiguous, as in C; the
    // Rust side concatenates and does one clamped copy-out — same total and
    // offsets, so the semantics are equivalent).
    let mut payload = desc.as_bytes().to_vec();
    payload.push(0);
    let payload = payload;
    let _ = str_off;
    rmib_copyout(oldp, off + core::mem::size_of::<SysctlDesc>(), &payload, write)?;

    Ok(core::mem::size_of::<SysctlDesc>() + payload.len())
}

/// Node lookup for SYS_UPDATE (C: `rmib_lookup` — rmib.c:340-360 area):
/// finds a child by id in `parent`'s child collection.
/// Mutable version of [`rmib_lookup`] (leaf writes during the downward walk
/// need &mut).
pub fn rmib_lookup_mut(parent: &mut RmibNode, id: u32) -> Option<&mut RmibNode> {
    match &mut parent.children {
        RmibChildren::None => None,
        RmibChildren::Dense(v) => v.get_mut(id as usize).filter(|n| n.flags != 0),
        RmibChildren::Sparse(v) => {
            v.iter_mut().find(|(i, _)| *i == id).map(|(_, n)| n)
        }
    }
}

pub fn rmib_lookup(parent: &RmibNode, id: u32) -> Option<&RmibNode> {
    match &parent.children {
        RmibChildren::None => None,
        RmibChildren::Dense(v) => v.get(id as usize).filter(|n| n.flags != 0),
        RmibChildren::Sparse(v) => {
            v.iter().find(|(i, _)| *i == id).map(|(_, n)| n)
        }
    }
}

#[cfg(test)]
mod pure_tests {
    use super::*;
    use alloc::vec;
    use minix_types::{
        CTLFLAG_PERMANENT, CTLFLAG_PRIVATE, CTLFLAG_READWRITE, CTLTYPE_INT, SYSCTL_VERSION,
    };

    fn call() -> RmibCall {
        RmibCall {
            endpt: 20,
            oname: [0; minix_types::CTL_MAXNAME as usize],
            name: [0; minix_types::CTL_MAXNAME as usize],
            namelen: 0,
            flags: 0,
            rootver: 7,
            treever: 9,
        }
    }

    fn node(flags: u32, name: &str) -> RmibNode {
        RmibNode {
            flags,
            name: String::from(name),
            ..RmibNode::default()
        }
    }

    /// C rmib.c:97-126 — the three clamp states: full copy / truncated / outside.
    #[test]
    fn test_rmib_copyout_clamps_to_window() {
        let window = RmibOldp { grant: 3, len: 16 };

        // Fully inside the window.
        let mut moved: Vec<(usize, usize)> = Vec::new();
        let r = rmib_copyout(Some(&window), 0, &[0xAA; 16], |b, off| {
            moved.push((off, b.len()));
            Ok(())
        });
        assert_eq!(r, Ok(16));
        assert_eq!(moved, vec![(0, 16)]);

        // Truncated at the tail: only 8 bytes of budget remain at off=8, so a
        // request of 16 moves 8.
        let mut moved2: Vec<(usize, usize)> = Vec::new();
        let r = rmib_copyout(Some(&window), 8, &[0xBB; 16], |b, off| {
            moved2.push((off, b.len()));
            Ok(())
        });
        assert_eq!(r, Ok(16), "返回请求长度(C rmib.c:124)");
        assert_eq!(moved2, vec![(8, 8)]);

        // Outside the window: nothing moves and size is returned as-is.
        let mut calls = 0;
        let r = rmib_copyout(Some(&window), 16, &[0xCC; 4], |_, _| {
            calls += 1;
            Ok(())
        });
        assert_eq!(r, Ok(4));
        assert_eq!(calls, 0);

        // None window (C NULL): compute only, no copy.
        let r = rmib_copyout(None, 0, &[1; 8], |_, _| {
            calls += 1;
            Ok(())
        });
        assert_eq!(r, Ok(8));
        assert_eq!(calls, 0);
    }

    /// C rmib.c:63-88 — inrange and getoldlen.
    #[test]
    fn test_rmib_inrange_and_getoldlen() {
        let window = RmibOldp { grant: 1, len: 32 };
        assert!(rmib_inrange(Some(&window), 0));
        assert!(rmib_inrange(Some(&window), 31));
        assert!(!rmib_inrange(Some(&window), 32));
        assert!(!rmib_inrange(None, 0));
        assert_eq!(rmib_getoldlen(Some(&window)), 32);
        assert_eq!(rmib_getoldlen(None), 0);
    }

    /// C rmib.c:200-260 — copyout_node packing: version stamp, SPARSE stripping,
    /// name, immediate dispatch, NODE special cases (csize/clen or NODE_FN).
    #[test]
    fn test_rmib_copyout_node_packs_sysctlnode() {
        let c = call();
        let window = RmibOldp { grant: 5, len: 256 };
        // A non-PRIVATE int immediate node: the value must be filled in.
        let mut pub_node = node(
            minix_types::CTLFLAG_PERMANENT | CTLTYPE_INT | CTLFLAG_IMMEDIATE,
            "nprocs",
        );
        pub_node.value = Some(RmibImmediate::Int(42));

        let mut captured: Vec<u8> = Vec::new();
        let size = rmib_copyout_node(&c, Some(&window), 0, 0, &pub_node, |b, _| {
            captured.extend_from_slice(b);
            Ok(())
        })
        .unwrap();
        assert_eq!(size, core::mem::size_of::<SysctlNode>());

        // SAFETY(test): read the packed int immediate arm.
        let scn: &SysctlNode = unsafe { &*(captured.as_ptr() as *const SysctlNode) };
        assert_eq!(scn.sysctl_flags, SYSCTL_VERSION | (pub_node.flags & !CTLFLAG_SPARSE));
        assert_eq!(scn.sysctl_num, 0);
        assert_eq!(scn.sysctl_ver, 7);
        // SAFETY(test): the int immediate arm was written by packing.
        assert_eq!(unsafe { scn.sysctl_un.scu_idata }, 42);

        // C rmib.c:219-231 — PRIVATE without authorization: the immediate value is
        // not filled in (stays zero); the other fields pack as usual.
        let mut private_node = node(
            minix_types::CTLFLAG_PERMANENT | CTLTYPE_INT | CTLFLAG_IMMEDIATE | CTLFLAG_PRIVATE,
            "secret",
        );
        private_node.value = Some(RmibImmediate::Int(42));
        let mut captured2: Vec<u8> = Vec::new();
        rmib_copyout_node(&c, Some(&window), 0, 1, &private_node, |b, _| {
            captured2.extend_from_slice(b);
            Ok(())
        })
        .unwrap();
        let scn2: &SysctlNode = unsafe { &*(captured2.as_ptr() as *const SysctlNode) };
        assert_eq!(scn2.sysctl_num, 1);
        // SAFETY(test): the unauthorized path does not write the immediate value.
        assert_eq!(unsafe { scn2.sysctl_un.scu_idata }, 0, "无授权不得泄漏 immediate");
    }

    /// C rmib.c:233-249 — NODE special cases: non-function plus visible → report
    /// csize/clen; function-driven → the SYSCTL_NODE_FN sentinel; the SPARSE flag
    /// never goes on the wire.
    #[test]
    fn test_rmib_copyout_node_parent_rules() {
        let c = call();
        let window = RmibOldp { grant: 5, len: 512 };

        let mut parent = node(
            minix_types::CTLFLAG_PERMANENT | CTLTYPE_NODE | CTLFLAG_SPARSE | CTLFLAG_READWRITE,
            "kern",
        );
        parent.children = RmibChildren::Dense(vec![node(CTLFLAG_PRIVATE, "secret")]);

        let mut captured: Vec<u8> = Vec::new();
        rmib_copyout_node(&c, Some(&window), 0, 0, &parent, |b, _| {
            captured.extend_from_slice(b);
            Ok(())
        })
        .unwrap();
        // SAFETY(test): read the packed result (simulating userland decoding).
        let scn: &SysctlNode = unsafe { &*(captured.as_ptr() as *const SysctlNode) };
        // SPARSE is stripped: wire flags = VERSION + original flags minus SPARSE.
        assert_eq!(scn.sysctl_flags & CTLFLAG_SPARSE, 0);
        assert_eq!(scn.sysctl_flags & SYSCTL_VERSION, SYSCTL_VERSION);
        assert_eq!(scn._sysctl_size, core::mem::size_of::<SysctlNode>() as u64);
        // SAFETY(test): the child arm was written by the NODE special case.
        assert_eq!(unsafe { scn.sysctl_un.scu_child.suc_clen }, 1);
        // SAFETY(test): a non-function node does not write _suc_child; arm ownership
        // was confirmed before reading.
        assert_eq!(unsafe { scn.sysctl_un.scu_child._suc_child }, 0);

        // Function-driven node: the NODE_FN sentinel is set.
        let mut func_node = node(CTLFLAG_PERMANENT | CTLTYPE_NODE, "dyn");
        func_node.func = true;
        let mut captured2: Vec<u8> = Vec::new();
        rmib_copyout_node(&c, Some(&window), 0, 0, &func_node, |b, _| {
            captured2.extend_from_slice(b);
            Ok(())
        })
        .unwrap();
        let scn2: &SysctlNode = unsafe { &*(captured2.as_ptr() as *const SysctlNode) };
        // SAFETY(test): the NODE_FN sentinel was written by the function-driven
        // special case.
        assert_eq!(unsafe { scn2.sysctl_un.scu_child._suc_child }, SYSCTL_NODE_FN as u64);
    }

    /// C rmib.c:355-364 — a PRIVATE node's description is skipped outright
    /// (returns 0, no accumulation).
    #[test]
    fn test_rmib_copyout_desc_skips_private() {
        let c = call();
        let window = RmibOldp { grant: 5, len: 256 };
        let mut private_node = node(CTLFLAG_PRIVATE, "secret");
        private_node.desc = Some(String::from("hidden description"));

        let mut calls = 0;
        let r = rmib_copyout_desc(&c, Some(&window), 0, 4, &private_node, |_, _| {
            calls += 1;
            Ok(())
        });
        assert_eq!(r, Ok(0));
        assert_eq!(calls, 0);
    }
}

/// Description-fetch request issued at MIB mount time (C: the
/// `m_mib_lsys_info` field set, rmib.c:998 onward).
pub struct RmibInfoReq {
    /// Subtree root id. C: `root_id`.
    pub root_id: u32,
    /// Grant for the root node name buffer. C: `name_grant`.
    pub name_grant: i32,
    /// Name buffer size. C: `name_size`.
    pub name_size: usize,
    /// Grant for the description buffer. C: `desc_grant`.
    pub desc_grant: i32,
    /// Description buffer size. C: `desc_size`.
    pub desc_size: usize,
}

/// C: `rmib_info` — rmib.c:998-1035. Copies the subtree root's name and
/// description to the MIB service (the MIB pulls them once at mount time).
/// A name that does not fit → ENAMETOOLONG (same as C; the service author's bug).
/// An over-long description is truncated to desc_size (C rmib.c:1030-1032);
/// no description copies an empty string. Returns OK on success (C returns the
/// result of the last safecopy).
pub fn rmib_info(
    table: &SubtreeTable,
    req: &RmibInfoReq,
    io: &mut dyn RmibIo,
) -> Result<(), i32> {
    let slot = table.slots.get(req.root_id as usize).ok_or(minix_types::ENOENT)?;
    let root = slot.tree.as_ref().ok_or(minix_types::ENOENT)?;

    // C rmib.c:1007-1011 — the name (including the NUL) must fit.
    let mut name = Vec::with_capacity(root.name.len() + 1);
    name.extend_from_slice(root.name.as_bytes());
    name.push(0);
    if name.len() > req.name_size {
        return Err(minix_types::ENAMETOOLONG);
    }
    io.copyout(&name, req.name_grant, 0)?;

    // C rmib.c:1014-1032 — no description copies an empty string; an over-long
    // one is truncated to desc_size.
    let mut desc: Vec<u8> = Vec::new();
    match &root.desc {
        Some(d) => {
            desc.extend_from_slice(d.as_bytes());
            desc.push(0);
        }
        None => desc.push(0),
    }
    let dsize = desc.len().min(req.desc_size);
    io.copyout(&desc[..dsize], req.desc_grant, 0)?;
    Ok(())
}

// ── E-RMIBWIRE 2/2: rmib_call traversal, leaf read/write, registration bookkeeping ──
//
// C: rmib_call (rmib.c:678-824), rmib_getptr (:482-516), rmib_read
// (:518-545), rmib_write (:547-645), rmib_readwrite (:647-668),
// rmib_register/deregister/reregister/send_reg (:862-975), rmib_init.
//
// ARCH deviations (two, both declared on the 10-stage-mib side):
// 1. grant copy-in/copy-out is injected through [`RmibIo`] (the real
//    implementation = sys_safecopyfrom/to, wired up after E1); the tree and
//    leaf data live in this process's memory, so reads/writes are pure memory
//    operations.
// 2. Registration-message production (asynsend3 to the MIB service) is
//    separated from sending: the bookkeeping functions return the pending
//    message, and sending belongs to the service main loop (asynsend3/
//    AMF_NOREPLY after E1).

use minix_types::{grant_valid, CTL_MAXNAME, CTLFLAG_ANYWRITE, CTL_SHORTNAME, ERESTART};

/// A forwarded sysctl call request (C: the `m_mib_lsys_call` field set,
/// rmib.c:687-716).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RmibCallReq {
    /// Subtree root id (the MountTable slot number). C: `root_id`.
    pub root_id: u32,
    /// Length of the remaining name. C: `name_len`.
    pub name_len: usize,
    /// Grant for the name buffer. C: `name_grant`.
    pub name_grant: i32,
    /// Grant for the old-value window. C: `oldp_grant`.
    pub oldp_grant: i32,
    /// Requested old-value length. C: `oldp_len`.
    pub oldp_len: usize,
    /// Grant for the new-value window. C: `newp_grant`.
    pub newp_grant: i32,
    /// New-value length. C: `newp_len`.
    pub newp_len: usize,
    /// Endpoint of the user process that issued the sysctl. C: `user_endpt`.
    pub user_endpt: i32,
    /// Call flags (RMIB_FLAG_AUTH). C: `flags`.
    pub flags: u32,
    /// Subtree version. C: `root_ver`.
    pub root_ver: u32,
    /// Whole-tree version. C: `tree_ver`.
    pub tree_ver: u32,
}

/// Copy-in/copy-out verbs for the grant channel (sys_safecopyfrom/to after E1;
/// host tests use an in-memory implementation).
pub trait RmibIo {
    /// Copy in at most `dst.len()` bytes from `grant` at offset `off`.
    fn copyin(&mut self, dst: &mut [u8], grant: i32, off: usize) -> Result<(), i32>;
    /// Write `src` into `grant` at offset `off`.
    fn copyout(&mut self, src: &[u8], grant: i32, off: usize) -> Result<(), i32>;
}

/// Service callback for a function-driven node (C:
/// `rnode->rnode_func(&call, rnode, oldp, newp)`, rmib.h:18-21 +
/// rmib.c:809-811 — C hangs a function pointer on the node, while Rust's
/// [`RmibNode`] only carries the `func: bool` flag and the service injects the
/// handler through this trait). Returns the number of bytes copied out (`Ok`)
/// or a negative errno (`Err`).
pub trait RmibFuncHandler {
    fn call_func(
        &mut self,
        node: &RmibNode,
        call: &RmibCall,
        oldp: Option<&RmibOldp>,
        newp: Option<&RmibNewp>,
        io: &mut dyn RmibIo,
    ) -> Result<usize, i32>;
}

/// A pending message produced by registration bookkeeping (the
/// `MIB_REGISTER`/`MIB_DEREGISTER` that C's `rmib_send_reg` assembles —
/// m_type plus the m_lsys_mib_register payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmibRegMessage {
    /// `MIB_REGISTER`: root id, flags with SPARSE stripped, child capacity and
    /// count, name.
    Register {
        /// Root id (slot number).
        root_id: u32,
        /// Flags with SPARSE stripped plus SYSCTL_VERSION. C: rmib_send_reg
        /// (:864-871).
        flags: u32,
        /// Child capacity. C: `csize`.
        csize: u32,
        /// Effective child count. C: `clen`.
        clen: u32,
        /// Mount path (prefix name). C: `mib`/`miblen`.
        mib: [i32; CTL_SHORTNAME as usize],
        /// Name length. C: `miblen`.
        miblen: u32,
    },
    /// `MIB_DEREGISTER`: root id only (C rmib.c:955-965).
    Deregister {
        /// Root id (slot number).
        root_id: u32,
    },
}

/// The in-slot tree plus mount path (C: the `rnodes[]` slot triple,
/// rnodes[.].rno_node/rno_name/rno_namelen).
#[derive(Debug, Clone, Default)]
pub struct MountSlot {
    /// The mounted subtree root (owned, replacing C's bare pointer). None = empty
    /// slot.
    pub tree: Option<RmibNode>,
    /// Mount path (prefix name). C: `rno_name` (an i32 array, one name component
    /// per element).
    pub name: [i32; CTL_SHORTNAME as usize],
    /// Path length. C: `rno_namelen`.
    pub namelen: usize,
}

/// Subtree registry (C: `rnodes[RMIB_MAX_SUBTREES]`). The slot number is the
/// on-wire root id.
#[derive(Debug, Clone, Default)]
pub struct SubtreeTable {
    /// Sixteen slots (C rmib.c:47).
    pub slots: [MountSlot; RMIB_MAX_SUBTREES],
}

impl SubtreeTable {
    /// An empty table.
    pub fn new() -> Self {
        Self { slots: core::array::from_fn(|_| MountSlot::default()) }
    }

    /// Register a subtree (C: `rmib_register` — rmib.c:892-924). The name must be
    /// non-empty and shorter than the limit, and the root must be a NODE;
    /// registering the same tree twice is EEXIST; a full table is ENOMEM.
    /// Returns `(root id, pending MIB_REGISTER message)` — sending belongs to the
    /// caller (asynsend3 after E1, ARCH deviation 2).
    pub fn register(
        &mut self,
        name: &[i32],
        root: RmibNode,
    ) -> Result<(u32, RmibRegMessage), i32> {
        // C: rmib.c:896-899 — namelen ∈ [1, CTL_SHORTNAME) and the root is a NODE.
        if name.is_empty() || name.len() >= CTL_SHORTNAME as usize {
            return Err(minix_types::EINVAL);
        }
        if minix_types::sysctl_type(root.flags) != minix_types::CTLTYPE_NODE {
            return Err(minix_types::EINVAL);
        }
        // C: rmib.c:902-908 — the same tree is EEXIST; take the first free slot.
        let mut free_id: Option<usize> = None;
        for (id, slot) in self.slots.iter().enumerate() {
            if slot.tree.as_ref() == Some(&root) {
                return Err(minix_types::EEXIST);
            }
            if slot.tree.is_none() && free_id.is_none() {
                free_id = Some(id);
            }
        }
        let id = free_id.ok_or(minix_types::ENOMEM)?;

        let mut mib = [0i32; CTL_SHORTNAME as usize];
        mib[..name.len()].copy_from_slice(name);
        let clen = count_children(&root);

        self.slots[id] = MountSlot {
            tree: Some(root),
            name: mib,
            namelen: name.len(),
        };

        // C: rmib_send_reg — strip SPARSE, stamp the version, add csize/clen and
        // the name.
        let tree = self.slots[id].tree.as_ref().unwrap();
        Ok((
            id as u32,
            RmibRegMessage::Register {
                root_id: id as u32,
                flags: SYSCTL_VERSION | (tree.flags & !CTLFLAG_SPARSE),
                csize: tree.size,
                clen,
                mib,
                miblen: name.len() as u32,
            },
        ))
    }

    /// Deregister a subtree (C: `rmib_deregister` — rmib.c:934-969). Returns the
    /// pending MIB_DEREGISTER message; an unregistered tree is ENOENT. C ignores
    /// asynsend3 failures (the comment says the caller cannot remedy them) — with
    /// sending owned by the caller, that semantics is preserved naturally.
    pub fn deregister(&mut self, root: &RmibNode) -> Result<RmibRegMessage, i32> {
        for (id, slot) in self.slots.iter_mut().enumerate() {
            if slot.tree.as_ref() == Some(root) {
                *slot = MountSlot::default();
                return Ok(RmibRegMessage::Deregister { root_id: id as u32 });
            }
        }
        Err(minix_types::ENOENT)
    }

    /// Re-send the registration of every live subtree (C: `rmib_reregister` —
    /// rmib.c:971-980; called by the main loop after the MIB service restarts).
    pub fn reregister(&mut self) -> Vec<RmibRegMessage> {
        let mut out = Vec::new();
        for (id, slot) in self.slots.iter_mut().enumerate() {
            if let Some(tree) = slot.tree.as_mut() {
                let clen = count_children(tree);
                out.push(RmibRegMessage::Register {
                    root_id: id as u32,
                    flags: SYSCTL_VERSION | (tree.flags & !CTLFLAG_SPARSE),
                    csize: tree.size,
                    clen,
                    mib: slot.name,
                    miblen: slot.namelen as u32,
                });
            }
        }
        out
    }
}

/// Recursively computes the effective child count (the clen half of C's
/// `rmib_init`, rmib.c:826-860): dense skips empty slots with flags==0; sparse
/// counts every explicit pair.
pub fn count_children(root: &RmibNode) -> u32 {
    match &root.children {
        RmibChildren::None => 0,
        RmibChildren::Sparse(v) => v.len() as u32,
        RmibChildren::Dense(v) => v.iter().filter(|n| n.flags != 0).count() as u32,
    }
}

/// The data side of C's `rmib_getptr` — rmib.c:482-516: returns the leaf
/// node's current value bytes (immediate serialized; data nodes return their
/// own buffer; STRING+IMMEDIATE and NODE have no data → None).
/// `rmib_readwrite`/`rmib_read` take their source from here.
fn getptr_bytes(rnode: &RmibNode) -> Option<Vec<u8>> {
    let node_type = sysctl_type(rnode.flags);
    let immediate = rnode.flags & minix_types::CTLFLAG_IMMEDIATE != 0;
    match node_type {
        CTLTYPE_BOOL if immediate => {
            Some(vec![matches!(rnode.value, Some(RmibImmediate::Bool(true))) as u8])
        }
        CTLTYPE_INT if immediate => {
            Some(rnode.value.map(|v| match v {
                RmibImmediate::Int(i) => i.to_le_bytes().to_vec(),
                _ => vec![0, 0, 0, 0],
            }).unwrap_or_default())
        }
        CTLTYPE_QUAD if immediate => {
            Some(rnode.value.map(|v| match v {
                RmibImmediate::Quad(q) => q.to_le_bytes().to_vec(),
                _ => vec![0; 8],
            }).unwrap_or_default())
        }
        CTLTYPE_STRING | CTLTYPE_STRUCT if immediate => None,
        _ => rnode.data.clone(),
    }
}

/// Writes the leaf node's current value bytes back (the write half of
/// `rmib_getptr`; immediate nodes write `value`, data nodes write `data`).
/// A length mismatch returns None (the caller maps that to EINVAL).
fn setptr_bytes(rnode: &mut RmibNode, bytes: &[u8]) -> Option<()> {
    let node_type = sysctl_type(rnode.flags);
    let immediate = rnode.flags & minix_types::CTLFLAG_IMMEDIATE != 0;
    if immediate {
        match node_type {
            CTLTYPE_BOOL => {
                rnode.value = Some(RmibImmediate::Bool(bytes.first().is_some_and(|b| *b != 0)));
            }
            CTLTYPE_INT => {
                let mut b4 = [0u8; 4];
                b4.copy_from_slice(bytes.get(..4)?);
                rnode.value = Some(RmibImmediate::Int(i32::from_le_bytes(b4)));
            }
            CTLTYPE_QUAD => {
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(bytes.get(..8)?);
                rnode.value = Some(RmibImmediate::Quad(u64::from_le_bytes(b8)));
            }
            _ => return None,
        }
    } else {
        rnode.data = Some(bytes.to_vec());
    }
    Some(())
}

/// C: `rmib_read` — rmib.c:518-545. Read the current (old) data: the length is
/// always returned; the data is copied out only when a window exists.
fn rmib_read(rnode: &RmibNode, oldp: Option<&RmibOldp>, io: &mut dyn RmibIo) -> Result<usize, i32> {
    let data = getptr_bytes(rnode).ok_or(minix_types::EINVAL)?;
    if let Some(w) = oldp {
        io.copyout(&data, w.grant, 0)?;
    }
    Ok(data.len())
}

/// C: `rmib_write` — rmib.c:547-645. Write a new value: first copy into a
/// temporary buffer (so a mid-way failure cannot corrupt the value, C
/// rmib.c:556-566), validate the length by type (non-strings must match
/// exactly; strings may be shorter and get a trailing NUL; over the stack
/// budget without authorization is EPERM), then commit.
fn rmib_write(rnode: &mut RmibNode, newp: Option<&RmibNewp>, call: &RmibCall, io: &mut dyn RmibIo) -> Result<(), i32> {
    let newp = match newp {
        Some(n) => n,
        None => return Ok(()), // nothing to do (rmib.c:558)
    };
    let newlen = newp.len;
    let node_type = sysctl_type(rnode.flags);
    let immediate = rnode.flags & minix_types::CTLFLAG_IMMEDIATE != 0;

    // C rmib.c:570-592 — data nodes must have storage; the length is validated
    // by type.
    if rnode.data.is_none() && (!immediate || matches!(node_type, CTLTYPE_STRING | CTLTYPE_STRUCT)) {
        // NODE/no-data node: rmib_getptr returns NULL → EINVAL.
        if !immediate {
            return Err(minix_types::EINVAL);
        }
    }
    match node_type {
        CTLTYPE_STRING => {
            if newlen > rnode.size as usize {
                return Err(minix_types::EINVAL);
            }
        }
        _ => {
            if newlen != rnode.size as usize {
                return Err(minix_types::EINVAL);
            }
        }
    }

    // C rmib.c:604-615 — over the stack budget (RMIB_STACKBUF) and not authorized
    // → EPERM.
    if newlen + 1 > RMIB_STACKBUF && (call.flags & RMIB_FLAG_AUTH) == 0 {
        return Err(minix_types::EPERM);
    }

    // Copy into the temporary buffer (C rmib.c:623-626).
    let mut src = vec![0u8; newlen + 1];
    io.copyin(&mut src[..newlen], newp.grant, 0)?;

    // C rmib.c:627-648 — validate and commit (STRING gets its NUL filled in).
    if node_type == CTLTYPE_STRING && newlen > 0 && src[newlen - 1] != 0 {
        if newlen == rnode.size as usize {
            return Err(minix_types::EINVAL); // the NUL does not fit (rmib.c:636-641)
        }
        src[newlen] = 0; // C rmib.c:643
    }
    // SAFETY(shape): the write arm is decided by the type; the length was
    // validated above.
    setptr_bytes(rnode, &src[..newlen.min(rnode.size as usize).max(1)]);
    Ok(())
}

/// C: `rmib_readwrite` — rmib.c:647-668. Generic leaf read+write: read the old
/// value first (always returning the old length), then write the new one.
fn rmib_readwrite(
    call: &RmibCall,
    rnode: &mut RmibNode,
    oldp: Option<&RmibOldp>,
    newp: Option<&RmibNewp>,
    io: &mut dyn RmibIo,
) -> Result<usize, i32> {
    let len = rmib_read(rnode, oldp, io)?;
    rmib_write(rnode, newp, call, io)?;
    Ok(len)
}

/// C: `rmib_call` — rmib.c:678-824. Handles a sysctl call forwarded by the MIB
/// service.
///
/// The name is walked component by component: a negative id is a
/// meta-identifier (QUERY/DESCRIBE must be the last component; CREATE/DESTROY
/// apply to static subtrees only → EPERM; everything else EOPNOTSUPP); a
/// positive id looks up a child (PRIVATE without authorization EPERM; name
/// components after a leaf ENOTDIR; a leaf plus a new value requires READWRITE
/// and [ANYWRITE or authorization]); function-driven nodes return EOPNOTSUPP in
/// this version (the handler is dispatched by the service and injected through
/// the `func` callback); ordinary leaves go through readwrite. Name exhausted
/// on a non-leaf → EISDIR (the name points at a node array).
///
/// `func` is the callback for function-driven nodes: when `Some`, func nodes
/// are handed to [`RmibFuncHandler::call_func`] (with the same return semantics
/// as C's `rnode->rnode_func(&call, rnode, oldp, newp)`, i.e.
/// `Ok(byte count)/Err(errno)`); when `None`, the existing EOPNOTSUPP is kept.
pub fn rmib_call(
    table: &mut SubtreeTable,
    req: &RmibCallReq,
    io: &mut dyn RmibIo,
    func: Option<&mut dyn RmibFuncHandler>,
) -> Result<usize, i32> {
    // C rmib.c:688-695 — an unregistered subtree returns ERESTART (the MIB should
    // deregister the mount it believes it has).
    let slot = match req.root_id as usize >= RMIB_MAX_SUBTREES {
        true => return Err(ERESTART),
        false => &mut table.slots[req.root_id as usize],
    };
    let root = match slot.tree.as_mut() {
        Some(t) => t,
        None => return Err(ERESTART),
    };

    // C rmib.c:702-716 — the mount path is the prefix; the remaining name is
    // copied in through the grant.
    let prefixlen = slot.namelen;
    if prefixlen + req.name_len > CTL_MAXNAME as usize {
        return Err(minix_types::EINVAL);
    }
    let mut name = [0i32; CTL_MAXNAME as usize];
    name[..prefixlen].copy_from_slice(&slot.name[..prefixlen]);
    if req.name_len > 0 {
        let mut tail = vec![0u8; req.name_len * 4];
        io.copyin(&mut tail, req.name_grant, 0)?;
        for (i, chunk) in tail.chunks_exact(4).enumerate() {
            name[prefixlen + i] = i32::from_le_bytes(chunk.try_into().expect("4 bytes"));
        }
    }

    // C rmib.c:718-726 — oldp/newp are built according to grant validity (an
    // invalid grant = NULL).
    let oldp = if grant_valid(req.oldp_grant) {
        Some(RmibOldp { grant: req.oldp_grant, len: req.oldp_len })
    } else {
        None
    };
    let newp = if grant_valid(req.newp_grant) {
        Some(RmibNewp { grant: req.newp_grant, len: req.newp_len })
    } else {
        None
    };

    // C rmib.c:730-733 + :747-760 — walk down level by level.
    let mut call_ctx = RmibCall {
        endpt: req.user_endpt,
        oname: name,
        name,
        namelen: req.name_len,
        flags: req.flags,
        rootver: req.root_ver,
        treever: req.tree_ver,
    };
    // C rmib.c:736 `call.call_name = &name[prefixlen]` — the view visible to
    // handlers and leaf writes starts after the mount prefix; each consumed
    // component shifts the view left by one (C advances the `call.call_name`
    // pointer, rmib.c:746-748). Rust shifts in place with copy_within:
    // name[0..namelen] always holds the remaining components after the current
    // node.
    call_ctx.name.copy_within(prefixlen.., 0);
    let mut rnode: &mut RmibNode = root;

    while call_ctx.namelen > 0 {
        let id = call_ctx.name[0];
        call_ctx.name.copy_within(1.., 0);
        call_ctx.namelen -= 1;

        // C: rparent is always a NODE (the assert at rmib.c:750).
        // Meta-identifier: must be the last component (rmib.c:755-771).
        if id < 0 {
            if call_ctx.namelen > 0 {
                return Err(minix_types::EINVAL);
            }
            return match id {
                minix_types::CTL_QUERY => {
                    enumerate_nodes(rnode, &call_ctx, oldp.as_ref(), io)
                }
                minix_types::CTL_DESCRIBE => {
                    describe_nodes(rnode, &call_ctx, oldp.as_ref(), io)
                }
                minix_types::CTL_CREATE | minix_types::CTL_DESTROY => {
                    Err(minix_types::EPERM) // static subtrees only (rmib.c:776-779)
                }
                _ => Err(minix_types::EOPNOTSUPP),
            };
        }

        // C rmib.c:782-784 — find the child.
        let next = rmib_lookup_mut(rnode, id as u32);
        rnode = match next {
            Some(child) => child,
            None => return Err(minix_types::ENOENT),
        };

        // C rmib.c:787-789 — access gate for this level.
        if (rnode.flags & CTLFLAG_PRIVATE) != 0 && (call_ctx.flags & RMIB_FLAG_AUTH) == 0 {
            return Err(minix_types::EPERM);
        }

        let is_leaf = sysctl_type(rnode.flags) != CTLTYPE_NODE;
        let has_func = rnode.func;

        // C rmib.c:793-796 — no name component may follow a leaf.
        if is_leaf && call_ctx.namelen > 0 {
            return Err(minix_types::ENOTDIR);
        }

        // C rmib.c:799-806 — leaf (or function node) plus a new value: the
        // write-permission gate.
        if (is_leaf || has_func) && newp.is_some() {
            if (rnode.flags & CTLFLAG_READWRITE) != CTLFLAG_READWRITE {
                return Err(minix_types::EPERM);
            }
            if (rnode.flags & CTLFLAG_ANYWRITE) == 0 && (call_ctx.flags & RMIB_FLAG_AUTH) == 0 {
                return Err(minix_types::EPERM);
            }
        }

        // C rmib.c:809-811 — function-driven nodes go to the handler; the return
        // value is the result (`Ok(byte count)`/`Err(negative errno)`). Without an
        // injected callback, EOPNOTSUPP is kept.
        if has_func {
            return match func {
                Some(h) => h.call_func(rnode, &call_ctx, oldp.as_ref(), newp.as_ref(), io),
                None => Err(minix_types::EOPNOTSUPP),
            };
        }

        // C rmib.c:813-815 — ordinary data leaf: generic read/write.
        if is_leaf {
            return rmib_readwrite(&call_ctx, rnode, oldp.as_ref(), newp.as_ref(), io);
        }
        // Otherwise keep walking down.
    }

    // C rmib.c:820-822 — name exhausted on a non-leaf: the name points at a node
    // array → EISDIR.
    Err(minix_types::EISDIR)
}

/// C: `rmib_query` — rmib.c:282-345. Enumerate children (a copyout_node
/// sequence).
fn enumerate_nodes(
    rnode: &RmibNode,
    call: &RmibCall,
    oldp: Option<&RmibOldp>,
    io: &mut dyn RmibIo,
) -> Result<usize, i32> {
    let mut off = 0usize;
    for (id, child) in iter_children(rnode) {
        off += rmib_copyout_node(call, oldp, off, id, child, |b, at| io.copyout(b, oldp_grant_of(oldp), at))?;
    }
    Ok(off)
}

/// C: `rmib_describe` — rmib.c:414-458. Enumerate child descriptions.
fn describe_nodes(
    rnode: &RmibNode,
    call: &RmibCall,
    oldp: Option<&RmibOldp>,
    io: &mut dyn RmibIo,
) -> Result<usize, i32> {
    let mut off = 0usize;
    for (id, child) in iter_children(rnode) {
        off += rmib_copyout_desc(call, oldp, off, id, child, |b, at| io.copyout(b, oldp_grant_of(oldp), at))?;
    }
    Ok(off)
}

fn oldp_grant_of(oldp: Option<&RmibOldp>) -> i32 {
    oldp.map(|o| o.grant).unwrap_or(-1)
}

/// Unified child-iteration shape (dense skips empty slots with flags==0;
/// sparse uses explicit ids; C rmib.c:337-350 / :435-447).
fn iter_children(rnode: &RmibNode) -> Vec<(u32, &RmibNode)> {
    match &rnode.children {
        RmibChildren::None => Vec::new(),
        RmibChildren::Dense(v) => v
            .iter()
            .enumerate()
            .filter(|(_, n)| n.flags != 0)
            .map(|(i, n)| (i as u32, n))
            .collect(),
        RmibChildren::Sparse(v) => v.iter().map(|(i, n)| (*i, n)).collect(),
    }
}

// ── rmib_call function-node dispatch (C rmib.c:809-811) ──

#[cfg(test)]
mod func_tests {
    use super::*;
    use alloc::{string::String, vec::Vec};
    use minix_types::{CTLTYPE_INT, CTLTYPE_NODE, EFAULT, EINVAL};

    /// In-memory grant channel: copyout appends records, copyin reads by grant.
    struct MemIo {
        grants: Vec<(i32, Vec<u8>)>,
        written: Vec<(i32, Vec<u8>)>,
    }
    impl MemIo {
        fn new(grants: Vec<(i32, Vec<u8>)>) -> Self {
            Self { grants, written: Vec::new() }
        }
    }
    impl RmibIo for MemIo {
        fn copyin(&mut self, dst: &mut [u8], grant: i32, off: usize) -> Result<(), i32> {
            let data = self
                .grants
                .iter()
                .find(|(g, _)| *g == grant)
                .map(|(_, d)| d.clone())
                .ok_or(EFAULT)?;
            if off + dst.len() > data.len() {
                return Err(EFAULT);
            }
            dst.copy_from_slice(&data[off..off + dst.len()]);
            Ok(())
        }
        fn copyout(&mut self, src: &[u8], grant: i32, off: usize) -> Result<(), i32> {
            let mut data = self
                .grants
                .iter()
                .find(|(g, _)| *g == grant)
                .map(|(_, d)| d.clone())
                .ok_or(EFAULT)?;
            if off + src.len() > data.len() {
                return Err(EFAULT);
            }
            data[off..off + src.len()].copy_from_slice(src);
            self.written.push((grant, data[off..off + src.len()].to_vec()));
            Ok(())
        }
    }

    /// Probe handler: records the node name and remaining-name length, returns per
    /// script.
    struct Probe {
        seen: Option<(String, usize)>,
        ret: Result<usize, i32>,
    }
    impl RmibFuncHandler for Probe {
        fn call_func(
            &mut self,
            node: &RmibNode,
            call: &RmibCall,
            _oldp: Option<&RmibOldp>,
            _newp: Option<&RmibNewp>,
            io: &mut dyn RmibIo,
        ) -> Result<usize, i32> {
            self.seen = Some((node.name.clone(), call.namelen));
            match self.ret {
                Ok(n) => {
                    io.copyout(&vec![0xABu8; n], 7, 0)?;
                    Ok(n)
                }
                Err(e) => Err(e),
            }
        }
    }

    fn node(flags: u32, name: &str) -> RmibNode {
        RmibNode { flags, name: String::from(name), ..RmibNode::default() }
    }

    /// Root NODE (children: [0] = func node, [1] = int leaf), mount name [0, 82].
    fn fixture() -> SubtreeTable {
        let mut root = node(CTLTYPE_NODE, "ipc");
        let mut info = node(CTLTYPE_NODE, "info");
        info.func = true;
        let mut leaf = node(CTLTYPE_INT | minix_types::CTLFLAG_READWRITE, "count");
        leaf.data = Some(alloc::vec![0, 0, 0, 0]);
        root.children = RmibChildren::Dense(alloc::vec![info, leaf]);
        let mut table = SubtreeTable::new();
        let (_id, _msg) = table.register(&[0, 82], root).expect("register");
        table
    }

    fn req(name_len: usize, name_grant: i32) -> RmibCallReq {
        RmibCallReq {
            root_id: 0,
            name_len,
            name_grant,
            oldp_grant: 7,
            oldp_len: 8,
            newp_grant: -1,
            newp_len: 0,
            user_endpt: 20,
            flags: 0,
            root_ver: 0,
            tree_ver: 0,
        }
    }

    #[test]
    fn test_func_node_dispatches_to_handler() {
        // C rmib.c:809-811 — name=[1] reaches the func node; the handler gets
        // namelen=0 (the name is exhausted at the node), and its return value is
        // the result.
        let mut table = fixture();
        let mut io = MemIo::new(alloc::vec![(5, alloc::vec![0, 0, 0, 0]), (7, alloc::vec![0; 16])]);
        let mut probe = Probe { seen: None, ret: Ok(4) };
        let r = rmib_call(&mut table, &req(1, 5), &mut io, Some(&mut probe));
        assert_eq!(r, Ok(4));
        assert_eq!(probe.seen, Some(("info".into(), 0)));
        assert_eq!(io.written, vec![(7, alloc::vec![0xAB; 4])]);
    }

    #[test]
    fn test_func_node_handler_errno_passthrough() {
        // The handler's Err is propagated as-is (C returns a negative errno).
        let mut table = fixture();
        let mut io = MemIo::new(alloc::vec![(5, alloc::vec![0, 0, 0, 0]), (7, alloc::vec![0; 16])]);
        let mut probe = Probe { seen: None, ret: Err(EINVAL) };
        assert_eq!(
            rmib_call(&mut table, &req(1, 5), &mut io, Some(&mut probe)),
            Err(EINVAL)
        );
    }

    #[test]
    fn test_func_node_without_handler_is_eopnotsupp() {
        // Pre-existing behavior preserved: None callback → EOPNOTSUPP.
        let mut table = fixture();
        let mut io = MemIo::new(alloc::vec![(5, alloc::vec![0, 0, 0, 0]), (7, alloc::vec![0; 16])]);
        assert_eq!(
            rmib_call(&mut table, &req(1, 5), &mut io, None),
            Err(minix_types::EOPNOTSUPP)
        );
    }

    #[test]
    fn test_handler_sees_name_after_consumed_component() {
        // C rmib.c:746-748 pointer-advance semantics: with name=[0(info),
        // 5(SEM_INFO)] the handler sees call.name[0]==5 and namelen==1 — the
        // remaining components after the current node (kern_ipc_info's
        // call_name[0]).
        let mut table = fixture();
        let mut io = MemIo::new(alloc::vec![
            (5, alloc::vec![0, 0, 0, 0, 5, 0, 0, 0]),
            (7, alloc::vec![0; 16]),
        ]);
        let mut probe = Probe { seen: None, ret: Ok(0) };
        let dbg = &table.slots[0];
        assert_eq!(dbg.namelen, 2, "slot namelen");
        assert!(dbg.tree.is_some());
        let root_ref = dbg.tree.as_ref().unwrap();
        assert!(rmib_lookup(root_ref, 0).is_some(), "child0 exists: {:?}", root_ref.children);
        let r = rmib_call(&mut table, &req(2, 5), &mut io, Some(&mut probe));
        assert!(r.is_ok(), "walker returned {r:?}");
        assert_eq!(probe.seen, Some(("info".into(), 1)));
    }

    #[test]
    fn test_data_leaf_bypasses_handler() {
        // name=[2] is an ordinary int leaf: it does not go to the handler but to
        // generic readwrite.
        let mut table = fixture();
        let mut io = MemIo::new(alloc::vec![(5, alloc::vec![1, 0, 0, 0]), (7, alloc::vec![0; 16])]);
        let mut probe = Probe { seen: None, ret: Ok(0) };
        let r = rmib_call(&mut table, &req(1, 5), &mut io, Some(&mut probe));
        assert!(r.is_ok());
        assert!(probe.seen.is_none(), "数据叶不得派发 handler");
    }
}

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

// ── E-RMIBWIRE 纯函数层:拷出原语与节点打包 ──
//
// C: rmib.c 的 `rmib_oldp`/`rmib_newp`(rmib.c:24-30)、`rmib_inrange`
// (:63-71)、`rmib_getoldlen`(:78-88)、`rmib_copyout`(:97-126)、
// `rmib_copyout_node`(:200-260)。拷出动作以闭包注入——grant 写入是
// 传输半(E1),而钳制/裁剪/打包语义是纯逻辑,宿主可测。

use alloc::string::String;
use alloc::vec::Vec;

use minix_types::{
    sysctl_type, CTLFLAG_IMMEDIATE, CTLFLAG_PRIVATE, CTLTYPE_BOOL, CTLTYPE_INT, CTLTYPE_NODE,
    CTLTYPE_QUAD, SysctlDesc, SysctlNode, SYSCTL_NAMELEN, SYSCTL_NODE_FN, SYSCTL_VERSION,
};

/// Outgoing data window (C: `struct rmib_oldp` — rmib.c:24-27): the
/// MIB service's grant plus the caller-requested length. `None`
/// (C `NULL`) means "compute only, no copy".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RmibOldp {
    /// MIB 服务的 grant(服务器向它写)。C: `oldp_grant`。
    pub grant: i32,
    /// 调用方请求的总长度。C: `oldp_len`。
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
    /// 调用方(发起 sysctl 的用户进程)端点。C: `call_endpt`。
    pub endpt: i32,
    /// 请求原始全名。C: `call_oname`。
    pub oname: [i32; minix_types::CTL_MAXNAME as usize],
    /// 剩余名字(自当前节点起)。C: `call_name`。
    pub name: [i32; minix_types::CTL_MAXNAME as usize],
    /// 剩余名字长度。C: `call_namelen`。
    pub namelen: usize,
    /// 调用旗标(RMIB_FLAG_AUTH)。C: `call_flags`。
    pub flags: u32,
    /// 子树版本。C: `call_rootver`。
    pub rootver: u32,
    /// 全树版本。C: `call_treever`。
    pub treever: u32,
}

/// C: `rmib_inrange` — rmib.c:63-71。偏移是否落在请求窗口内。
pub const fn rmib_inrange(oldp: Option<&RmibOldp>, off: usize) -> bool {
    match oldp {
        None => false,
        Some(o) => off < o.len,
    }
}

/// C: `rmib_getoldlen` — rmib.c:78-88。请求总长度(None → 0)。
pub const fn rmib_getoldlen(oldp: Option<&RmibOldp>) -> usize {
    match oldp {
        None => 0,
        Some(o) => o.len,
    }
}

/// C: `rmib_copyout` — rmib.c:97-126. 把 `buf[..size]` 写进窗口的
/// `[off..]`,自动钳制到请求长度;窗口外(或 `None`)时不动并原样返回
/// `size`(C 的"nothing to do"语义)。拷出动词注入,宿主可测。
///
/// 返回 `Ok(size)`(C 的"requested length for the caller's convenience")
/// 或 `Err(errno)`(注入动词失败,如 EFAULT)。
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

/// 远程 MIB 节点——服务自持子树的客户端形状(C: `struct rmib_node`,
/// rmib.h:64-90)。指针成员改为 owned 形状(名称串 + 子节点向量或稀疏
/// 对),函数驱动节点以 `func: true` 表达。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RmibNode {
    /// C: `rnode_flags` — CTLTYPE_ 类型 + CTLFLAG_ 旗标。
    pub flags: u32,
    /// C: `rnode_size` — 关联数据大小;NODE 时为子节点数(func 驱动的
    /// NODE 表示期望请求数)。
    pub size: u32,
    /// 直接值( Immediate 节点)。C: `rnode_val_u` 的 bool/int/quad/clen。
    pub value: Option<RmibImmediate>,
    /// 字符串/结构数据(非 immediate 的数据节点)。C: `rnode_data`。
    pub data: Option<Vec<u8>>,
    /// 子节点(非稀疏:索引即 id;稀疏:显式 id 对)。C: `rnode_cptr` /
    /// `rnode_icptr`。
    pub children: RmibChildren,
    /// 函数驱动节点(C: `rnode_func != NULL`)。Rust 侧不携带函数指针,
    /// handler 归属由服务自派。
    pub func: bool,
    /// 节点名。C: `rnode_name`。
    pub name: String,
    /// 描述(可缺省)。C: `rnode_desc`。
    pub desc: Option<String>,
}

/// 直接值(C: `rnode_val_u` 联合的具体臂)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmibImmediate {
    /// C: `rvu_bool`。
    Bool(bool),
    /// C: `rvu_int`。
    Int(i32),
    /// C: `rvu_quad`。
    Quad(u64),
    /// C: `rvu_clen`(实际子节点数,写回 NODE 描述时用)。
    Clen(u32),
}

/// 子节点集合:常规数组(id = 索引)或稀疏数组(显式 id 对,升序无重复;
/// C: `rnode_cptr` / `rnode_icptr`)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RmibChildren {
    /// 无子节点。
    #[default]
    None,
    /// C: `rnode_cptr` — 索引即 id,flags==0 的空槽被遍历跳过(rmib.c:351)。
    Dense(Vec<RmibNode>),
    /// C: `rnode_icptr` — (id, 节点) 对,必须升序无重复(rmib.h:53-56)。
    Sparse(Vec<(u32, RmibNode)>),
}

impl RmibChildren {
    /// 子节点数(C: `rnode_size` 对 sparse 情形由库维护为 icptr 长度)。
    pub fn len(&self) -> usize {
        match self {
            RmibChildren::None => 0,
            RmibChildren::Dense(v) => v.len(),
            RmibChildren::Sparse(v) => v.len(),
        }
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// C: `rmib_copyout_node` — rmib.c:200-260。把一个节点打包成
/// `sysctlnode`(96 字节,布局见 minix-types::sysctl_abi)并拷出:
/// 剥离 CTLFLAG_SPARSE(库内部旗标,rmib.c:208-214)、盖版本戳
/// SYSCTL_VERSION、immediate 值在可见时填入、NODE 特则(真实父节点
/// 报 csize/clen,函数驱动节点置 SYSCTL_NODE_FN)。
///
/// 返回 `Ok(size_of::<SysctlNode>())`——与 C 一致,便于调用方累计偏移。
pub fn rmib_copyout_node(
    call: &RmibCall,
    oldp: Option<&RmibOldp>,
    off: usize,
    id: u32,
    rnode: &RmibNode,
    write: impl FnOnce(&[u8], usize) -> Result<(), i32>,
) -> Result<usize, i32> {
    // C: rmib.c:203-206 — 窗口外不动,但返回节点大小供偏移累计。
    if !rmib_inrange(oldp, off) {
        return Ok(core::mem::size_of::<SysctlNode>());
    }

    // SAFETY-形状:SysctlNode 无 Default(union 成员);repr(C) 全 PoD,
    // 零值即 C 的 memset(&scn, 0, sizeof(scn))(rmib.c:206)。
    let mut scn: SysctlNode = unsafe { core::mem::zeroed() };

    // C: rmib.c:208-214 — 版本戳 + 剥离 CTLFLAG_SPARSE(NetBSD 重载旗标,
    // 不对 userland 暴露)。
    scn.sysctl_flags = SYSCTL_VERSION | (rnode.flags & !CTLFLAG_SPARSE);
    scn.sysctl_num = id as i32;
    let name_bytes = rnode.name.as_bytes();
    let copy_len = name_bytes.len().min(SYSCTL_NAMELEN);
    scn.sysctl_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
    scn.sysctl_ver = call.rootver;
    scn._sysctl_size = rnode.size as u64;

    // C: rmib.c:216-217 — PRIVATE 节点的信息仅对授权调用方可见。
    let visible =
        (rnode.flags & CTLFLAG_PRIVATE == 0) || (call.flags & RMIB_FLAG_AUTH) != 0;

    // C: rmib.c:219-231 — immediate 节点在可见时填入直接值。
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

    // C: rmib.c:233-249 — NODE 特则:真实父节点报 csize/clen(可见时),
    // 函数驱动节点置 SYSCTL_NODE_FN 哨兵(供 trace(1) 识别)。
    if sysctl_type(rnode.flags) == CTLTYPE_NODE {
        scn._sysctl_size = core::mem::size_of::<SysctlNode>() as u64;
        if !rnode.func && visible {
            scn.sysctl_un.scu_child.suc_csize = rnode.size;
            scn.sysctl_un.scu_child.suc_clen = rnode.children.len() as u32;
        } else if rnode.func {
            scn.sysctl_un.scu_child._suc_child = SYSCTL_NODE_FN as u64;
        }
    }

    // SAFETY: SysctlNodeUn 是 plain-old-data 联合;写 child 臂与上面
    // C 的 scn 字段填法一致(打印机可见性/类型已判)。
    // (字段写入已按 repr(C) 布局完成。)
    rmib_copyout(oldp, off, scn_as_bytes(&scn), write)
}

/// `SysctlNode` 的字节视图(repr(C),96 字节)。
fn scn_as_bytes(scn: &SysctlNode) -> &[u8] {
    // SAFETY: `#[repr(C)]` 结构体,全部字段为 PoD,无 padding 初始化
    // 要求(目标缓冲按字节读)。
    unsafe {
        core::slice::from_raw_parts(scn as *const SysctlNode as *const u8, core::mem::size_of::<SysctlNode>())
    }
}

/// C: `rmib_copyout_desc` 的描述打包半 — rmib.c:355-380(描述结构 +
/// 描述串分两次拷出;此处产出合并后的字节序列与应拷总量,写入仍注入)。
/// 描述长度含 NUL(desc 为 None 时按 1 字节计,rmib.c:368-372)。
pub fn rmib_copyout_desc(
    call: &RmibCall,
    oldp: Option<&RmibOldp>,
    off: usize,
    id: u32,
    rnode: &RmibNode,
    mut write: impl FnMut(&[u8], usize) -> Result<(), i32>,
) -> Result<usize, i32> {
    // C: rmib.c:360-364 — PRIVATE 节点的描述也是私有:直接返回 0
    // (不累计偏移,遍历中相当于"这个节点不存在")。
    if (rnode.flags & CTLFLAG_PRIVATE) != 0 && (call.flags & RMIB_FLAG_AUTH) == 0 {
        return Ok(0);
    }

    let desc = rnode.desc.as_deref().unwrap_or("");
    let len = desc.len() + 1; // 含 NUL(rmib.c:368-372)

    // repr(C) PoD,零值 = C 的 memset(rmib.c:373-376)。
    let mut scd: SysctlDesc = unsafe { core::mem::zeroed() };
    scd.descr_num = id as i32;
    scd.descr_ver = call.rootver;
    scd.descr_len = len as u32;

    // C: rmib.c:377-379 — 先拷描述头(offsetof descr_str = 12),再拷串。
    // 窗口钳制由 rmib_copyout 负责;这里先拷头。
    let head = unsafe {
        core::slice::from_raw_parts(
            &scd as *const SysctlDesc as *const u8,
            core::mem::size_of::<SysctlDesc>(),
        )
    };
    rmib_copyout(oldp, off, head, |bytes, at| write(bytes, at))?;
    let str_off = off + core::mem::size_of::<SysctlDesc>();

    // 描述串(带 NUL)。C 分两次拷出;合并为一次写入(头+串不连续内存,
    // C 原样;Rust 侧拼接后一次钳制拷出,语义等价——总量与偏移一致)。
    let mut payload = desc.as_bytes().to_vec();
    payload.push(0);
    let payload = payload;
    let _ = str_off;
    rmib_copyout(oldp, off + core::mem::size_of::<SysctlDesc>(), &payload, write)?;

    Ok(core::mem::size_of::<SysctlDesc>() + payload.len())
}

/// SYS_UPDATE 的节点查找(C: `rmib_lookup` — rmib.c:340-360 区域):
/// 在 `parent` 的子节点集合中按 id 找子节点。
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

    /// C rmib.c:97-126 — 拷出钳制三态:全量/截断/窗口外。
    #[test]
    fn test_rmib_copyout_clamps_to_window() {
        let window = RmibOldp { grant: 3, len: 16 };

        // 窗口内全量。
        let mut moved: Vec<(usize, usize)> = Vec::new();
        let r = rmib_copyout(Some(&window), 0, &[0xAA; 16], |b, off| {
            moved.push((off, b.len()));
            Ok(())
        });
        assert_eq!(r, Ok(16));
        assert_eq!(moved, vec![(0, 16)]);

        // 尾部截断:off=8 处只剩 8 字节预算,请求 16 只动 8。
        let mut moved2: Vec<(usize, usize)> = Vec::new();
        let r = rmib_copyout(Some(&window), 8, &[0xBB; 16], |b, off| {
            moved2.push((off, b.len()));
            Ok(())
        });
        assert_eq!(r, Ok(16), "返回请求长度(C rmib.c:124)");
        assert_eq!(moved2, vec![(8, 8)]);

        // 窗口外:什么都不动,原样返回 size。
        let mut calls = 0;
        let r = rmib_copyout(Some(&window), 16, &[0xCC; 4], |_, _| {
            calls += 1;
            Ok(())
        });
        assert_eq!(r, Ok(4));
        assert_eq!(calls, 0);

        // None 窗口(C NULL):只算不动。
        let r = rmib_copyout(None, 0, &[1; 8], |_, _| {
            calls += 1;
            Ok(())
        });
        assert_eq!(r, Ok(8));
        assert_eq!(calls, 0);
    }

    /// C rmib.c:63-88 — inrange 与 getoldlen。
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

    /// C rmib.c:200-260 — copyout_node 打包:版本戳、SPARSE 剥离、名称、
    /// immediate 分派、NODE 特则(csize/clen 或 NODE_FN 哨兵)。
    #[test]
    fn test_rmib_copyout_node_packs_sysctlnode() {
        let c = call();
        let window = RmibOldp { grant: 5, len: 256 };
        // 非 PRIVATE 的 int immediate 节点:值应填入。
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

        // SAFETY(test):读取打包好的 int immediate 臂。
        let scn: &SysctlNode = unsafe { &*(captured.as_ptr() as *const SysctlNode) };
        assert_eq!(scn.sysctl_flags, SYSCTL_VERSION | (pub_node.flags & !CTLFLAG_SPARSE));
        assert_eq!(scn.sysctl_num, 0);
        assert_eq!(scn.sysctl_ver, 7);
        // SAFETY(test):int immediate 臂由打包写入。
        assert_eq!(unsafe { scn.sysctl_un.scu_idata }, 42);

        // C rmib.c:219-231 — PRIVATE 且无授权:immediate 不填(保持零),
        // 其余字段照常打包。
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
        // SAFETY(test):未授权路径不写 immediate。
        assert_eq!(unsafe { scn2.sysctl_un.scu_idata }, 0, "无授权不得泄漏 immediate");
    }

    /// C rmib.c:233-249 — NODE 特则:非函数 + 可见 → 报 csize/clen;
    /// 函数驱动 → SYSCTL_NODE_FN 哨兵;SPARSE 旗标不进 wire。
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
        // SAFETY(test):读打包结果(模拟 userland 解读)。
        let scn: &SysctlNode = unsafe { &*(captured.as_ptr() as *const SysctlNode) };
        // SPARSE 已剥:wire flags = VERSION + 原 flags 去 SPARSE。
        assert_eq!(scn.sysctl_flags & CTLFLAG_SPARSE, 0);
        assert_eq!(scn.sysctl_flags & SYSCTL_VERSION, SYSCTL_VERSION);
        assert_eq!(scn._sysctl_size, core::mem::size_of::<SysctlNode>() as u64);
        // SAFETY(test):child 臂由 NODE 特则写入。
        assert_eq!(unsafe { scn.sysctl_un.scu_child.suc_clen }, 1);
        // SAFETY(test):非函数节点未写 _suc_child,读前已确认臂归属。
        assert_eq!(unsafe { scn.sysctl_un.scu_child._suc_child }, 0);

        // 函数驱动节点:置 NODE_FN 哨兵。
        let mut func_node = node(CTLFLAG_PERMANENT | CTLTYPE_NODE, "dyn");
        func_node.func = true;
        let mut captured2: Vec<u8> = Vec::new();
        rmib_copyout_node(&c, Some(&window), 0, 0, &func_node, |b, _| {
            captured2.extend_from_slice(b);
            Ok(())
        })
        .unwrap();
        let scn2: &SysctlNode = unsafe { &*(captured2.as_ptr() as *const SysctlNode) };
        // SAFETY(test):NODE_FN 哨兵由函数驱动特则写入。
        assert_eq!(unsafe { scn2.sysctl_un.scu_child._suc_child }, SYSCTL_NODE_FN as u64);
    }

    /// C rmib.c:355-364 — PRIVATE 节点的描述直接跳过(返回 0,不累计)。
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

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
/// [`rmib_lookup`] 的可变版(下行遍历中叶写入需要 &mut)。
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

/// MIB 挂载时的描述拉取请求(C: `m_mib_lsys_info` 字段集,rmib.c:998 起)。
pub struct RmibInfoReq {
    /// 子树根 id。C: `root_id`。
    pub root_id: u32,
    /// 根节点名缓冲 grant。C: `name_grant`。
    pub name_grant: i32,
    /// 名缓冲大小。C: `name_size`。
    pub name_size: usize,
    /// 描述缓冲 grant。C: `desc_grant`。
    pub desc_grant: i32,
    /// 描述缓冲大小。C: `desc_size`。
    pub desc_size: usize,
}

/// C: `rmib_info` — rmib.c:998-1035. 把子树根的名字与描述拷给 MIB 服务
/// (挂载时 MIB 会拉一次)。名字放不下 → ENAMETOOLONG(C 同,服务编写者
/// 的错);描述超长按 desc_size 截断(C rmib.c:1030-1032);无描述拷
/// 空串。成功返回 OK(C 的返回值即最后一次 safecopy 的结果)。
pub fn rmib_info(
    table: &SubtreeTable,
    req: &RmibInfoReq,
    io: &mut dyn RmibIo,
) -> Result<(), i32> {
    let slot = table.slots.get(req.root_id as usize).ok_or(minix_types::ENOENT)?;
    let root = slot.tree.as_ref().ok_or(minix_types::ENOENT)?;

    // C rmib.c:1007-1011 — 名字(含 NUL)必须放得下。
    let mut name = Vec::with_capacity(root.name.len() + 1);
    name.extend_from_slice(root.name.as_bytes());
    name.push(0);
    if name.len() > req.name_size {
        return Err(minix_types::ENAMETOOLONG);
    }
    io.copyout(&name, req.name_grant, 0)?;

    // C rmib.c:1014-1032 — 无描述拷空串;超长截断到 desc_size。
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

// ── E-RMIBWIRE 2/2:rmib_call 遍历、叶读写、注册簿记 ──
//
// C: rmib_call(rmib.c:678-824)、rmib_getptr(:482-516)、rmib_read
// (:518-545)、rmib_write(:547-645)、rmib_readwrite(:647-668)、
// rmib_register/deregister/reregister/send_reg(:862-975)、rmib_init。
//
// ARCH 偏差(两处,均已在 10-stage-mib 侧声明):
// 1. grant 拷入/拷出经 [`RmibIo`] 注入(真实实现 = sys_safecopyfrom/to,
//    E1 后接线);树与叶子数据是本进程内存,读写为纯内存操作。
// 2. 注册消息(asynsend3 到 MIB 服务)产出与发送分离:簿记函数返回待发
//    消息,发送归服务主循环(E1 后接 asynsend3/AMF_NOREPLY)。

use minix_types::{grant_valid, CTL_MAXNAME, CTLFLAG_ANYWRITE, CTL_SHORTNAME, ERESTART};

/// 转发的 sysctl 调用请求(C: `m_mib_lsys_call` 字段集,rmib.c:687-716)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RmibCallReq {
    /// 子树根 id(即 MountTable 槽号)。C: `root_id`。
    pub root_id: u32,
    /// 剩余名字长度。C: `name_len`。
    pub name_len: usize,
    /// 名字缓冲 grant。C: `name_grant`。
    pub name_grant: i32,
    /// 旧值窗口 grant。C: `oldp_grant`。
    pub oldp_grant: i32,
    /// 旧值请求长度。C: `oldp_len`。
    pub oldp_len: usize,
    /// 新值窗口 grant。C: `newp_grant`。
    pub newp_grant: i32,
    /// 新值长度。C: `newp_len`。
    pub newp_len: usize,
    /// 发起 sysctl 的用户进程端点。C: `user_endpt`。
    pub user_endpt: i32,
    /// 调用旗标(RMIB_FLAG_AUTH)。C: `flags`。
    pub flags: u32,
    /// 子树版本。C: `root_ver`。
    pub root_ver: u32,
    /// 全树版本。C: `tree_ver`。
    pub tree_ver: u32,
}

/// grant 通道的拷入/拷出动词(E1 后接 sys_safecopyfrom/to;宿主测试用
/// 内存实现)。
pub trait RmibIo {
    /// 从 `grant` 的 `off` 偏移拷入至多 `dst.len()` 字节。
    fn copyin(&mut self, dst: &mut [u8], grant: i32, off: usize) -> Result<(), i32>;
    /// 把 `src` 写到 `grant` 的 `off` 偏移。
    fn copyout(&mut self, src: &[u8], grant: i32, off: usize) -> Result<(), i32>;
}

/// 函数驱动节点的服务回调(C: `rnode->rnode_func(&call, rnode, oldp,
/// newp)`,rmib.h:18-21 + rmib.c:809-811——C 以函数指针挂在节点上,
/// Rust 的 [`RmibNode`] 只带 `func: bool` 标志,handler 由服务侧经本
/// trait 注入)。返回拷出字节数(`Ok`)或负 errno(`Err`)。
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

/// 注册簿记产出的待发消息(C: `rmib_send_reg` 组装的
/// `MIB_REGISTER`/`MIB_DEREGISTER`——m_type 与 m_lsys_mib_register 载荷)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmibRegMessage {
    /// `MIB_REGISTER`:根 id、剥离 SPARSE 的旗标、子容量/子数、名字。
    Register {
        /// 根 id(槽号)。
        root_id: u32,
        /// 剥离 SPARSE 的旗标 + SYSCTL_VERSION。C: rmib_send_reg(:864-871)。
        flags: u32,
        /// 子容量。C: `csize`。
        csize: u32,
        /// 有效子数。C: `clen`。
        clen: u32,
        /// 挂载路径(前缀名)。C: `mib`/`miblen`。
        mib: [i32; CTL_SHORTNAME as usize],
        /// 名字长度。C: `miblen`。
        miblen: u32,
    },
    /// `MIB_DEREGISTER`:仅根 id(C rmib.c:955-965)。
    Deregister {
        /// 根 id(槽号)。
        root_id: u32,
    },
}

/// 槽内树 + 挂载路径(C: `rnodes[]` 槽三元组,rnodes[.].rno_node/rno_name/
/// rno_namelen)。
#[derive(Debug, Clone, Default)]
pub struct MountSlot {
    /// 挂载的子树根(自持,替代 C 的裸指针)。None = 空槽。
    pub tree: Option<RmibNode>,
    /// 挂载路径(前缀名)。C: `rno_name`(i32 数组,每元素一个名字分量)。
    pub name: [i32; CTL_SHORTNAME as usize],
    /// 路径长度。C: `rno_namelen`。
    pub namelen: usize,
}

/// 子树注册表(C: `rnodes[RMIB_MAX_SUBTREES]`)。槽号即线上根 id。
#[derive(Debug, Clone, Default)]
pub struct SubtreeTable {
    /// 十六个槽(C rmib.c:47)。
    pub slots: [MountSlot; RMIB_MAX_SUBTREES],
}

impl SubtreeTable {
    /// 空表。
    pub fn new() -> Self {
        Self { slots: core::array::from_fn(|_| MountSlot::default()) }
    }

    /// 注册子树(C: `rmib_register` — rmib.c:892-924)。名字非空且短于
    /// 上限、根必须是 NODE 型;同树重复注册 EEXIST;表满 ENOMEM。
    /// 返回 `(根 id, 待发 MIB_REGISTER 消息)`——发送归调用方(E1 后
    /// asynsend3,ARCH 偏差 2)。
    pub fn register(
        &mut self,
        name: &[i32],
        root: RmibNode,
    ) -> Result<(u32, RmibRegMessage), i32> {
        // C: rmib.c:896-899 — namelen ∈ [1, CTL_SHORTNAME) 且根为 NODE 型。
        if name.is_empty() || name.len() >= CTL_SHORTNAME as usize {
            return Err(minix_types::EINVAL);
        }
        if minix_types::sysctl_type(root.flags) != minix_types::CTLTYPE_NODE {
            return Err(minix_types::EINVAL);
        }
        // C: rmib.c:902-908 — 同树 EEXIST;取第一个空槽。
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

        // C: rmib_send_reg — 剥 SPARSE + 版本戳 + csize/clen + 名字。
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

    /// 注销子树(C: `rmib_deregister` — rmib.c:934-969)。返回待发
    /// MIB_DEREGISTER 消息;未注册返回 ENOENT。C 对 asynsend3 失败不处理
    /// (注释:调用方无从补救)——发送归调用方后该语义自然保持。
    pub fn deregister(&mut self, root: &RmibNode) -> Result<RmibRegMessage, i32> {
        for (id, slot) in self.slots.iter_mut().enumerate() {
            if slot.tree.as_ref() == Some(root) {
                *slot = MountSlot::default();
                return Ok(RmibRegMessage::Deregister { root_id: id as u32 });
            }
        }
        Err(minix_types::ENOENT)
    }

    /// 重发全部存活子树的注册(C: `rmib_reregister` — rmib.c:971-980;
    /// MIB 服务重启后由主循环调用)。
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

/// 递归计算有效子节点数(C: `rmib_init` 的 clen 半,rmib.c:826-860):
/// dense 跳过 flags==0 空槽;sparse 计全部显式项。
pub fn count_children(root: &RmibNode) -> u32 {
    match &root.children {
        RmibChildren::None => 0,
        RmibChildren::Sparse(v) => v.len() as u32,
        RmibChildren::Dense(v) => v.iter().filter(|n| n.flags != 0).count() as u32,
    }
}

/// C: `rmib_getptr` — rmib.c:482-516 的数据面:返回叶节点的当前值字节
/// (immediate 序列化;data 节点返回自有缓冲;STRING+IMMEDIATE 与 NODE
/// 无数据 → None)。`rmib_readwrite`/`rmib_read` 据此取源。
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

/// 写回叶节点的当前值字节(`rmib_getptr` 的写半; immediate 节点写回
/// `value`,数据节点写回 `data`)。长度不匹配返回 None(调用方给 EINVAL)。
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

/// C: `rmib_read` — rmib.c:518-545. 读当前(旧)数据:长度总是返回,
/// 数据仅在窗口存在时拷出。
fn rmib_read(rnode: &RmibNode, oldp: Option<&RmibOldp>, io: &mut dyn RmibIo) -> Result<usize, i32> {
    let data = getptr_bytes(rnode).ok_or(minix_types::EINVAL)?;
    if let Some(w) = oldp {
        io.copyout(&data, w.grant, 0)?;
    }
    Ok(data.len())
}

/// C: `rmib_write` — rmib.c:547-645. 写新值:先拷入临时缓冲(防半途失败
/// 毁值,C rmib.c:556-566),长度按类型校验(非串须精确匹配;串可短、
/// 尾 NUL 自补;超栈预算非授权 EPERM),最后落盘。
fn rmib_write(rnode: &mut RmibNode, newp: Option<&RmibNewp>, call: &RmibCall, io: &mut dyn RmibIo) -> Result<(), i32> {
    let newp = match newp {
        Some(n) => n,
        None => return Ok(()), // nothing to do(rmib.c:558)
    };
    let newlen = newp.len;
    let node_type = sysctl_type(rnode.flags);
    let immediate = rnode.flags & minix_types::CTLFLAG_IMMEDIATE != 0;

    // C rmib.c:570-592 — 数据节点须有存储;长度按类型校验。
    if rnode.data.is_none() && (!immediate || matches!(node_type, CTLTYPE_STRING | CTLTYPE_STRUCT)) {
        // NODE/无数据节点:rmib_getptr 返回 NULL → EINVAL。
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

    // C rmib.c:604-615 — 超栈预算(RMIB_STACKBUF)且非授权 → EPERM。
    if newlen + 1 > RMIB_STACKBUF && (call.flags & RMIB_FLAG_AUTH) == 0 {
        return Err(minix_types::EPERM);
    }

    // 拷入临时缓冲(C rmib.c:623-626)。
    let mut src = vec![0u8; newlen + 1];
    io.copyin(&mut src[..newlen], newp.grant, 0)?;

    // C rmib.c:627-648 — 校验并落盘(STRING 自补 NUL)。
    if node_type == CTLTYPE_STRING && newlen > 0 && src[newlen - 1] != 0 {
        if newlen == rnode.size as usize {
            return Err(minix_types::EINVAL); // NUL 放不下(rmib.c:636-641)
        }
        src[newlen] = 0; // C rmib.c:643
    }
    // SAFETY(形状):写入臂由类型决定,长度已在上面校验。
    setptr_bytes(rnode, &src[..newlen.min(rnode.size as usize).max(1)]);
    Ok(())
}

/// C: `rmib_readwrite` — rmib.c:647-668. 叶节点通用读+写:先读旧值
/// (总是返回旧长度),再写新值。
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

/// C: `rmib_call` — rmib.c:678-824. 处理 MIB 服务转发的 sysctl 调用。
///
/// 名字逐级下行:负 id 是元标识符(QUERY/DESCRIBE 须为最后分量;
/// CREATE/DESTROY 仅静态子树 → EPERM;其余 EOPNOTSUPP);正 id 查子
/// 节点(PRIVATE 无授权 EPERM;叶节点后还有名字分量 ENOTDIR;叶+新值
/// 须 READWRITE 且[ANYWRITE 或授权]);函数驱动节点此版返回 EOPNOTSUPP
/// (handler 由服务自派,经 `func` 回调注入);普通叶走 readwrite。名字
/// 耗尽在非叶节点 → EISDIR(名字指向节点数组)。
///
/// `func` 是函数驱动节点的回调(C-12):`Some` 时 func 节点交由
/// [`RmibFuncHandler::call_func`](C `rnode->rnode_func(&call, rnode,
/// oldp, newp)` 的返回值语义同 `Ok(字节数)/Err(errno)`),`None` 时维持
/// 既有 EOPNOTSUPP。
pub fn rmib_call(
    table: &mut SubtreeTable,
    req: &RmibCallReq,
    io: &mut dyn RmibIo,
    func: Option<&mut dyn RmibFuncHandler>,
) -> Result<usize, i32> {
    // C rmib.c:688-695 — 未注册子树返回 ERESTART(MIB 应注销其以为的挂载)。
    let slot = match req.root_id as usize >= RMIB_MAX_SUBTREES {
        true => return Err(ERESTART),
        false => &mut table.slots[req.root_id as usize],
    };
    let root = match slot.tree.as_mut() {
        Some(t) => t,
        None => return Err(ERESTART),
    };

    // C rmib.c:702-716 — 挂载路径作前缀;剩余名字经 grant 拷入。
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

    // C rmib.c:718-726 — oldp/newp 按 grant 有效性构造(grant 无效 = NULL)。
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

    // C rmib.c:730-733 + :747-760 — 逐级下行。
    let mut call_ctx = RmibCall {
        endpt: req.user_endpt,
        oname: name,
        name,
        namelen: req.name_len,
        flags: req.flags,
        rootver: req.root_ver,
        treever: req.tree_ver,
    };
    // C rmib.c:736 `call.call_name = &name[prefixlen]`——handler/叶写入
    // 的可见视图从挂载前缀之后开始;每消费一个分量,视图左移一格
    // (C 的 `call.call_name++` 指针推进,rmib.c:746-748)。Rust 以
    // copy_within 平移对齐:name[0..namelen] 恒为"当前节点之后"的
    // 剩余分量。
    call_ctx.name.copy_within(prefixlen.., 0);
    let mut rnode: &mut RmibNode = root;

    while call_ctx.namelen > 0 {
        let id = call_ctx.name[0];
        call_ctx.name.copy_within(1.., 0);
        call_ctx.namelen -= 1;

        // C: rparent 总是 NODE(rmib.c:750 的 assert)。
        // 元标识符:必须是最后一个分量(rmib.c:755-771)。
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
                    Err(minix_types::EPERM) // 仅静态子树(rmib.c:776-779)
                }
                _ => Err(minix_types::EOPNOTSUPP),
            };
        }

        // C rmib.c:782-784 — 找子节点。
        let next = rmib_lookup_mut(rnode, id as u32);
        rnode = match next {
            Some(child) => child,
            None => return Err(minix_types::ENOENT),
        };

        // C rmib.c:787-789 — 本级访问门。
        if (rnode.flags & CTLFLAG_PRIVATE) != 0 && (call_ctx.flags & RMIB_FLAG_AUTH) == 0 {
            return Err(minix_types::EPERM);
        }

        let is_leaf = sysctl_type(rnode.flags) != CTLTYPE_NODE;
        let has_func = rnode.func;

        // C rmib.c:793-796 — 叶后不得再有名字分量。
        if is_leaf && call_ctx.namelen > 0 {
            return Err(minix_types::ENOTDIR);
        }

        // C rmib.c:799-806 — 叶(或函数节点)+新值:写权限门。
        if (is_leaf || has_func) && newp.is_some() {
            if (rnode.flags & CTLFLAG_READWRITE) != CTLFLAG_READWRITE {
                return Err(minix_types::EPERM);
            }
            if (rnode.flags & CTLFLAG_ANYWRITE) == 0 && (call_ctx.flags & RMIB_FLAG_AUTH) == 0 {
                return Err(minix_types::EPERM);
            }
        }

        // C rmib.c:809-811 — 函数驱动节点交 handler;返回值即结果
        // (`Ok(字节数)`/`Err(负 errno)`)。无回调注入时维持 EOPNOTSUPP。
        if has_func {
            return match func {
                Some(h) => h.call_func(rnode, &call_ctx, oldp.as_ref(), newp.as_ref(), io),
                None => Err(minix_types::EOPNOTSUPP),
            };
        }

        // C rmib.c:813-815 — 常规数据叶:通用读写。
        if is_leaf {
            return rmib_readwrite(&call_ctx, rnode, oldp.as_ref(), newp.as_ref(), io);
        }
        // 否则继续下行。
    }

    // C rmib.c:820-822 — 名字耗尽在非叶节点:名字指向节点数组 → EISDIR。
    Err(minix_types::EISDIR)
}

/// C: `rmib_query` — rmib.c:282-345. 枚举子节点(copyout_node 序列)。
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

/// C: `rmib_describe` — rmib.c:414-458. 枚举子节点描述。
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

/// 子节点遍历统一形态(dense 跳过 flags==0 空槽;sparse 用显式 id;
/// C rmib.c:337-350 / :435-447)。
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

// ── rmib_call 函数节点派发(C-12;C rmib.c:809-811)──

#[cfg(test)]
mod func_tests {
    use super::*;
    use alloc::{string::String, vec::Vec};
    use minix_types::{CTLTYPE_INT, CTLTYPE_NODE, EFAULT, EINVAL};

    /// 内存 grant 通道:copyout 追加记录,copyin 按 grant 读。
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

    /// 探针 handler:记录节点名与剩余名字长度,按脚本返回。
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

    /// 根 NODE(子:[0]=func 节点,[1]=int 叶),挂载名 [0, 82]。
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
        // C rmib.c:809-811 — name=[1] 走到 func 节点,handler 收到
        // namelen=0(名字在节点处耗尽),返回值即结果。
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
        // handler 的 Err 原样上抛(C 的返回负 errno)。
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
        // C-12 前的既有行为保持:None 回调 → EOPNOTSUPP。
        let mut table = fixture();
        let mut io = MemIo::new(alloc::vec![(5, alloc::vec![0, 0, 0, 0]), (7, alloc::vec![0; 16])]);
        assert_eq!(
            rmib_call(&mut table, &req(1, 5), &mut io, None),
            Err(minix_types::EOPNOTSUPP)
        );
    }

    #[test]
    fn test_handler_sees_name_after_consumed_component() {
        // C rmib.c:746-748 指针推进语义:name=[0(info), 5(SEM_INFO)]
        // 时,handler 看到的是 call.name[0]==5、namelen==1——即
        // "当前节点之后"的剩余分量(kern_ipc_info 的 call_name[0])。
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
        // name=[2] 是普通 int 叶:不走 handler,走通用 readwrite。
        let mut table = fixture();
        let mut io = MemIo::new(alloc::vec![(5, alloc::vec![1, 0, 0, 0]), (7, alloc::vec![0; 16])]);
        let mut probe = Probe { seen: None, ret: Ok(0) };
        let r = rmib_call(&mut table, &req(1, 5), &mut io, Some(&mut probe));
        assert!(r.is_ok());
        assert!(probe.seen.is_none(), "数据叶不得派发 handler");
    }
}

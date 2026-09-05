//! IPC filtering: system call mask, IPC target bitmap, and IPC filter pool.
//!
//! # Minix3 C Source Mapping
//!
//! - `const.h:20-27` — `get_sys_bit/set_sys_bit/unset_sys_bit` bitmap macros
//! - `priv.h:35,38,46,86,87` — `s_ipc_to`, `s_k_call_mask`, `s_ipcf` fields;
//!   `may_send_to`, `may_asynsend_to` macros
//! - `ipc.h:14-22` — `WILLRECEIVE`, `CANRECEIVE` macros (receive-time filter)
//! - `ipc.h:25-48` — `IPC_STATUS_GET/CLEAR/ADD/ADD_CALL/ADD_FLAGS` macros (status report)
//! - `ipc_filter.h` — `IPCF_NONE/BLACKLIST/WHITELIST`, `IPCF_POOL_*` macros,
//!   `struct ipc_filter_s` (filter chain node)
//! - `include/minix/ipc_filter.h` — `IPCF_MATCH_M_SOURCE/M_TYPE` flags,
//!   `struct ipc_filter_el_s` (filter element), `ANY_USR/SYS/TSK` endpoints
//! - `system.c:95-127` — `kernel_call_dispatch` (s_k_call_mask check at L111)
//! - `system.c:803-874` — `allow_ipc_filtered_msg` (fine-grained filter chain)
//!
//! # Design Decisions (23-ipc-filter.md §3)
//!
//! - **D1**: Inline functions instead of macros for bitmap operations
//! - **D2**: Standalone filter functions for testability
//! - **D3**: `u64` for s_k_call_mask (58 syscalls fit in one u64)
//! - **D4**: Return false + caller EPERM on filter failure (matches C ECALLDENIED)
//! - **D5**: `Option<IpcFilterSlot>` replaces C's `type == IPCF_NONE` sentinel.
//!   `None` = free slot (C: `IPCF_POOL_IS_FREE_SLOT`), `Some` = allocated.
//!   Eliminates the "type field as state flag" pattern — Rust's Option
//!   enforces "illegal states unrepresentable".
//! - **D6**: `Option<usize>` pool index replaces C's `*mut ipc_filter_s` raw pointer
//! - **D7**: bitflags for IPCF_MATCH_M_SOURCE/M_TYPE (type-safe composition)
//! - **D8**: `enum IpcFilterType { Blacklist, Whitelist }` (NONE expressed by Option)
//! - **D9**: IPC_STATUS mechanism — see `ipc_status_add_call`/`ipc_status_add_flags`
//!   in `proc.rs`. RECEIVE-path consumer wires status into the IPC status register
//!   (x86-64 RBX / aarch64 X1 / riscv64 A1) on `delivermsg`/`mini_send`/`mini_notify`.
//!   C: `IPC_STATUS_REG = bx` (i386) / `r1` (earm) — ipcconst.h:10,7.

use crate::kpriv::KPriv;

// ── Minix3 error codes ──

/// Operation not permitted. C: `EPERM`
pub const EPERM: i32 = 1;

/// Maximum system call number for mask checking.
/// C: `NR_SYS_CALLS` — com.h:270 (58 in Minix3)
pub const NR_SYS_CALLS: usize = 58;

// ── IPC filter functions ──

/// Check if a process may send IPC to a target process.
///
/// C: `may_send_to(rp, nr)` — priv.h:86
/// Expands to `get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr))` (const.h:20).
///
/// Checks the caller's `s_ipc_to` bitmap for the target's `s_id`.
/// All processes with `SYS_PROC` flag have their IPC targets filtered.
/// User processes share `USER_PRIV` which has limited `s_ipc_to`.
#[inline]
#[allow(dead_code)] // R-19 (2026-08-13): Wrapper is tested directly; will be
                    // wired into dispatch path when IPC filter pool is enabled.
pub(crate) fn ipc_filter_check(caller_priv: &KPriv, target_sys_id: u16) -> bool {
    caller_priv.may_send_to(target_sys_id)
}

/// Check if a process may invoke a specific kernel system call.
///
/// C: `GET_BIT(priv(caller)->s_k_call_mask, call_nr)` — system.c:111
/// Bitmap primitive `GET_BIT` (bitmap.h:16) — an independent macro mapping
/// directly into a plain `bitchunk_t` array; same expansion shape as
/// `get_sys_bit` (const.h:20) but NOT an alias (different argument types).
///
/// Uses the `s_k_call_mask` bitmap. Each bit corresponds to a system call number.
/// Returns `true` if the call is permitted, `false` otherwise.
/// C returns `ECALLDENIED` (errno.h:206) on denial — see syscall.rs `KcallResult::CallDenied`.
#[inline]
pub(crate) fn kcall_filter_check(caller_priv: &KPriv, call_nr: u32) -> bool {
    if call_nr as usize >= 64 {
        return false;
    }
    // s_k_call_mask is a KCallMask (capability.rs); bit i = kernel call i.
    caller_priv.ipc.s_k_call_mask.contains(crate::capability::KCallMask::from_bits(1u64 << call_nr))
}

/// Set a bit in the IPC target bitmap.
///
/// C: `set_sys_bit(map, bit)` — const.h:24
#[inline]
pub fn set_sys_bit(map: &mut u64, id: u16) {
    if (id as usize) < 64 {
        *map |= 1u64 << id;
    }
}

/// Clear a bit in the IPC target bitmap.
///
/// C: `unset_sys_bit(map, bit)` — const.h:26
#[inline]
pub fn unset_sys_bit(map: &mut u64, id: u16) {
    if (id as usize) < 64 {
        *map &= !(1u64 << id);
    }
}

/// Test a bit in the IPC target bitmap.
///
/// C: `get_sys_bit(map, bit)` — const.h:20
#[inline]
pub fn get_sys_bit(map: u64, id: u16) -> bool {
    if (id as usize) >= 64 {
        return false;
    }
    (map & (1u64 << id)) != 0
}

// ── IPC Filter Pool ──

/// IPC filter type. C: `IPCF_NONE/IPCF_BLACKLIST/IPCF_WHITELIST` — ipc_filter.h:13-15
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IpcFilterType {
    Blacklist,
    Whitelist,
}

/// IPC filter element flags. C: `IPCF_MATCH_M_SOURCE/IPCF_MATCH_M_TYPE` — include/minix/ipc_filter.h:18-19
pub(crate) struct IpcFilterElFlags;

impl IpcFilterElFlags {
    pub const MATCH_M_SOURCE: u32 = 0x1;
    pub const MATCH_M_TYPE: u32 = 0x2;
}

// ── D-16/D-18: filter match/check semantics (2026-09-06) ────────────────
//
// C: minix3/minix/kernel/ipc_filter.h:10-41 (IPCF_EL_CHECK / IPCF_EL_MATCH
// 宏链) + system.c:803-874 (allow_ipc_filtered_msg 链式遍历)。

/// Special filter endpoints matching a whole class.
/// C: `ANY_USR/ANY_SYS/ANY_TSK` — include/minix/ipc_filter.h:10-12
/// (`_ENDPOINT(1..3, _ENDPOINT_P(ANY))`)。
pub(crate) const ANY_USR: minix_types::Endpoint =
    minix_types::Endpoint::from_generation_slot(1, minix_types::Endpoint::ANY.slot());
pub(crate) const ANY_SYS: minix_types::Endpoint =
    minix_types::Endpoint::from_generation_slot(2, minix_types::Endpoint::ANY.slot());
pub(crate) const ANY_TSK: minix_types::Endpoint =
    minix_types::Endpoint::from_generation_slot(3, minix_types::Endpoint::ANY.slot());

/// Endpoint privilege class, resolved by the engine (needs the process
/// table + privilege table). C: `IPCF_IS_USR_EP/IPCF_IS_SYS_EP/
/// IPCF_IS_TSK_EP` — ipc_filter.h:26-33.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointClass {
    Usr,
    Sys,
    Task,
}

/// `IPCF_IS_ANY_EP(E)` — ipc_filter.h:34-35.
pub(crate) fn is_any_ep(ep: minix_types::Endpoint) -> bool {
    ep == ANY_USR || ep == ANY_SYS || ep == ANY_TSK
}

/// `IPCF_EL_CHECK(E)` — ipc_filter.h:19-25. 设置期元素合法性：至少设一个
/// MATCH 标志；设了 MATCH_M_SOURCE 时 m_source 必须是 ANY_* 或
/// `source_ok`（调用方预算的 isokendpt 结果——需要进程表解析）。
pub(crate) fn el_check(el: &IpcFilterElement, source_ok: bool) -> bool {
    let has_match = (el.flags & (IpcFilterElFlags::MATCH_M_SOURCE | IpcFilterElFlags::MATCH_M_TYPE)) != 0;
    let source_resolves = (el.flags & IpcFilterElFlags::MATCH_M_SOURCE) == 0
        || is_any_ep(minix_types::Endpoint(el.m_source))
        || source_ok;
    has_match && source_resolves
}

/// `IPCF_EL_MATCH(E, M)` — ipc_filter.h:40-41（= MATCH_M_TYPE && 
/// MATCH_M_SOURCE）。`class_of` 解析消息来源端点的特权类别（ANY_* 类别
/// 匹配需要它）。
pub(crate) fn el_match(
    el: &IpcFilterElement,
    m_source: minix_types::Endpoint,
    m_type: i32,
    class_of_source: &mut ClassResolver<'_>,
) -> bool {
    el_match_with(el, m_source, m_type, class_of_source)
}

fn el_match_with(
    el: &IpcFilterElement,
    m_source: minix_types::Endpoint,
    m_type: i32,
    class_of_source: &mut ClassResolver<'_>,
) -> bool {
    // IPCF_EL_MATCH_M_TYPE — ipc_filter.h:30-32.
    let type_ok =
        (el.flags & IpcFilterElFlags::MATCH_M_TYPE) == 0 || el.m_type == m_type;
    // IPCF_EL_MATCH_M_SOURCE — ipc_filter.h:32-38.
    let source_ok = (el.flags & IpcFilterElFlags::MATCH_M_SOURCE) == 0 || {
        let el_source = minix_types::Endpoint(el.m_source);
        el_source == m_source
            || match class_of_source(m_source) {
                Some(EndpointClass::Usr) => el_source == ANY_USR,
                Some(EndpointClass::Sys) => el_source == ANY_SYS,
                Some(EndpointClass::Task) => el_source == ANY_TSK,
                None => false,
            }
    };
    type_ok && source_ok
}

/// 消息来源端点的特权类别解析器（由 IpcEngine 注入：需要进程表 + 特权表）。
pub(crate) type ClassResolver<'a> = dyn FnMut(minix_types::Endpoint) -> Option<EndpointClass> + 'a;

/// D-16: `allow_ipc_filtered_msg` 的链式判定（C system.c:849-865）。
///
/// 语义（逐行对照 C）：初始 `allow = (head.type == IPCF_BLACKLIST)`；
/// 沿 `next` 链遍历每个 filter，当 `allow != (filter 是白名单)` 时扫描其
/// 元素，首个 `IPCF_EL_MATCH` 命中即翻转 `allow = (filter 是白名单)`
/// （内层 break；**外层链遍历继续**——后序异类 filter 可再次翻转，顺序
/// 即优先级）。无 filter（head 为 None）→ 恒允许（C :810-812）。
pub(crate) fn chain_allowed(
    pool: &IpcFilterPool,
    head: Option<usize>,
    m_source: minix_types::Endpoint,
    m_type: i32,
    class_of_source: &mut ClassResolver,
) -> bool {
    if head.is_none() {
        return true;
    }
    let head_type = pool.get(head.unwrap()).map(|s| s.filter_type);
    let mut cur = head;
    let mut allow = matches!(head_type, Some(IpcFilterType::Blacklist));
    while let Some(idx) = cur {
        let Some(slot) = pool.get(idx) else { break };
        let is_whitelist = slot.filter_type == IpcFilterType::Whitelist;
        if allow != is_whitelist {
            for el in &slot.elements[..slot.num_elements] {
                if el_match_with(el, m_source, m_type, class_of_source) {
                    allow = is_whitelist;
                    break;
                }
            }
        }
        cur = slot.next;
    }
    allow
}

/// 释放从 `head` 开始的整条 filter 链（C `clear_ipc_filters`，
/// system.c:751-770 的链遍历语义）。返回释放的槽位数。
pub(crate) fn free_chain(pool: &mut IpcFilterPool, head: Option<usize>) -> usize {
    let mut freed = 0;
    let mut cur = head;
    while let Some(idx) = cur {
        let next = pool.get(idx).and_then(|s| s.next);
        pool.free(idx);
        freed += 1;
        cur = next;
    }
    freed
}

/// 在链尾追加一个槽位（C add_ipc_filter system.c:742-745 的
/// "for (*ipcfp = &priv->s_ipcf; *ipcfp != NULL; ipcfp = &(*ipcfp)->next)"
/// 尾插语义）。返回新的链头（调用方回写 `s_ipcf`）。
pub(crate) fn append_to_chain(
    pool: &mut IpcFilterPool,
    head: Option<usize>,
    new_idx: usize,
) -> Option<usize> {
    let Some(head) = head else { return Some(new_idx) };
    let mut cur = Some(head);
    while let Some(idx) = cur {
        let next = pool.get(idx).and_then(|s| s.next);
        match next {
            Some(n) => cur = Some(n),
            None => {
                if let Some(slot) = pool.get_mut(idx) {
                    slot.next = Some(new_idx);
                }
                break;
            }
        }
    }
    Some(head)
}

/// A single IPC filter element. C: `ipc_filter_el_s` — include/minix/ipc_filter.h:23-27
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct IpcFilterElement {
    pub flags: u32,
    pub m_source: i32,
    pub m_type: i32,
}

/// Maximum number of elements per filter.
/// C: `IPCF_MAX_ELEMENTS = NR_SYS_PROCS * 2` — include/minix/ipc_filter.h:15
pub const IPCF_MAX_ELEMENTS: usize = crate::kpriv::NR_SYS_PROCS * 2;

/// A single allocated IPC filter slot. C: `ipc_filter_s` — ipc_filter.h:43-49
///
/// In C, `type == IPCF_NONE` means "free slot". In Rust, we use
/// `Option<IpcFilterSlot>` where `None` = free, `Some` = allocated.
/// This eliminates the sentinel-value pattern (D5).
#[derive(Debug)]
pub(crate) struct IpcFilterSlot {
    /// C: `type` field in `ipc_filter_s` (ipc_filter.h:42). Stored for
    /// debugging/introspection but not read in current filter logic.
    #[allow(dead_code)]
    pub filter_type: IpcFilterType,
    pub num_elements: usize,
    /// C: `flags` field in `ipc_filter_s` (ipc_filter.h:45). Reserved for
    /// future filter-chain flags (IPCF_MATCH_M_SOURCE etc.). Currently
    /// unused — filter chaining not yet implemented (FIX-02: R-13).
    #[allow(dead_code)]
    pub flags: i32,
    /// Index of next filter in chain, if any. C: `struct ipc_filter_s *next`
    /// (ipc_filter.h:48). Stored as Option<usize> index into the pool
    /// instead of a raw pointer. Currently unused — filter chaining not
    /// yet implemented (FIX-02: R-13).
    #[allow(dead_code)]
    pub next: Option<usize>,
    pub elements: [IpcFilterElement; IPCF_MAX_ELEMENTS],
}

impl IpcFilterSlot {
    fn new(filter_type: IpcFilterType) -> Self {
        Self {
            filter_type,
            num_elements: 0,
            flags: 0,
            next: None,
            elements: [IpcFilterElement {
                flags: 0,
                m_source: 0,
                m_type: 0,
            }; IPCF_MAX_ELEMENTS],
        }
    }
}

/// Size of the IPC filter pool. C: `IPCF_POOL_SIZE = 2 * NR_SYS_PROCS` — ipc_filter.h:53
const IPCF_POOL_SIZE: usize = 2 * crate::kpriv::NR_SYS_PROCS;

/// IPC filter pool — a fixed-size array of filter slots.
///
/// C: `ipc_filter_pool[IPCF_POOL_SIZE]` — ipc_filter.h:54
/// C init: `IPCF_POOL_INIT()` = `memset(&ipc_filter_pool, 0, ...)` — ipc_filter.h:71
///
/// In Rust, the pool starts with all `None` slots (equivalent to C's `memset(0)`
/// which sets `type = IPCF_NONE = 0` = free). Allocation finds the first `None`
/// slot and replaces it with `Some(IpcFilterSlot)`. Deallocation sets it back
/// to `None`.
pub(crate) struct IpcFilterPool {
    slots: [Option<IpcFilterSlot>; IPCF_POOL_SIZE],
}

const NONE_SLOT: Option<IpcFilterSlot> = None;

impl IpcFilterPool {
    /// Create a new empty IPC filter pool.
    ///
    /// Equivalent to C's `IPCF_POOL_INIT()` (memset to zero).
    /// All slots start as `None` (= free / `IPCF_NONE`).
    pub const fn new() -> Self {
        Self {
            slots: [NONE_SLOT; IPCF_POOL_SIZE],
        }
    }

    /// Allocate a filter slot of the given type.
    ///
    /// C: `IPCF_POOL_ALLOCATE_SLOT(type, &slot)` — ipc_filter.h:59-70
    ///
    /// Returns `Some(index)` on success, `None` if pool is exhausted.
    /// The index can be stored in `KPriv::s_ipcf` (C: `priv->s_ipcf`).
    pub(crate) fn allocate(&mut self, filter_type: IpcFilterType) -> Option<usize> {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(IpcFilterSlot::new(filter_type));
                return Some(i);
            }
        }
        None
    }

    /// Free a filter slot by index.
    ///
    /// C: `IPCF_POOL_FREE_SLOT(slot)` = `(slot)->type = IPCF_NONE` — ipc_filter.h:57
    pub(crate) fn free(&mut self, index: usize) {
        if index < IPCF_POOL_SIZE {
            self.slots[index] = None;
        }
    }

    /// Get a reference to a filter slot by index.
    #[allow(dead_code)] // accessor for future filter lookup; not yet wired
    pub(crate) fn get(&self, index: usize) -> Option<&IpcFilterSlot> {
        self.slots.get(index).and_then(|s| s.as_ref())
    }

    /// Get a mutable reference to a filter slot by index.
    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut IpcFilterSlot> {
        self.slots.get_mut(index).and_then(|s| s.as_mut())
    }

    /// Number of currently allocated slots.
    #[cfg(test)]
    pub(crate) fn allocated_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kpriv::KPriv;

    #[test]
    fn test_ipc_filter_check_allowed() {
        let mut caller = KPriv::new(0);
        caller.ipc.s_ipc_to = crate::capability::IpcMask::from_bits(1 << 5); // can send to sys_id=5

        assert!(ipc_filter_check(&caller, 5));
        assert!(!ipc_filter_check(&caller, 3));
    }

    #[test]
    fn test_ipc_filter_check_no_targets() {
        let caller = KPriv::new(0);
        // s_ipc_to = NONE → cannot send to anyone
        assert!(!ipc_filter_check(&caller, 0));
        assert!(!ipc_filter_check(&caller, 5));
    }

    #[test]
    fn test_kcall_filter_check() {
        let mut caller = KPriv::new(0);
        caller.ipc.s_k_call_mask = crate::capability::KCallMask::from_bits(0xFF); // allow syscalls 0-7

        assert!(kcall_filter_check(&caller, 0));
        assert!(kcall_filter_check(&caller, 7));
        assert!(!kcall_filter_check(&caller, 8));
        assert!(!kcall_filter_check(&caller, 32));
    }

    #[test]
    fn test_kcall_filter_check_out_of_range() {
        let caller = KPriv::new(0);
        assert!(!kcall_filter_check(&caller, 64));
        assert!(!kcall_filter_check(&caller, 100));
    }

    #[test]
    fn test_sys_bit_operations() {
        let mut map = 0u64;
        set_sys_bit(&mut map, 5);
        assert!(get_sys_bit(map, 5));
        assert!(!get_sys_bit(map, 3));

        unset_sys_bit(&mut map, 5);
        assert!(!get_sys_bit(map, 5));
    }

    #[test]
    fn test_sys_bit_out_of_range() {
        let mut map = 0u64;
        set_sys_bit(&mut map, 64); // should be no-op
        assert_eq!(map, 0);
        assert!(!get_sys_bit(map, 64));
    }

    #[test]
    fn test_ipc_filter_pool_new_is_empty() {
        let pool = IpcFilterPool::new();
        assert_eq!(pool.allocated_count(), 0);
    }

    #[test]
    fn test_ipc_filter_pool_allocate_and_free() {
        let mut pool = IpcFilterPool::new();

        // Allocate a blacklist slot
        let idx = pool.allocate(IpcFilterType::Blacklist);
        assert!(idx.is_some());
        let idx = idx.unwrap();
        assert_eq!(idx, 0);
        assert_eq!(pool.allocated_count(), 1);

        // Verify the slot content
        let slot = pool.get(idx).unwrap();
        assert_eq!(slot.filter_type, IpcFilterType::Blacklist);
        assert_eq!(slot.num_elements, 0);

        // Allocate a whitelist slot
        let idx2 = pool.allocate(IpcFilterType::Whitelist);
        assert!(idx2.is_some());
        assert_eq!(idx2.unwrap(), 1);
        assert_eq!(pool.allocated_count(), 2);

        // Free the first slot
        pool.free(idx);
        assert_eq!(pool.allocated_count(), 1);
        assert!(pool.get(idx).is_none());

        // Allocate again — should reuse freed slot
        let idx3 = pool.allocate(IpcFilterType::Blacklist);
        assert_eq!(idx3, Some(0));
    }

    #[test]
    fn test_ipc_filter_pool_free_out_of_range_is_noop() {
        let mut pool = IpcFilterPool::new();
        pool.free(9999); // should not panic
        assert_eq!(pool.allocated_count(), 0);
    }

    // ── D-16/D-18: match/check/chain 单元测试（2026-09-06）──

    /// canned 类别解析器：usr 端点 → Usr，sys/task 由测试直接给定。
    fn usr_class(_ep: minix_types::Endpoint) -> Option<EndpointClass> {
        Some(EndpointClass::Usr)
    }

    #[test]
    fn test_el_check_requires_a_match_flag() {
        let no_flags = IpcFilterElement { flags: 0, m_source: 0, m_type: 0 };
        // C ipc_filter.h:19-25 — 无任何 MATCH 标志 → 非法。
        assert!(!el_check(&no_flags, true));
        let with_type = IpcFilterElement { flags: IpcFilterElFlags::MATCH_M_TYPE, m_source: 0, m_type: 5 };
        assert!(el_check(&with_type, true));
        // MATCH_M_SOURCE + 不可解析来源（isokendpt 失败）→ 非法。
        let bad_source = IpcFilterElement { flags: IpcFilterElFlags::MATCH_M_SOURCE, m_source: 0x7FFF, m_type: 0 };
        assert!(!el_check(&bad_source, false));
        // MATCH_M_SOURCE + ANY_* → 免 isokendpt。
        let any_source = IpcFilterElement { flags: IpcFilterElFlags::MATCH_M_SOURCE, m_source: ANY_USR.0, m_type: 0 };
        assert!(el_check(&any_source, false));
    }

    #[test]
    fn test_el_match_type_and_source() {
        let el = IpcFilterElement {
            flags: IpcFilterElFlags::MATCH_M_SOURCE | IpcFilterElFlags::MATCH_M_TYPE,
            m_source: 0x100, // 具体来源
            m_type: 42,
        };
        assert!(el_match(&el, minix_types::Endpoint(0x100), 42, &mut usr_class));
        // 类型不匹配 → 不命中。
        assert!(!el_match(&el, minix_types::Endpoint(0x100), 43, &mut usr_class));
        // 来源不匹配 → 不命中。
        assert!(!el_match(&el, minix_types::Endpoint(0x200), 42, &mut usr_class));
    }

    #[test]
    fn test_el_match_any_class_endpoints() {
        // ANY_USR 命中 Usr 类消息来源；对 Sys/Task 不命中。
        let el = IpcFilterElement {
            flags: IpcFilterElFlags::MATCH_M_SOURCE,
            m_source: ANY_USR.0,
            m_type: 0,
        };
        assert!(el_match(&el, minix_types::Endpoint(0x100), 0, &mut |_| Some(EndpointClass::Usr)));
        assert!(!el_match(&el, minix_types::Endpoint(0x100), 0, &mut |_| Some(EndpointClass::Sys)));
        assert!(!el_match(&el, minix_types::Endpoint(0x100), 0, &mut |_| Some(EndpointClass::Task)));
        // ANY_TSK 对 Task 命中。
        let el_tsk = IpcFilterElement { flags: IpcFilterElFlags::MATCH_M_SOURCE, m_source: ANY_TSK.0, m_type: 0 };
        assert!(el_match(&el_tsk, minix_types::Endpoint(0x100), 0, &mut |_| Some(EndpointClass::Task)));
    }

    #[test]
    fn test_chain_allowed_whitelist_blocks_unlisted() {
        // 链 = [whitelist{m_source=A}]：A 允许，B 拒绝。
        let mut pool = IpcFilterPool::new();
        let idx = pool.allocate(IpcFilterType::Whitelist).unwrap();
        if let Some(slot) = pool.get_mut(idx) {
            slot.num_elements = 1;
            slot.elements[0] = IpcFilterElement {
                flags: IpcFilterElFlags::MATCH_M_SOURCE,
                m_source: 0x100,
                m_type: 0,
            };
        }
        assert!(chain_allowed(&pool, Some(idx), minix_types::Endpoint(0x100), 0, &mut usr_class));
        assert!(!chain_allowed(&pool, Some(idx), minix_types::Endpoint(0x200), 0, &mut usr_class));
    }

    #[test]
    fn test_chain_allowed_blacklist_and_order_flip() {
        // 单黑名单：命中即拒。
        let mut pool = IpcFilterPool::new();
        let bl = pool.allocate(IpcFilterType::Blacklist).unwrap();
        if let Some(slot) = pool.get_mut(bl) {
            slot.num_elements = 1;
            slot.elements[0] = IpcFilterElement {
                flags: IpcFilterElFlags::MATCH_M_SOURCE,
                m_source: 0x100,
                m_type: 0,
            };
        }
        assert!(!chain_allowed(&pool, Some(bl), minix_types::Endpoint(0x100), 0, &mut usr_class));
        assert!(chain_allowed(&pool, Some(bl), minix_types::Endpoint(0x200), 0, &mut usr_class));

        // 链 [wl{A}, bl{A}]：白名单放行后被黑名单翻回——顺序即优先级
        // （C system.c:849-865 的外层遍历不因翻转提前退出）。
        let wl = pool.allocate(IpcFilterType::Whitelist).unwrap();
        {
            let slot = pool.get_mut(wl).unwrap();
            slot.num_elements = 1;
            slot.elements[0] = IpcFilterElement {
                flags: IpcFilterElFlags::MATCH_M_SOURCE,
                m_source: 0x100,
                m_type: 0,
            };
            slot.next = Some(bl);
        }
        assert!(!chain_allowed(&pool, Some(wl), minix_types::Endpoint(0x100), 0, &mut usr_class));
    }

    #[test]
    fn test_free_chain_frees_whole_chain() {
        let mut pool = IpcFilterPool::new();
        let a = pool.allocate(IpcFilterType::Whitelist).unwrap();
        let b = pool.allocate(IpcFilterType::Blacklist).unwrap();
        if let Some(slot) = pool.get_mut(a) {
            slot.next = Some(b);
        }
        assert_eq!(free_chain(&mut pool, Some(a)), 2);
        assert_eq!(pool.allocated_count(), 0);
    }
}
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
    // s_k_call_mask is [u32; SYS_CALL_MASK_SIZE], combine into u64 for easy bit testing
    let mask = caller_priv.ipc.s_k_call_mask[0] as u64
        | ((caller_priv.ipc.s_k_call_mask[1] as u64) << 32);
    (mask & (1u64 << call_nr)) != 0
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
#[allow(dead_code)] // FIX-02 future feature; filter element flags, not yet wired
pub(crate) struct IpcFilterElFlags;

#[allow(dead_code)] // FIX-02 future feature; grouped dead_code on associated constants
impl IpcFilterElFlags {
    pub const MATCH_M_SOURCE: u32 = 0x1;
    pub const MATCH_M_TYPE: u32 = 0x2;
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
        caller.ipc.s_ipc_to = 1 << 5; // can send to sys_id=5

        assert!(ipc_filter_check(&caller, 5));
        assert!(!ipc_filter_check(&caller, 3));
    }

    #[test]
    fn test_ipc_filter_check_no_targets() {
        let caller = KPriv::new(0);
        // s_ipc_to = 0 → cannot send to anyone
        assert!(!ipc_filter_check(&caller, 0));
        assert!(!ipc_filter_check(&caller, 5));
    }

    #[test]
    fn test_kcall_filter_check() {
        let mut caller = KPriv::new(0);
        caller.ipc.s_k_call_mask[0] = 0xFF; // allow syscalls 0-7
        caller.ipc.s_k_call_mask[1] = 0;    // deny syscalls 32-63

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
}

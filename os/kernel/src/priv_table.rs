//! Privilege table and privilege access methods.
//!
//! Corresponds to macros and data structures in Minix3's `kernel/priv.h`.
//! Transforms C macro implicit conventions into Rust explicit type constraints.
//!
//! # C Macro → Rust Method Mapping
//!
//! | C Macro | Rust Method |
//! |---------|-------------|
//! | `priv_addr(i)` | `PrivTable::get(id)` |
//! | `priv_id(rp)` | `KProcess::priv_id()` |
//! | `priv(rp)` | `PrivTable::get(id)` |
//! | `id_to_nr(id)` | `PrivTable::id_to_nr(id)` |
//! | `nr_to_id(nr)` | `PrivTable::nr_to_id(nr)` |
//! | `may_send_to(rp, nr)` | `PrivTable::may_send_to(priv, nr)` |
//! | `may_asynsend_to(rp, nr)` | `PrivTable::may_asynsend_to(priv, caller_nr, nr)` |
//! | `BEG_STATIC_PRIV_ADDR` | `PrivTable::static_slots()` |
//! | `BEG_DYN_PRIV_ADDR` | `PrivTable::dynamic_slots()` |

use minix_types::Bitmap;

/// Maximum number of system processes (corresponds to NR_SYS_PROCS = 64).
pub const NR_SYS_PROCS: usize = 64;

/// Number of boot processes (corresponds to NR_BOOT_PROCS = NR_TASKS + LAST_SPECIAL_PROC_NR + 1).
pub const NR_BOOT_PROCS: usize = 5 + 11 + 1;

/// User privilege ID (privilege slot index shared by all user processes).
pub const USER_PRIV_ID: u8 = 0;

/// Process number NONE constant (corresponds to C's NONE / NO_PROC_NR).
pub const PROC_NR_NONE: i32 = -1;

/// Compile-time check: NR_BOOT_PROCS must not exceed NR_SYS_PROCS.
const _: () = assert!(NR_BOOT_PROCS <= NR_SYS_PROCS);

/// Privilege ID newtype.
///
/// Corresponds to C's `sys_id_t`, uses newtype to prevent confusion with `ProcNr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SysId(pub u8);

impl SysId {
    /// Whether this is a static privilege ID (corresponds to `is_static_priv_id`).
    pub const fn is_static(self) -> bool {
        (self.0 as usize) < NR_BOOT_PROCS
    }

    /// Whether this is a valid privilege ID.
    pub const fn is_valid(self) -> bool {
        (self.0 as usize) < NR_SYS_PROCS
    }
}

/// Privilege flags (corresponds to C's `s_flags` field).
#[derive(Debug, Clone, Copy, Default)]
pub struct PrivFlags(pub u16);

impl PrivFlags {
    /// System process flag.
    pub const SYS_PROC: u16 = 0x001;
    /// Billable flag.
    pub const BILLABLE: u16 = 0x002;
    /// Preemptible flag.
    pub const PREEMPTIBLE: u16 = 0x004;
    /// Dynamic privilege ID flag (corresponds to DYN_PRIV_ID).
    pub const DYN_PRIV_ID: u16 = 0x010;

    pub fn contains(self, flag: u16) -> bool {
        (self.0 & flag) != 0
    }

    pub fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }

    pub fn remove(&mut self, flag: u16) {
        self.0 &= !flag;
    }
}

/// Privilege structure.
///
/// Corresponds to Minix3's `struct priv`.
/// Each system process has its own Priv, all user processes share the Priv at `USER_PRIV_ID`.
///
/// Current implementation is minimal, containing only core fields needed for privilege table access.
#[derive(Debug)]
pub struct Priv {
    /// Associated process number (`PROC_NR_NONE` means idle).
    pub s_proc_nr: core::sync::atomic::AtomicI32,
    /// Privilege table index.
    pub s_id: SysId,
    /// Privilege flags.
    pub s_flags: PrivFlags,
    /// IPC target mask (corresponds to `s_ipc_to`).
    pub s_ipc_to: Bitmap,
    /// Kernel call mask (corresponds to `s_k_call_mask`).
    pub s_k_call_mask: Bitmap,
}

impl Priv {
    /// Creates empty privilege slot.
    fn empty(id: SysId) -> Self {
        Self {
            s_proc_nr: core::sync::atomic::AtomicI32::new(PROC_NR_NONE),
            s_id: id,
            s_flags: PrivFlags::default(),
            s_ipc_to: Bitmap::new(NR_SYS_PROCS),
            s_k_call_mask: Bitmap::new(NR_SYS_PROCS),
        }
    }

    /// Whether slot is idle (`s_proc_nr == PROC_NR_NONE`).
    pub fn is_empty(&self) -> bool {
        self.s_proc_nr.load(core::sync::atomic::Ordering::Acquire) == PROC_NR_NONE
    }

    /// Whether this is a system process.
    pub fn is_sys_proc(&self) -> bool {
        self.s_flags.contains(PrivFlags::SYS_PROC)
    }
}

/// Privilege table.
///
/// Wraps `priv[]` and `ppriv_addr[]`, provides type-safe access interface.
/// Corresponds to C's global array at `BEG_PRIV_ADDR..END_PRIV_ADDR`.
///
/// # Static vs Dynamic Partition
///
/// ```text
/// slots[0..NR_BOOT_PROCS]           → Static privilege slots (boot processes)
/// slots[NR_BOOT_PROCS..NR_SYS_PROCS] → Dynamic privilege slots (runtime services)
/// ```
pub struct PrivTable {
    slots: [Priv; NR_SYS_PROCS],
}

impl PrivTable {
    /// Creates empty privilege table (all slots idle).
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|i| Priv::empty(SysId(i as u8))),
        }
    }

    /// Gets privilege structure by privilege ID (corresponds to `priv_addr(i)`).
    ///
    /// Returns `None` if ID is out of bounds.
    pub fn get(&self, id: SysId) -> Option<&Priv> {
        if id.is_valid() {
            Some(&self.slots[id.0 as usize])
        } else {
            None
        }
    }

    /// Gets mutable privilege structure reference by privilege ID.
    pub fn get_mut(&mut self, id: SysId) -> Option<&mut Priv> {
        if id.is_valid() {
            Some(&mut self.slots[id.0 as usize])
        } else {
            None
        }
    }

    /// Static privilege slots slice (corresponds to `BEG_STATIC_PRIV_ADDR..END_STATIC_PRIV_ADDR`).
    pub fn static_slots(&self) -> &[Priv] {
        &self.slots[..NR_BOOT_PROCS]
    }

    /// Dynamic privilege slots slice (corresponds to `BEG_DYN_PRIV_ADDR..END_DYN_PRIV_ADDR`).
    pub fn dynamic_slots(&self) -> &[Priv] {
        &self.slots[NR_BOOT_PROCS..]
    }

    /// Dynamic region allocation (corresponds to `get_priv` with `priv_id==NULL_PRIV_ID` branch).
    ///
    /// Linear scan for idle slot in dynamic region, returns allocated `SysId`.
    /// Returns `ENOSPC` if no idle slot.
    pub fn alloc_dynamic(&mut self, proc_nr: i32) -> Result<SysId, i32> {
        for (i, slot) in self.slots[NR_BOOT_PROCS..].iter_mut().enumerate() {
            if slot.is_empty() {
                let id = SysId((NR_BOOT_PROCS + i) as u8);
                slot.s_proc_nr.store(proc_nr, core::sync::atomic::Ordering::Release);
                return Ok(id);
            }
        }
        Err(12) // ENOSPC
    }

    /// Static region allocation (corresponds to `get_priv` with specified `priv_id` branch).
    pub fn alloc_static(&mut self, id: SysId, proc_nr: i32) -> Result<(), i32> {
        if !id.is_static() {
            return Err(22); // EINVAL
        }
        let slot = &mut self.slots[id.0 as usize];
        if !slot.is_empty() {
            return Err(16); // EBUSY
        }
        slot.s_proc_nr.store(proc_nr, core::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// Privilege ID → process number (corresponds to `id_to_nr`).
    pub fn id_to_nr(&self, id: SysId) -> Option<i32> {
        self.get(id)
            .map(|p| p.s_proc_nr.load(core::sync::atomic::Ordering::Acquire))
    }

    /// Process number → privilege ID (corresponds to `nr_to_id`).
    ///
    /// Scans privilege table for slot where `s_proc_nr == nr`.
    /// Note: In C this is accessed indirectly via `proc_addr + priv`, here simplified to linear search.
    pub fn nr_to_id(&self, nr: i32) -> Option<SysId> {
        self.slots
            .iter()
            .find(|p| p.s_proc_nr.load(core::sync::atomic::Ordering::Acquire) == nr)
            .map(|p| p.s_id)
    }

    /// Checks send permission (corresponds to `may_send_to`).
    ///
    /// Returns `false` when:
    /// - Corresponding bit not set in bitmap (no permission)
    /// - Privilege ID out of bounds (safe degradation, not UB)
    /// - Target process number not in privilege table
    pub fn may_send_to(&self, caller_priv: &Priv, target_nr: i32) -> bool {
        let target_id = match self.nr_to_id(target_nr) {
            Some(id) => id,
            None => return false,
        };
        caller_priv.s_ipc_to.get(target_id.0 as usize)
    }

    /// Checks async send permission (corresponds to `may_asynsend_to`).
    ///
    /// In addition to `may_send_to`, also allows process to send async message to itself.
    pub fn may_asynsend_to(
        &self,
        caller_priv: &Priv,
        caller_nr: i32,
        target_nr: i32,
    ) -> bool {
        self.may_send_to(caller_priv, target_nr) || caller_nr == target_nr
    }

    /// Releases privilege slot (called when process terminates).
    pub fn free(&mut self, id: SysId) {
        if let Some(slot) = self.get_mut(id) {
            slot.s_proc_nr
                .store(PROC_NR_NONE, core::sync::atomic::Ordering::Release);
            slot.s_flags = PrivFlags::default();
            slot.s_ipc_to.clear();
            slot.s_k_call_mask.clear();
        }
    }
}

impl Default for PrivTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sys_id_is_static() {
        assert!(SysId(0).is_static());
        assert!(SysId((NR_BOOT_PROCS - 1) as u8).is_static());
        assert!(!SysId(NR_BOOT_PROCS as u8).is_static());
        assert!(!SysId(63).is_static());
    }

    #[test]
    fn test_sys_id_is_valid() {
        assert!(SysId(0).is_valid());
        assert!(SysId(63).is_valid());
    }

    #[test]
    fn test_priv_table_new_all_empty() {
        let table = PrivTable::new();
        for slot in &table.slots {
            assert!(slot.is_empty());
        }
    }

    #[test]
    fn test_priv_table_static_dynamic_split() {
        let table = PrivTable::new();
        assert_eq!(table.static_slots().len(), NR_BOOT_PROCS);
        assert_eq!(table.dynamic_slots().len(), NR_SYS_PROCS - NR_BOOT_PROCS);
    }

    #[test]
    fn test_priv_table_get_by_id() {
        let table = PrivTable::new();
        assert!(table.get(SysId(0)).is_some());
        assert!(table.get(SysId(63)).is_some());
    }

    #[test]
    fn test_alloc_static() {
        let mut table = PrivTable::new();
        let id = SysId(3);
        assert!(table.alloc_static(id, 100).is_ok());
        assert!(!table.get(id).unwrap().is_empty());

        // Repeated allocation should return EBUSY
        assert_eq!(table.alloc_static(id, 101), Err(16));
    }

    #[test]
    fn test_alloc_static_invalid() {
        let mut table = PrivTable::new();
        // Dynamic region ID not allowed for static allocation
        let id = SysId(NR_BOOT_PROCS as u8);
        assert_eq!(table.alloc_static(id, 100), Err(22)); // EINVAL
    }

    #[test]
    fn test_alloc_dynamic() {
        let mut table = PrivTable::new();
        let id = table.alloc_dynamic(200).unwrap();
        assert!(id.0 as usize >= NR_BOOT_PROCS);
        assert!(!table.get(id).unwrap().is_empty());

        // id_to_nr should return correct process number
        assert_eq!(table.id_to_nr(id), Some(200));
    }

    #[test]
    fn test_alloc_dynamic_exhausted() {
        let mut table = PrivTable::new();
        // Fill all dynamic slots
        for i in 0..(NR_SYS_PROCS - NR_BOOT_PROCS) {
            let nr = (i + 100) as i32;
            table.alloc_dynamic(nr).unwrap();
        }
        // Next allocation should fail
        assert_eq!(table.alloc_dynamic(999), Err(12)); // ENOSPC
    }

    #[test]
    fn test_id_to_nr_and_nr_to_id() {
        let mut table = PrivTable::new();
        let id = SysId(5);
        table.alloc_static(id, -5).unwrap();

        // id_to_nr
        assert_eq!(table.id_to_nr(id), Some(-5));

        // nr_to_id
        assert_eq!(table.nr_to_id(-5), Some(id));
    }

    #[test]
    fn test_may_send_to() {
        let mut table = PrivTable::new();

        // Set up target process
        let target_id = SysId(4);
        table.alloc_static(target_id, -4).unwrap();

        // Set up sender's IPC mask
        let caller_id = SysId(0);
        table.alloc_static(caller_id, 0).unwrap();
        let caller_priv = table.get_mut(caller_id).unwrap();
        caller_priv.s_ipc_to.set(target_id.0 as usize, true);
        caller_priv.s_flags.insert(PrivFlags::SYS_PROC);

        let caller_priv = table.get(caller_id).unwrap();
        assert!(table.may_send_to(caller_priv, -4)); // Allowed
        assert!(!table.may_send_to(caller_priv, -3)); // Not authorized
        assert!(!table.may_send_to(caller_priv, -99)); // Non-existent process
    }

    #[test]
    fn test_may_asynsend_to_self() {
        let mut table = PrivTable::new();
        let id = SysId(0);
        table.alloc_static(id, 0).unwrap();

        let caller_priv = table.get(id).unwrap();
        // Self-send allowed even if s_ipc_to doesn't have own bit set
        assert!(table.may_asynsend_to(caller_priv, 0, 0));
    }

    #[test]
    fn test_may_asynsend_to_not_self() {
        let mut table = PrivTable::new();
        let caller_id = SysId(0);
        table.alloc_static(caller_id, 0).unwrap();

        // Target doesn't exist, not self → deny
        let caller_priv = table.get(caller_id).unwrap();
        assert!(!table.may_asynsend_to(caller_priv, 0, 42));
    }

    #[test]
    fn test_free_slot() {
        let mut table = PrivTable::new();
        let id = SysId(2);
        table.alloc_static(id, 42).unwrap();
        assert!(!table.get(id).unwrap().is_empty());

        table.free(id);
        assert!(table.get(id).unwrap().is_empty());
        assert!(table.get(id).unwrap().s_ipc_to.is_empty());
    }

    #[test]
    fn test_free_and_realloc() {
        let mut table = PrivTable::new();
        // First dynamic allocation
        let dyn_id = table.alloc_dynamic(300).unwrap();
        table.free(dyn_id);
        // Should be allocatable again after free
        let dyn_id2 = table.alloc_dynamic(301).unwrap();
        assert_eq!(dyn_id, dyn_id2); // Should reuse same slot
    }

    #[test]
    fn test_compile_time_check() {
        assert!(NR_BOOT_PROCS <= NR_SYS_PROCS);
    }

    #[test]
    fn test_priv_flags() {
        let mut flags = PrivFlags::default();
        assert!(!flags.contains(PrivFlags::SYS_PROC));

        flags.insert(PrivFlags::SYS_PROC);
        assert!(flags.contains(PrivFlags::SYS_PROC));

        flags.remove(PrivFlags::SYS_PROC);
        assert!(!flags.contains(PrivFlags::SYS_PROC));
    }

    #[test]
    fn test_priv_is_sys_proc() {
        let mut p = Priv::empty(SysId(0));
        assert!(!p.is_sys_proc());

        p.s_flags.insert(PrivFlags::SYS_PROC);
        assert!(p.is_sys_proc());
    }
}

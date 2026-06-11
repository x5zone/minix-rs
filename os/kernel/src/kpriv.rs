use alloc::boxed::Box;
use alloc::vec::Vec;
use minix_types::Endpoint;

use crate::proc::{ProcNr, SigSet};

pub type PrivId = u16;
pub type SysId = u16;

pub const NR_IO_RANGE: usize = 64;
pub const NR_MEM_RANGE: usize = 20;
pub const NR_IRQ: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoRange {
    pub base: u32,
    pub limit: u32,
}

impl IoRange {
    pub const fn new() -> Self {
        Self { base: 0, limit: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemRange {
    pub base: u64,
    pub limit: u64,
}

impl MemRange {
    pub const fn new() -> Self {
        Self { base: 0, limit: 0 }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PrivFlagsBits: u16 {
        const PREEMPTIBLE = 0x002;
        const BILLABLE = 0x004;
        const DYN_PRIV_ID = 0x008;
        const SYS_PROC = 0x010;
        const CHECK_IO_PORT = 0x020;
        const CHECK_IRQ = 0x040;
        const CHECK_MEM = 0x080;
        const ROOT_SYS_PROC = 0x100;
        const VM_SYS_PROC = 0x200;
        const LU_SYS_PROC = 0x400;
        const RST_SYS_PROC = 0x800;
    }
}

/// Predefined privilege flag combinations for process types.
///
/// C: priv.h — `IDL_F`, `TSK_F`, `SRV_F`, etc.
pub mod priv_flag_set {
    use super::PrivFlagsBits as F;
    pub const IDL_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::BILLABLE.bits());
    pub const TSK_F: F = F::from_bits_truncate(F::SYS_PROC.bits());
    pub const SRV_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::PREEMPTIBLE.bits());
    pub const DSRV_F: F = F::from_bits_truncate(SRV_F.bits() | F::DYN_PRIV_ID.bits());
    pub const RSYS_F: F = F::from_bits_truncate(SRV_F.bits() | F::ROOT_SYS_PROC.bits());
    pub const VM_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::VM_SYS_PROC.bits());
    pub const USR_F: F = F::from_bits_truncate(F::BILLABLE.bits() | F::PREEMPTIBLE.bits());
}

pub const NR_TASKS: PrivId = 5;
pub const INIT_PROC_NR: PrivId = 11;
pub const USER_PRIV_ID: PrivId = NR_TASKS + INIT_PROC_NR;

pub(crate) struct KPriv {
    pub(crate) s_proc_nr: Option<ProcNr>,
    pub(crate) s_id: SysId,
    pub(crate) s_flags: PrivFlagsBits,
    pub(crate) s_init_flags: i32,
    pub(crate) s_asyntab: u64,
    pub(crate) s_asynsize: usize,
    pub(crate) s_asynendpoint: Endpoint,
    pub(crate) s_trap_mask: u16,
    pub(crate) s_ipc_to: u64,
    pub(crate) s_k_call_mask: [u32; 2],
    pub(crate) s_sig_mgr: Endpoint,
    pub(crate) s_bak_sig_mgr: Endpoint,
    pub(crate) s_notify_pending: u64,
    pub(crate) s_asyn_pending: u64,
    pub(crate) s_int_pending: u32,
    pub(crate) s_sig_pending: SigSet,
    pub(crate) s_ipcf: Option<usize>,
    pub(crate) s_alarm_timer: u64,
    pub(crate) s_stack_guard: Option<usize>,
    pub(crate) s_diag_sig: bool,
    pub(crate) s_nr_io_range: i32,
    pub(crate) s_io_tab: [IoRange; NR_IO_RANGE],
    pub(crate) s_nr_mem_range: i32,
    pub(crate) s_mem_tab: [MemRange; NR_MEM_RANGE],
    pub(crate) s_nr_irq: i32,
    pub(crate) s_irq_tab: [i32; NR_IRQ],
    pub(crate) s_grant_table: usize,
    pub(crate) s_grant_entries: i32,
    pub(crate) s_grant_endpoint: Endpoint,
    pub(crate) s_state_table: usize,
    pub(crate) s_state_entries: i32,
}

impl KPriv {
    pub fn new(id: SysId) -> Self {
        Self {
            s_proc_nr: None,
            s_id: id,
            s_flags: PrivFlagsBits::empty(),
            s_init_flags: 0,
            s_asyntab: 0,
            s_asynsize: 0,
            s_asynendpoint: Endpoint::NONE,
            s_trap_mask: 0,
            s_ipc_to: 0,
            s_k_call_mask: [0; 2],
            s_sig_mgr: Endpoint::NONE,
            s_bak_sig_mgr: Endpoint::NONE,
            s_notify_pending: 0,
            s_asyn_pending: 0,
            s_int_pending: 0,
            s_sig_pending: SigSet::empty(),
            s_ipcf: None,
            s_alarm_timer: 0,
            s_stack_guard: None,
            s_diag_sig: false,
            s_nr_io_range: 0,
            s_io_tab: [IoRange::new(); NR_IO_RANGE],
            s_nr_mem_range: 0,
            s_mem_tab: [MemRange::new(); NR_MEM_RANGE],
            s_nr_irq: 0,
            s_irq_tab: [0; NR_IRQ],
            s_grant_table: 0,
            s_grant_entries: 0,
            s_grant_endpoint: Endpoint::NONE,
            s_state_table: 0,
            s_state_entries: 0,
        }
    }

    pub fn is_sys_proc(&self) -> bool {
        self.s_flags.contains(PrivFlagsBits::SYS_PROC)
    }

    pub fn is_preemptible(&self) -> bool {
        self.s_flags.contains(PrivFlagsBits::PREEMPTIBLE)
    }

    pub fn is_billable(&self) -> bool {
        self.s_flags.contains(PrivFlagsBits::BILLABLE)
    }

    pub fn may_send_to(&self, target_id: SysId) -> bool {
        if target_id as usize >= 64 {
            return false;
        }
        (self.s_ipc_to & (1u64 << target_id)) != 0
    }
}

pub const NR_SYS_PROCS: usize = 64;

pub struct PrivTable {
    privs: Box<[KPriv]>,
}

impl PrivTable {
    pub fn new() -> Self {
        let privs: Vec<KPriv> = (0..NR_SYS_PROCS)
            .map(|i| KPriv::new(i as SysId))
            .collect();
        Self {
            privs: privs.into_boxed_slice(),
        }
    }

    pub fn get(&self, id: PrivId) -> Option<&KPriv> {
        let idx = id as usize;
        if idx < NR_SYS_PROCS {
            Some(&self.privs[idx])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: PrivId) -> Option<&mut KPriv> {
        let idx = id as usize;
        if idx < NR_SYS_PROCS {
            Some(&mut self.privs[idx])
        } else {
            None
        }
    }

    pub fn init(&mut self) {
        for (i, priv_) in self.privs.iter_mut().enumerate() {
            *priv_ = KPriv::new(i as SysId);
        }
    }

    /// Assign a static privilege to a boot process.
    ///
    /// Corresponds to Minix3's `get_priv(rp, static_priv_id(proc_nr))`.
    /// static_priv_id maps: `priv_id = NR_TASKS + proc_nr` (for proc_nr >= 0).
    /// Kernel tasks (proc_nr < 0) use the same formula since their priv_id
    /// is determined by the static slot layout: IDLE=0, CLOCK=1, SYSTEM=2, KERNEL=3.
    ///
    /// C: `get_priv()` — system.c:272-311, `static_priv_id()` — priv.h:12
    ///
    /// # Returns
    /// `Some(priv_id)` on success, `None` if priv_id is out of range or
    /// the slot is already occupied by another process.
    pub fn assign_static(&mut self, proc_nr: ProcNr) -> Option<PrivId> {
        // C: priv_id = static_priv_id(proc_nr) = NR_TASKS + proc_nr
        // For kernel tasks (proc_nr < 0), proc_nr maps to priv_id directly:
        //   IDLE=-4 → priv_id=0, CLOCK=-3 → priv_id=1, etc.
        // For user processes (proc_nr >= 0): priv_id = NR_TASKS + proc_nr
        let priv_id = if proc_nr < 0 {
            (NR_TASKS as ProcNr + proc_nr) as PrivId
        } else {
            (NR_TASKS + proc_nr as PrivId) as PrivId
        };

        let priv_ = self.get_mut(priv_id)?;

        // C: if(priv[priv_id].s_proc_nr != NONE) return EBUSY
        if priv_.s_proc_nr.is_some() {
            return None;
        }

        // C: rc->p_priv = sp; sp->s_proc_nr = proc_nr(rc)
        priv_.s_proc_nr = Some(proc_nr);

        Some(priv_id)
    }

    /// Set privilege flags, trap mask, IPC mask, kernel call mask, and
    /// scheduling parameters for a boot process. Corresponds to the per-type
    /// privilege setup in main.c:178-248.
    ///
    /// C: main.c:178-248 (sets s_flags, s_trap_mask, s_ipc_to, s_k_call_mask, priority, quantum)
    pub fn configure_boot_priv(
        &mut self,
        priv_id: PrivId,
        flags: PrivFlagsBits,
        init_flags: i32,
        trap_mask: u16,
        ipc_to: u64,
        k_call_mask: [u32; 2],
        sig_mgr: Endpoint,
    ) {
        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.s_flags = flags;
            priv_.s_init_flags = init_flags;
            priv_.s_trap_mask = trap_mask;
            priv_.s_ipc_to = ipc_to;
            priv_.s_k_call_mask = k_call_mask;
            priv_.s_sig_mgr = sig_mgr;
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
    fn test_kpriv_new() {
        let priv_ = KPriv::new(5);
        assert_eq!(priv_.s_id, 5);
        assert_eq!(priv_.s_proc_nr, None);
    }

    #[test]
    fn test_kpriv_is_sys_proc() {
        let mut priv_ = KPriv::new(0);
        assert!(!priv_.is_sys_proc());
        priv_.s_flags = PrivFlagsBits::SYS_PROC;
        assert!(priv_.is_sys_proc());
    }

    #[test]
    fn test_priv_table_new() {
        let table = PrivTable::new();
        assert!(table.get(0).is_some());
        assert!(table.get(63).is_some());
        assert!(table.get(64).is_none());
    }

    #[test]
    fn test_may_send_to() {
        let mut priv_ = KPriv::new(0);
        priv_.s_ipc_to = 1 << 5;
        assert!(priv_.may_send_to(5));
        assert!(!priv_.may_send_to(3));
    }
}

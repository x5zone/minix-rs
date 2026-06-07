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

pub mod priv_flags {
    pub const PREEMPTIBLE: u16 = 0x002;
    pub const BILLABLE: u16 = 0x004;
    pub const DYN_PRIV_ID: u16 = 0x008;
    pub const SYS_PROC: u16 = 0x010;
    pub const CHECK_IO_PORT: u16 = 0x020;
    pub const CHECK_IRQ: u16 = 0x040;
    pub const CHECK_MEM: u16 = 0x080;
    pub const ROOT_SYS_PROC: u16 = 0x100;
    pub const VM_SYS_PROC: u16 = 0x200;
    pub const LU_SYS_PROC: u16 = 0x400;
    pub const RST_SYS_PROC: u16 = 0x800;
}

pub const NR_TASKS: PrivId = 5;
pub const INIT_PROC_NR: PrivId = 11;
pub const USER_PRIV_ID: PrivId = NR_TASKS + INIT_PROC_NR;

pub mod priv_flag_set {
    pub const IDL_F: u16 = super::priv_flags::SYS_PROC | super::priv_flags::BILLABLE;
    pub const TSK_F: u16 = super::priv_flags::SYS_PROC;
    pub const SRV_F: u16 = super::priv_flags::SYS_PROC | super::priv_flags::PREEMPTIBLE;
    pub const DSRV_F: u16 = SRV_F | super::priv_flags::DYN_PRIV_ID;
    pub const RSYS_F: u16 = SRV_F | super::priv_flags::ROOT_SYS_PROC;
    pub const VM_F: u16 = super::priv_flags::SYS_PROC | super::priv_flags::VM_SYS_PROC;
    pub const USR_F: u16 = super::priv_flags::BILLABLE | super::priv_flags::PREEMPTIBLE;
}

pub struct KPriv {
    pub s_proc_nr: Option<ProcNr>,
    pub s_id: SysId,
    pub s_flags: u16,
    pub s_init_flags: i32,
    pub s_asyntab: u64,
    pub s_asynsize: usize,
    pub s_asynendpoint: Endpoint,
    pub s_trap_mask: u16,
    pub s_ipc_to: u64,
    pub s_k_call_mask: [u32; 2],
    pub s_sig_mgr: Endpoint,
    pub s_bak_sig_mgr: Endpoint,
    pub s_notify_pending: u64,
    pub s_asyn_pending: u64,
    pub s_int_pending: u32,
    pub s_sig_pending: SigSet,
    pub s_ipcf: Option<usize>,
    pub s_alarm_timer: u64,
    pub s_stack_guard: Option<usize>,
    pub s_diag_sig: bool,
    pub s_nr_io_range: i32,
    pub s_io_tab: [IoRange; NR_IO_RANGE],
    pub s_nr_mem_range: i32,
    pub s_mem_tab: [MemRange; NR_MEM_RANGE],
    pub s_nr_irq: i32,
    pub s_irq_tab: [i32; NR_IRQ],
    pub s_grant_table: usize,
    pub s_grant_entries: i32,
    pub s_grant_endpoint: Endpoint,
    pub s_state_table: usize,
    pub s_state_entries: i32,
}

impl KPriv {
    pub fn new(id: SysId) -> Self {
        Self {
            s_proc_nr: None,
            s_id: id,
            s_flags: 0,
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
        (self.s_flags & priv_flags::SYS_PROC) != 0
    }

    pub fn is_preemptible(&self) -> bool {
        (self.s_flags & priv_flags::PREEMPTIBLE) != 0
    }

    pub fn is_billable(&self) -> bool {
        (self.s_flags & priv_flags::BILLABLE) != 0
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
        priv_.s_flags = priv_flags::SYS_PROC;
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

use alloc::boxed::Box;
use alloc::vec::Vec;
use minix_types::Endpoint;

use crate::proc::{ProcNr, SigSet};
use crate::proc::NR_BOOT_PROCS;
use crate::proc_table::NR_TASKS;

pub type PrivId = u16;
pub type SysId = u16;

// C: minix/include/minix/config.h:52,55,58
pub const NR_IO_RANGE: usize = 64;
pub const NR_MEM_RANGE: usize = 20;
pub const NR_IRQ: usize = 16;

// C: minix/include/minix/com.h:272 — SYS_CALL_MASK_SIZE = BITMAP_CHUNKS(NR_SYS_CALLS) = 2
pub const SYS_CALL_MASK_SIZE: usize = 2;

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
    /// Privilege flags for `KPriv::s_flags`.
    ///
    /// C: minix/include/minix/const.h:143-154
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PrivFlagsBits: u16 {
        const PREEMPTIBLE     = 0x002;  // const.h:143
        const BILLABLE        = 0x004;  // const.h:144
        const DYN_PRIV_ID     = 0x008;  // const.h:145
        const SYS_PROC        = 0x010;  // const.h:147
        const CHECK_IO_PORT   = 0x020;  // const.h:148
        const CHECK_IRQ       = 0x040;  // const.h:149
        const CHECK_MEM       = 0x080;  // const.h:150
        const ROOT_SYS_PROC   = 0x100;  // const.h:151
        const VM_SYS_PROC     = 0x200;  // const.h:152
        const LU_SYS_PROC     = 0x400;  // const.h:153
        const RST_SYS_PROC    = 0x800;  // const.h:154
    }
}

/// Predefined privilege flag combinations for process types.
///
/// C: minix/include/minix/priv.h:36-49 — `IDL_F`, `TSK_F`, `SRV_F`, etc.
pub mod priv_flag_set {
    use super::PrivFlagsBits as F;
    /// C: priv.h:36 — IDL_F = SYS_PROC | BILLABLE (idle is not preemptible)
    pub const IDL_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::BILLABLE.bits());
    /// C: priv.h:44 — TSK_F = SYS_PROC (other kernel tasks)
    pub const TSK_F: F = F::from_bits_truncate(F::SYS_PROC.bits());
    /// C: priv.h:45 — SRV_F = SYS_PROC | PREEMPTIBLE (system services)
    pub const SRV_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::PREEMPTIBLE.bits());
    /// C: priv.h:46 — DSRV_F = SRV_F | DYN_PRIV_ID (dynamic system services)
    pub const DSRV_F: F = F::from_bits_truncate(SRV_F.bits() | F::DYN_PRIV_ID.bits());
    /// C: priv.h:47 — RSYS_F = SRV_F | ROOT_SYS_PROC (root system proc)
    pub const RSYS_F: F = F::from_bits_truncate(SRV_F.bits() | F::ROOT_SYS_PROC.bits());
    /// C: priv.h:48 — VM_F = SYS_PROC | VM_SYS_PROC (vm)
    pub const VM_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::VM_SYS_PROC.bits());
    /// C: priv.h:49 — USR_F = BILLABLE | PREEMPTIBLE (user processes)
    pub const USR_F: F = F::from_bits_truncate(F::BILLABLE.bits() | F::PREEMPTIBLE.bits());
}

/// C: minix/include/minix/priv.h:18 — USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR)
/// ROOT_USR_PROC_NR = INIT_PROC_NR = 11, so USER_PRIV_ID = NR_TASKS + 11 = 16
pub const INIT_PROC_NR: PrivId = 11;
pub const USER_PRIV_ID: PrivId = NR_TASKS as PrivId + INIT_PROC_NR;

/// C: minix/include/minix/priv.h:12 — static_priv_id(n) = NR_TASKS + n
#[inline]
pub fn static_priv_id(proc_nr: ProcNr) -> PrivId {
    (NR_TASKS as i32 + proc_nr) as PrivId
}

/// C: minix/include/minix/priv.h:11 — is_static_priv_id(id)
#[inline]
pub fn is_static_priv_id(id: PrivId) -> bool {
    let nr_static = NR_BOOT_PROCS as PrivId;
    id < nr_static
}

/// C: minix/include/minix/priv.h:14 — NULL_PRIV_ID = -1
pub const NULL_PRIV_ID: PrivId = u16::MAX;

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
    /// Synchronous alarm timer (C: `minix_timer_t s_alarm_timer` — priv.h:48).
    /// `None` = no alarm pending; `Some(entry)` = active alarm with expiration
    /// time and action. Replaces C's bare `u64` sentinel: `0` meant "no alarm"
    /// and any non-zero was a tick value (action lost). Using `Option<TimerEntry>`
    /// enforces "非法状态不可表达" (Ch3 §D7 — Doc 21).
    pub(crate) s_alarm_timer: Option<crate::clock::TimerEntry>,
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
            s_alarm_timer: None,
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

    pub(crate) fn get(&self, id: PrivId) -> Option<&KPriv> {
        let idx = id as usize;
        if idx < NR_SYS_PROCS {
            Some(&self.privs[idx])
        } else {
            None
        }
    }

    pub(crate) fn get_mut(&mut self, id: PrivId) -> Option<&mut KPriv> {
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
            (NR_TASKS as i32 + proc_nr) as PrivId
        } else {
            (NR_TASKS as PrivId + proc_nr as PrivId) as PrivId
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
        assert_eq!(priv_.s_flags, PrivFlagsBits::empty());
        assert_eq!(priv_.s_k_call_mask, [0u32; 2]);
    }

    #[test]
    fn test_kpriv_is_sys_proc() {
        let mut priv_ = KPriv::new(0);
        assert!(!priv_.is_sys_proc());
        priv_.s_flags = PrivFlagsBits::SYS_PROC;
        assert!(priv_.is_sys_proc());
    }

    #[test]
    fn test_kpriv_flag_predicates() {
        let mut priv_ = KPriv::new(0);
        assert!(!priv_.is_preemptible());
        assert!(!priv_.is_billable());

        priv_.s_flags = priv_flag_set::USR_F;
        assert!(priv_.is_preemptible());
        assert!(priv_.is_billable());
        assert!(!priv_.is_sys_proc()); // USR_F has no SYS_PROC
    }

    #[test]
    fn test_priv_flag_set_idl() {
        // IDL_F = SYS_PROC | BILLABLE, no PREEMPTIBLE
        let flags = priv_flag_set::IDL_F;
        assert!(flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(flags.contains(PrivFlagsBits::BILLABLE));
        assert!(!flags.contains(PrivFlagsBits::PREEMPTIBLE));
    }

    #[test]
    fn test_priv_flag_set_usr_no_sys_proc() {
        // USR_F = BILLABLE | PREEMPTIBLE — user processes are NOT system processes
        let flags = priv_flag_set::USR_F;
        assert!(!flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(flags.contains(PrivFlagsBits::BILLABLE));
        assert!(flags.contains(PrivFlagsBits::PREEMPTIBLE));
    }

    #[test]
    fn test_priv_flag_set_vm() {
        let flags = priv_flag_set::VM_F;
        assert!(flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(flags.contains(PrivFlagsBits::VM_SYS_PROC));
    }

    #[test]
    fn test_priv_table_new() {
        let table = PrivTable::new();
        assert!(table.get(0).is_some());
        assert!(table.get(63).is_some());
        assert!(table.get(64).is_none());
    }

    #[test]
    fn test_priv_table_assign_static() {
        let mut table = PrivTable::new();

        // Assign kernel task: IDLE = -4 → priv_id = NR_TASKS + (-4) = 1
        let id = table.assign_static(-4);
        assert!(id.is_some());
        let id = id.unwrap();
        // NR_TASKS=5, proc_nr=-4 → priv_id = 5 + (-4) = 1
        assert_eq!(id, 1);
        assert_eq!(table.get(id).unwrap().s_proc_nr, Some(-4));

        // Duplicate assignment fails
        let id2 = table.assign_static(-4);
        assert!(id2.is_none());
    }

    #[test]
    fn test_priv_table_assign_static_user_proc() {
        let mut table = PrivTable::new();

        // Assign user process: RS_PROC_NR = 1 → priv_id = NR_TASKS + 1 = 6
        let id = table.assign_static(1);
        assert!(id.is_some());
        let id = id.unwrap();
        assert_eq!(id, NR_TASKS as PrivId + 1);
    }

    #[test]
    fn test_priv_table_configure_boot_priv() {
        let mut table = PrivTable::new();
        let priv_id = table.assign_static(-4).unwrap();

        table.configure_boot_priv(
            priv_id,
            priv_flag_set::IDL_F,
            0,
            0,
            0,
            [0; 2],
            Endpoint::NONE,
        );

        let priv_ = table.get(priv_id).unwrap();
        assert!(priv_.s_flags.contains(PrivFlagsBits::SYS_PROC));
        assert!(priv_.s_flags.contains(PrivFlagsBits::BILLABLE));
    }

    #[test]
    fn test_may_send_to() {
        let mut priv_ = KPriv::new(0);
        priv_.s_ipc_to = 1 << 5;
        assert!(priv_.may_send_to(5));
        assert!(!priv_.may_send_to(3));
        assert!(!priv_.may_send_to(64)); // out of range
    }

    #[test]
    fn test_static_priv_id() {
        // C: static_priv_id(n) = NR_TASKS + n
        assert_eq!(static_priv_id(-4), 1); // IDLE: 5 + (-4) = 1
        assert_eq!(static_priv_id(0), NR_TASKS as PrivId); // DS: 5 + 0 = 5
        assert_eq!(static_priv_id(1), NR_TASKS as PrivId + 1); // RS: 5 + 1 = 6
    }

    #[test]
    fn test_is_static_priv_id() {
        assert!(is_static_priv_id(0));
        assert!(is_static_priv_id(NR_BOOT_PROCS as PrivId - 1));
        assert!(!is_static_priv_id(NR_BOOT_PROCS as PrivId));
    }

    #[test]
    fn test_user_priv_id() {
        // C: USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR) = NR_TASKS + 11 = 16
        assert_eq!(USER_PRIV_ID, NR_TASKS as PrivId + 11);
    }

    #[test]
    fn test_io_range_new() {
        let range = IoRange::new();
        assert_eq!(range.base, 0);
        assert_eq!(range.limit, 0);
    }

    #[test]
    fn test_mem_range_new() {
        let range = MemRange::new();
        assert_eq!(range.base, 0);
        assert_eq!(range.limit, 0);
    }

    #[test]
    fn test_kpriv_alarm_timer_default_none() {
        // Doc 21 §4.2 / Ch3 D7 promises `s_alarm_timer: Option<TimerEntry>`.
        // Default state must be `None` (no alarm pending), matching C's
        // `tmr_inittimer(&sp->s_alarm_timer)` at system.c:180.
        let p = KPriv::new(0);
        assert!(p.s_alarm_timer.is_none());
    }

    #[test]
    fn test_kpriv_alarm_timer_some_carries_action() {
        // Unlike C's bare `u64` (which lost TimerAction), `Option<TimerEntry>`
        // preserves both exp_time and action. This enforces "非法状态不可表达".
        use crate::clock::{TimerAction, TimerEntry};
        use minix_types::Endpoint;
        let mut p = KPriv::new(0);
        p.s_alarm_timer = Some(TimerEntry {
            exp_time: 1000,
            action: TimerAction::NotifyAlarm {
                endpoint: Endpoint::NONE,
            },
        });
        let entry = p.s_alarm_timer.as_ref().unwrap();
        assert_eq!(entry.exp_time, 1000);
        assert!(matches!(entry.action, TimerAction::NotifyAlarm { .. }));
    }
}

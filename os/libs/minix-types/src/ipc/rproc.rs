//! `rproc` wire face — byte-ABI pinning of C `struct rproc` (the RS-internal
//! service slot) for `do_getsysinfo`'s `SI_PROC_TAB`/`SI_PROCALL_TAB`
//! copy-out (request.c:1113-1121).
//!
//! C sources, in mirror order:
//! - `struct rproc` — `minix3/minix/servers/rs/type.h:56-108`;
//! - `struct rprocupd` — type.h:31-45 (the embedded `r_upd`);
//! - `struct rs_state_data` — rs.h:92-101;
//! - `ixfer_priv_s` (= `struct priv`) — `minix3/minix/kernel/priv.h:21-72`;
//! - `minix_timer_t` — `minix3/minix/include/minix/timers.h:32-38`;
//! - `sigset_t` — NetBSD `sys/sigtypes.h` (4 × `__uint32_t`, 16 bytes);
//! - `sys_map_t` — `minix3/minix/kernel/type.h:13` (`bitchunk_t
//!   chunk[BITMAP_CHUNKS(NR_SYS_PROCS)]`, NR_SYS_PROCS = 64 → 2 chunks);
//! - `irq_id_t` — kernel/type.h (`unsigned long`).
//!
//! Data model: **x86-64 LP64** (see `rs_start.rs`). Because this struct has
//! ~60 fields across four nesting levels, the offset constants are NOT
//! hand-computed: they are *derived from the `#[repr(C)]` witness* via
//! `offset_of!`, so the compiler's ABI layout is the single authority and a
//! misread C field order surfaces as a compile error in the landmark tests
//! (which pin a handful of independently hand-derived values).

/// Field offsets within `struct rproc`, derived from the witness layout.
pub mod rproc_off {
    use super::RprocLayout;
    use core::mem::offset_of;

    pub const R_PUB: usize = offset_of!(RprocLayout, r_pub);
    pub const R_OLD_RP: usize = offset_of!(RprocLayout, r_old_rp);
    pub const R_NEW_RP: usize = offset_of!(RprocLayout, r_new_rp);
    pub const R_PREV_RP: usize = offset_of!(RprocLayout, r_prev_rp);
    pub const R_NEXT_RP: usize = offset_of!(RprocLayout, r_next_rp);
    pub const R_UPD: usize = offset_of!(RprocLayout, r_upd);
    pub const R_PID: usize = offset_of!(RprocLayout, r_pid);
    pub const R_ASR_COUNT: usize = offset_of!(RprocLayout, r_asr_count);
    pub const R_RESTARTS: usize = offset_of!(RprocLayout, r_restarts);
    pub const R_BACKOFF: usize = offset_of!(RprocLayout, r_backoff);
    pub const R_FLAGS: usize = offset_of!(RprocLayout, r_flags);
    pub const R_INIT_ERR: usize = offset_of!(RprocLayout, r_init_err);
    pub const R_PERIOD: usize = offset_of!(RprocLayout, r_period);
    pub const R_CHECK_TM: usize = offset_of!(RprocLayout, r_check_tm);
    pub const R_ALIVE_TM: usize = offset_of!(RprocLayout, r_alive_tm);
    pub const R_STOP_TM: usize = offset_of!(RprocLayout, r_stop_tm);
    pub const R_CALLER: usize = offset_of!(RprocLayout, r_caller);
    pub const R_CALLER_REQUEST: usize = offset_of!(RprocLayout, r_caller_request);
    pub const R_CMD: usize = offset_of!(RprocLayout, r_cmd);
    pub const R_ARGS: usize = offset_of!(RprocLayout, r_args);
    pub const R_ARGV: usize = offset_of!(RprocLayout, r_argv);
    pub const R_ARGC: usize = offset_of!(RprocLayout, r_argc);
    pub const R_SCRIPT: usize = offset_of!(RprocLayout, r_script);
    pub const R_EXEC: usize = offset_of!(RprocLayout, r_exec);
    pub const R_EXEC_LEN: usize = offset_of!(RprocLayout, r_exec_len);
    pub const R_PRIV: usize = offset_of!(RprocLayout, r_priv);
    pub const R_UID: usize = offset_of!(RprocLayout, r_uid);
    pub const R_SCHEDULER: usize = offset_of!(RprocLayout, r_scheduler);
    pub const R_PRIORITY: usize = offset_of!(RprocLayout, r_priority);
    pub const R_QUANTUM: usize = offset_of!(RprocLayout, r_quantum);
    pub const R_CPU: usize = offset_of!(RprocLayout, r_cpu);
    pub const R_MAP_PREALLOC_ADDR: usize = offset_of!(RprocLayout, r_map_prealloc_addr);
    pub const R_MAP_PREALLOC_LEN: usize = offset_of!(RprocLayout, r_map_prealloc_len);
    pub const R_IO_TAB: usize = offset_of!(RprocLayout, r_io_tab);
    pub const R_NR_IO_RANGE: usize = offset_of!(RprocLayout, r_nr_io_range);
    pub const R_IRQ_TAB: usize = offset_of!(RprocLayout, r_irq_tab);
    pub const R_NR_IRQ: usize = offset_of!(RprocLayout, r_nr_irq);
    pub const R_IPC_LIST: usize = offset_of!(RprocLayout, r_ipc_list);
    pub const R_NR_CONTROL: usize = offset_of!(RprocLayout, r_nr_control);
    pub const R_CONTROL: usize = offset_of!(RprocLayout, r_control);
    /// `sizeof(struct rproc)`.
    pub const SIZE: usize = size_of::<RprocLayout>();

    /// `struct rprocupd` internals (relative to `R_UPD`).
    pub mod upd {
        use super::super::RprocUpdLayout;
        use core::mem::offset_of;
        pub const LU_FLAGS: usize = offset_of!(RprocUpdLayout, lu_flags);
        pub const INIT_FLAGS: usize = offset_of!(RprocUpdLayout, init_flags);
        pub const PREPARE_STATE: usize = offset_of!(RprocUpdLayout, prepare_state);
        pub const STATE_ENDPOINT: usize = offset_of!(RprocUpdLayout, state_endpoint);
        pub const PREPARE_TM: usize = offset_of!(RprocUpdLayout, prepare_tm);
        pub const PREPARE_MAXTIME: usize = offset_of!(RprocUpdLayout, prepare_maxtime);
        pub const RP: usize = offset_of!(RprocUpdLayout, rp);
        pub const PREPARE_STATE_DATA: usize = offset_of!(RprocUpdLayout, prepare_state_data);
        pub const PREPARE_STATE_DATA_GID: usize =
            offset_of!(RprocUpdLayout, prepare_state_data_gid);
        pub const PREV_RPUPD: usize = offset_of!(RprocUpdLayout, prev_rpupd);
        pub const NEXT_RPUPD: usize = offset_of!(RprocUpdLayout, next_rpupd);
        /// `sizeof(struct rprocupd)`.
        pub const SIZE: usize = size_of::<RprocUpdLayout>();
    }

    /// `struct priv` internals (relative to `R_PRIV`).
    pub mod priv_off {
        use super::super::PrivLayout;
        use core::mem::offset_of;
        pub const S_PROC_NR: usize = offset_of!(PrivLayout, s_proc_nr);
        pub const S_ID: usize = offset_of!(PrivLayout, s_id);
        pub const S_FLAGS: usize = offset_of!(PrivLayout, s_flags);
        pub const S_INIT_FLAGS: usize = offset_of!(PrivLayout, s_init_flags);
        pub const S_ASYNTAB: usize = offset_of!(PrivLayout, s_asyntab);
        pub const S_ASYNSIZE: usize = offset_of!(PrivLayout, s_asynsize);
        pub const S_ASYNENDPOINT: usize = offset_of!(PrivLayout, s_asynendpoint);
        pub const S_TRAP_MASK: usize = offset_of!(PrivLayout, s_trap_mask);
        pub const S_IPC_TO: usize = offset_of!(PrivLayout, s_ipc_to);
        pub const S_K_CALL_MASK: usize = offset_of!(PrivLayout, s_k_call_mask);
        pub const S_SIG_MGR: usize = offset_of!(PrivLayout, s_sig_mgr);
        pub const S_BAK_SIG_MGR: usize = offset_of!(PrivLayout, s_bak_sig_mgr);
        pub const S_NOTIFY_PENDING: usize = offset_of!(PrivLayout, s_notify_pending);
        pub const S_ASYN_PENDING: usize = offset_of!(PrivLayout, s_asyn_pending);
        pub const S_INT_PENDING: usize = offset_of!(PrivLayout, s_int_pending);
        pub const S_SIG_PENDING: usize = offset_of!(PrivLayout, s_sig_pending);
        pub const S_IPCF: usize = offset_of!(PrivLayout, s_ipcf);
        pub const S_ALARM_TIMER: usize = offset_of!(PrivLayout, s_alarm_timer);
        pub const S_STACK_GUARD: usize = offset_of!(PrivLayout, s_stack_guard);
        pub const S_DIAG_SIG: usize = offset_of!(PrivLayout, s_diag_sig);
        pub const S_NR_IO_RANGE: usize = offset_of!(PrivLayout, s_nr_io_range);
        pub const S_IO_TAB: usize = offset_of!(PrivLayout, s_io_tab);
        pub const S_NR_MEM_RANGE: usize = offset_of!(PrivLayout, s_nr_mem_range);
        pub const S_MEM_TAB: usize = offset_of!(PrivLayout, s_mem_tab);
        pub const S_NR_IRQ: usize = offset_of!(PrivLayout, s_nr_irq);
        pub const S_IRQ_TAB: usize = offset_of!(PrivLayout, s_irq_tab);
        pub const S_GRANT_TABLE: usize = offset_of!(PrivLayout, s_grant_table);
        pub const S_GRANT_ENTRIES: usize = offset_of!(PrivLayout, s_grant_entries);
        pub const S_GRANT_ENDPOINT: usize = offset_of!(PrivLayout, s_grant_endpoint);
        pub const S_STATE_TABLE: usize = offset_of!(PrivLayout, s_state_table);
        pub const S_STATE_ENTRIES: usize = offset_of!(PrivLayout, s_state_entries);
        /// `sizeof(struct priv)`.
        pub const SIZE: usize = size_of::<PrivLayout>();
    }
}

/// `ABI-shaped layout witness` — the C field order with C-equivalent types.
/// Never instantiated at runtime; exists so `offset_of!` can derive the true
/// GCC x86-64 layout and the offset constants can be checked against it.
#[repr(C)]
struct RprocLayout {
    r_pub: u64,
    r_old_rp: u64,
    r_new_rp: u64,
    r_prev_rp: u64,
    r_next_rp: u64,
    r_upd: RprocUpdLayout,
    r_pid: i32,
    r_asr_count: i32,
    r_restarts: i32,
    r_backoff: i64,
    r_flags: u32,
    r_init_err: i32,
    r_period: i64,
    r_check_tm: i64,
    r_alive_tm: i64,
    r_stop_tm: i64,
    r_caller: i32,
    r_caller_request: i32,
    r_cmd: [u8; 512],
    r_args: [u8; 512],
    r_argv: [u64; 12],
    r_argc: i32,
    r_script: [u8; 256],
    r_exec: u64,
    r_exec_len: u64,
    r_priv: PrivLayout,
    r_uid: u32,
    r_scheduler: i32,
    r_priority: i32,
    r_quantum: i32,
    r_cpu: i32,
    r_map_prealloc_addr: u64,
    r_map_prealloc_len: u64,
    r_io_tab: [RprocIoRange; 64],
    r_nr_io_range: i32,
    r_irq_tab: [i32; 16],
    r_nr_irq: i32,
    r_ipc_list: [u8; 256],
    r_nr_control: i32,
    r_control: [[u8; 16]; 8],
}

#[repr(C)]
struct RprocUpdLayout {
    lu_flags: i32,
    init_flags: i32,
    prepare_state: i32,
    state_endpoint: i32,
    prepare_tm: i64,
    prepare_maxtime: i64,
    rp: u64,
    prepare_state_data: RprocStateDataLayout,
    prepare_state_data_gid: i32,
    prev_rpupd: u64,
    next_rpupd: u64,
}

#[repr(C)]
struct RprocStateDataLayout {
    size: u64,
    ipcf_els: u64,
    ipcf_els_size: u64,
    ipcf_els_gid: i32,
    eval_addr: u64,
    eval_len: u64,
    eval_gid: i32,
}

#[repr(C)]
struct RprocIoRange {
    ior_base: u32,
    ior_limit: u32,
}

#[repr(C)]
struct PrivLayout {
    s_proc_nr: i32,
    s_id: i16,
    s_flags: i16,
    s_init_flags: i32,
    s_asyntab: u64,
    s_asynsize: u64,
    s_asynendpoint: i32,
    s_trap_mask: i16,
    s_ipc_to: [u32; 2],
    s_k_call_mask: [u32; 2],
    s_sig_mgr: i32,
    s_bak_sig_mgr: i32,
    s_notify_pending: [u32; 2],
    s_asyn_pending: [u32; 2],
    s_int_pending: u64,
    s_sig_pending: [u32; 4],
    s_ipcf: u64,
    s_alarm_timer: MinixTimerLayout,
    s_stack_guard: u64,
    s_diag_sig: u8,
    s_nr_io_range: i32,
    s_io_tab: [RprocIoRange; 64],
    s_nr_mem_range: i32,
    s_mem_tab: [RprocMemRange; 20],
    s_nr_irq: i32,
    s_irq_tab: [i32; 16],
    s_grant_table: u64,
    s_grant_entries: i32,
    s_grant_endpoint: i32,
    s_state_table: u64,
    s_state_entries: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct MinixTimerLayout {
    tmr_next: u64,
    tmr_exp_time: i64,
    tmr_func: u64,
    tmr_arg: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RprocMemRange {
    mr_base: u64,
    mr_limit: u64,
}

#[cfg(test)]
mod landmark_tests {
    use super::rproc_off::{self, priv_off, upd};

    /// Independently hand-derived layout landmarks (x86-64 LP64, walk of
    /// type.h:56-108 + priv.h:21-72). These catch a systematic misread of
    /// the C field order — the offset constants themselves are derived from
    /// the witness, so a witness misorder shows up as a landmark mismatch.
    #[test]
    fn test_rproc_layout_landmarks() {
        // struct priv (kernel/priv.h:21-72).
        assert_eq!(priv_off::S_PROC_NR, 0);
        assert_eq!(priv_off::S_ID, 4);
        assert_eq!(priv_off::S_FLAGS, 6);
        assert_eq!(priv_off::S_INIT_FLAGS, 8); // int aligns right after the two shorts (0/4/6/8)
        assert_eq!(priv_off::S_ASYNTAB, 16);
        assert_eq!(priv_off::S_TRAP_MASK, 36);
        assert_eq!(priv_off::S_IPC_TO, 40);
        assert_eq!(priv_off::S_K_CALL_MASK, 48);
        assert_eq!(priv_off::S_INT_PENDING, 80);
        assert_eq!(priv_off::S_SIG_PENDING, 88);
        assert_eq!(priv_off::S_ALARM_TIMER, 112);
        assert_eq!(priv_off::S_IO_TAB, 160);
        assert_eq!(priv_off::S_MEM_TAB, 680);
        assert_eq!(priv_off::S_GRANT_TABLE, 1072);
        assert_eq!(priv_off::SIZE, 1104);
        // struct rprocupd (type.h:31-45).
        assert_eq!(upd::SIZE, 120);
        assert_eq!(upd::RP, 32);
        assert_eq!(upd::PREPARE_STATE_DATA, 40);
        assert_eq!(upd::PREV_RPUPD, 104);
        // struct rproc (type.h:56-108).
        assert_eq!(rproc_off::R_UPD, 40);
        assert_eq!(rproc_off::R_PID, 160);
        assert_eq!(rproc_off::R_CMD, 232);
        assert_eq!(rproc_off::R_ARGV, 1256);
        assert_eq!(rproc_off::R_PRIV, 1632);
        assert_eq!(rproc_off::R_IO_TAB, 2776);
        assert_eq!(rproc_off::SIZE, 3752);
    }
}

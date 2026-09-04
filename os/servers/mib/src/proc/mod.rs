//! MIB process information: snapshots first, formats after.
//!
//! 16 owns the pulls (`tables`); 17/18/19 own the NetBSD formats;
//! 20 owns the MINIX/ProcFS door. Table *layouts* belong to
//! kernel/PM/VFS (A-6); only pull discipline travels here.
//!
//! 16-mib-proc-tables.md + companions.

pub mod lwp;
pub mod minix_proc;
pub mod proc2;
pub mod proc_args;
pub mod tables;

pub use minix_proc::{
    NameSource, ProcState, check_data_namelen, data_flags, is_task_pid, list_flags,
    list_row_included, mflags_for, name_source, resolve_task_slot,
};
pub use proc_args::{
    ArgsReq, PageHome, cap_fragment, cap_oldlen, check_args_namelen, check_args_slot, copy_budget,
    decode_req as decode_args_req, fragment_split, is_count_estimate, locate_page, max_estimate,
    walk_can_start,
};

pub use proc2::{
    Proc2Req, carry_sinter, check_pid_slot, check_proc2_args, copy_size, decode_req, eflag,
    groups_capped, headroom, match_kernel, match_row, map_stat, nice_output, nlwps, pflag,
    zombie_tty,
};

pub use lwp::{
    AwakeVerdict, BlockTarget, KernLwpVerdict, RtsCause, SleepVerdict, SleepWmesg, VfsBlock,
    VfsLane, WCHAN_CLASS_ENDPT, WCHAN_CLASS_MIB, WCHAN_CLASS_PM, WCHAN_CLASS_RTS, WCHAN_CLASS_TASK,
    WCHAN_CLASS_VFS, check_lwp_args, classify_awake, decode_blocked_on, extra_headroom,
    judge_kern_lwp, judge_pid_slot, judge_sleep, judge_times, pick_rts, user_flag,
    wchan_with_class,
};

pub use tables::{
    EXTRA_PROCS, EndptLane, MP_MAGIC, NO_SLOT, PMAGIC, PULL_ORDER, PullSource, PullVerdict,
    chain_lookup, hash_slot, hash_slots, judge_pull, magic_ok, paren_direct, ticks_to_timeval,
    wmesg_lane,
};

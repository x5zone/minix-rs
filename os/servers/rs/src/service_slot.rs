//! Service slot data structures — the system-service registration table rows.
//!
//! Mirrors `minix3/minix/servers/rs/type.h` (`struct rproc`), `include/minix/rs.h`
//! (`struct rprocpub` + `SF_*`), and `servers/rs/const.h` (`r_flags` +
//! `RS_SRV_IS_IDLE`). The two C tables (`rproc[]`/`rprocpub[]`, glo.h:33-34)
//! are merged into one row per slot: [`ServiceSlot`] embeds [`PublicSlot`] as
//! `pub_`, replacing the `r_pub` self-referential pointer (type.h:57) with a
//! plain field access (ARCH A-3 — see 02-rs-process-table.md §3.1).
//!
//! Full field table + mechanism ownership: 02-rs-process-table.md §2.3/§2.4.

use crate::privilege::Privilege;
use alloc::sync::Arc;
use core::fmt;
use minix_types::{Clock, Endpoint, Pid};

// ── Size constants (C: servers/rs/const.h + include/minix/rs.h) ────────────

/// Maximum label length. C: `RS_MAX_LABEL_LEN` — rs.h:58.
pub const RS_MAX_LABEL_LEN: usize = 16;
/// Space reserved for program and arguments. C: `MAX_COMMAND_LEN` — const.h:18.
pub const MAX_COMMAND_LEN: usize = 512;
/// Maximum restart script name length. C: `MAX_SCRIPT_LEN` — const.h:19.
pub const MAX_SCRIPT_LEN: usize = 256;
/// Maximum number of arguments. C: `MAX_NR_ARGS` — const.h:20.
pub const MAX_NR_ARGS: usize = 10;
/// Max size of the IPC target-process-name list. C: `MAX_IPC_LIST` — const.h:22.
pub const MAX_IPC_LIST: usize = 256;
/// Number of control entries. C: `RS_NR_CONTROL` — rs.h:55.
pub const RS_NR_CONTROL: usize = 8;
/// Allowed I/O port ranges. C: `NR_IO_RANGE` — config.h:52.
pub const NR_IO_RANGE: usize = 64;
/// Allowed memory ranges. C: `NR_MEM_RANGE` — config.h:55.
pub const NR_MEM_RANGE: usize = 20;
/// Allowed IRQ lines. C: `NR_IRQ` — config.h:58.
pub const NR_IRQ: usize = 16;
/// Number of socket driver domains. C: `NR_DOMAIN` — config.h:61.
pub const NR_DOMAIN: usize = 8;
/// `r_argv` capacity: path + args + null. C: `ARGV_ELEMENTS` — type.h:80.
pub const ARGV_ELEMENTS: usize = MAX_NR_ARGS + 2;

// ── r_flags (C: const.h:28-43) ──────────────────────────────────────────────

bitflags::bitflags! {
    /// Runtime status and policy flags of a service slot.
    ///
    /// C: `unsigned r_flags` + the 16 `RS_*` macros — `minix3/minix/servers/rs/const.h:28-43`.
    /// Bits are orthogonal (a service can be `IN_USE|ACTIVE|INITIALIZING` at
    /// once), so a bitset — not an enum — mirrors the C semantics exactly.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RFlags: u16 {
        /// Set when the process slot is in use. C: `RS_IN_USE` — const.h:28.
        const IN_USE        = 0x001;
        /// Set when exit is expected. C: `RS_EXITING` — const.h:29.
        const EXITING       = 0x002;
        /// Set when refresh must be done. C: `RS_REFRESHING` — const.h:30.
        const REFRESHING    = 0x004;
        /// Service failed to reply to a ping request. C: `RS_NOPINGREPLY` — const.h:31.
        const NOPINGREPLY   = 0x008;
        /// Service has terminated. C: `RS_TERMINATED` — const.h:32.
        const TERMINATED    = 0x010;
        /// No reply sent to the RS_DOWN caller yet. C: `RS_LATEREPLY` — const.h:33.
        const LATEREPLY     = 0x020;
        /// Set when init is in progress. C: `RS_INITIALIZING` — const.h:34.
        const INITIALIZING  = 0x040;
        /// Set when update is in progress. C: `RS_UPDATING` — const.h:35.
        const UPDATING      = 0x080;
        /// Set when updating and preparation is done. C: `RS_PREPARE_DONE` — const.h:36.
        const PREPARE_DONE  = 0x100;
        /// Set when updating and init is done. C: `RS_INIT_DONE` — const.h:37.
        const INIT_DONE     = 0x200;
        /// Set when updating and init is pending. C: `RS_INIT_PENDING` — const.h:38.
        const INIT_PENDING  = 0x400;
        /// Set for the active instance of a service. C: `RS_ACTIVE` — const.h:39.
        const ACTIVE        = 0x800;
        /// Set for an instance ready to be cleaned up. C: `RS_DEAD` — const.h:40.
        const DEAD          = 0x1000;
        /// Detach at cleanup time. C: `RS_CLEANUP_DETACH` — const.h:41.
        const CLEANUP_DETACH = 0x2000;
        /// Run script at cleanup time. C: `RS_CLEANUP_SCRIPT` — const.h:42.
        const CLEANUP_SCRIPT = 0x4000;
        /// After exit, restart with a new endpoint. C: `RS_REINCARNATE` — const.h:43.
        const REINCARNATE   = 0x8000;
    }
}

impl RFlags {
    /// Whether the slot is idle (reclaimable / no pending work).
    ///
    /// C: `RS_SRV_IS_IDLE(S)` — const.h:45:
    /// `DEAD` set, or no bits outside `IN_USE|ACTIVE|CLEANUP_DETACH|CLEANUP_SCRIPT`.
    pub fn is_idle(self) -> bool {
        self.contains(Self::DEAD)
            || (self & !(Self::IN_USE | Self::ACTIVE | Self::CLEANUP_DETACH | Self::CLEANUP_SCRIPT))
                .is_empty()
    }
}

// ── sys_flags (C: rs.h:191-206) ─────────────────────────────────────────────

bitflags::bitflags! {
    /// Service capability/policy flags (the "public" half of the slot).
    ///
    /// C: `unsigned sys_flags` + the 13 `SF_*` macros —
    /// `minix3/minix/include/minix/rs.h:191-203`. Set from the boot sys table
    /// or the `RS_UP` request; mostly immutable during the service lifetime.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SysFlags: u16 {
        /// Set for core system services. C: `SF_CORE_SRV` — rs.h:191.
        const CORE_SRV    = 0x001;
        /// Set when the process needs synch boot init. C: `SF_SYNCH_BOOT` — rs.h:192.
        const SYNCH_BOOT  = 0x002;
        /// Set when the process needs copy to start. C: `SF_NEED_COPY` — rs.h:193.
        const NEED_COPY   = 0x004;
        /// Set when the process has a copy in memory. C: `SF_USE_COPY` — rs.h:194.
        const USE_COPY    = 0x008;
        /// Set when the process needs a replica to start. C: `SF_NEED_REPL` — rs.h:195.
        const NEED_REPL   = 0x010;
        /// Set when the process has a replica. C: `SF_USE_REPL` — rs.h:196.
        const USE_REPL    = 0x020;
        /// Set when the process needs vm update. C: `SF_VM_UPDATE` — rs.h:197.
        const VM_UPDATE   = 0x040;
        /// Set when vm update is a rollback. C: `SF_VM_ROLLBACK` — rs.h:198.
        const VM_ROLLBACK = 0x080;
        /// Set when vm update ignores mmapped regions. C: `SF_VM_NOMMAP` — rs.h:199.
        const VM_NOMMAP   = 0x100;
        /// Set when the process has a restart script. C: `SF_USE_SCRIPT` — rs.h:200.
        const USE_SCRIPT  = 0x200;
        /// Set when the process detaches on restart. C: `SF_DET_RESTART` — rs.h:201.
        const DET_RESTART = 0x400;
        /// Set when the process should not be restarted. C: `SF_NORESTART` — rs.h:202.
        const NORESTART   = 0x800;
        /// Set when we should ignore binary exp. offset. C: `SF_NO_BIN_EXP` — rs.h:203.
        const NO_BIN_EXP  = 0x1000;
    }
}

/// C: `SRV_SF` — const.h:65 (`SF_CORE_SRV`).
pub const SRV_SF: SysFlags = SysFlags::CORE_SRV;
/// C: `SRVR_SF` — const.h:66 (`SRV_SF | SF_NEED_REPL`).
pub const SRVR_SF: SysFlags = SysFlags::CORE_SRV.union(SysFlags::NEED_REPL);
/// C: `DSRV_SF` — const.h:67 (dynamic system services, no flags).
pub const DSRV_SF: SysFlags = SysFlags::empty();
/// C: `VM_SF` — const.h:68 (`SRVR_SF`, vm).
pub const VM_SF: SysFlags = SRVR_SF;
/// Immutable sys flags inherited across replica/update instances.
///
/// C: `IMM_SF` — rs.h:205-206. `inherit_service_defaults` (08) refuses to
/// change these bits on a new instance.
pub const IMM_SF: SysFlags = SysFlags::NO_BIN_EXP
    .union(SysFlags::CORE_SRV)
    .union(SysFlags::SYNCH_BOOT)
    .union(SysFlags::NEED_COPY)
    .union(SysFlags::NEED_REPL);

// ── SlotId ──────────────────────────────────────────────────────────────────

/// Index of a service slot in the process table (0..NR_SYS_PROCS).
///
/// C: the `slot_nr` loop variable / `rp - rproc` offset (manager.c:1943,
/// main.c:255). Replaces raw `struct rproc *` handles (ARCH A-3): the table
/// owns the rows, other code holds indices, so a stale handle cannot dangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(pub usize);

impl SlotId {
    /// Creates a slot id. Callers must keep `index < NR_SYS_PROCS`.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Raw index (the C `slot_nr`).
    pub const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for SlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "slot {}", self.0)
    }
}

// ── Label ───────────────────────────────────────────────────────────────────

/// A 16-byte service label / process name.
///
/// C: `char label[RS_MAX_LABEL_LEN]` — rs.h:176-177, compared with `strcmp`
/// and copied with `strlcpy`. Fixed-size (no heap) to keep the C memory
/// layout of the public table, which is shared via a grant
/// (`rinit.rproctab_gid`, main.c:185).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label([u8; RS_MAX_LABEL_LEN]);

impl Label {
    /// Empty label (all zeros).
    pub const fn empty() -> Self {
        Self([0; RS_MAX_LABEL_LEN])
    }

    /// Copies `bytes` truncated to 16 bytes (C `strlcpy` semantics, rs.h:58).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut label = [0u8; RS_MAX_LABEL_LEN];
        let n = bytes.len().min(RS_MAX_LABEL_LEN);
        label[..n].copy_from_slice(&bytes[..n]);
        Self(label)
    }

    /// Raw bytes (including the trailing NUL padding).
    pub fn as_bytes(&self) -> &[u8; RS_MAX_LABEL_LEN] {
        &self.0
    }

    /// NUL-terminated string view; `None` if the buffer is not NUL-terminated
    /// or not valid UTF-8 (fail-closed, no unsafe `str` construction).
    pub fn as_str(&self) -> Option<&str> {
        let len = self
            .0
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(RS_MAX_LABEL_LEN);
        core::str::from_utf8(&self.0[..len]).ok()
    }
}

impl PartialEq<&str> for Label {
    /// C: `strcmp(rpub->label, label) == 0` — manager.c:1948.
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

// ── IO range (C: minix3/minix/include/minix/type.h:133-137) ───────────────────────────────────────

/// Allowed I/O port range, backed up from the privilege structure.
///
/// C: `struct io_range` — `minix3/minix/include/minix/type.h:133-137`; `r_io_tab` —
/// type.h:100. Populated by 03-rs-privilege.md; the backup lets `edit_slot`
/// (08) rebuild `r_priv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IoRange {
    /// First port of the range. C: `ior_base`.
    pub base: u32,
    /// Length of the range. C: `ior_limit`.
    pub len: u32,
}

// ── PublicSlot (C: rs.h:165-183) ────────────────────────────────────────────

/// The public half of a service slot (`rprocpub`), merged into `ServiceSlot`.
///
/// C: `struct rprocpub` — `minix3/minix/include/minix/rs.h:165-183`.
/// Fields a service exposes to other services (via the `rproctab_gid` grant).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicSlot {
    /// Set when the entry is in use. C: `in_use` — rs.h:166.
    pub in_use: bool,
    /// Sys flags. C: `sys_flags` — rs.h:167.
    pub sys_flags: SysFlags,
    /// Process endpoint number. C: `endpoint` — rs.h:168.
    pub endpoint: Endpoint,
    /// Old instance endpoint (VM, when updating). C: `old_endpoint` — rs.h:169.
    pub old_endpoint: Option<Endpoint>,
    /// New instance endpoint (VM, when updating). C: `new_endpoint` — rs.h:170.
    pub new_endpoint: Option<Endpoint>,
    /// Major device number or NO_DEV. C: `dev_nr` — rs.h:172.
    pub dev_nr: u32,
    /// Number of socket driver domains. C: `nr_domain` — rs.h:173.
    pub nr_domain: u8,
    /// Set of socket driver domains. C: `domain[NR_DOMAIN]` — rs.h:174.
    pub domain: [i32; NR_DOMAIN],
    /// Label of this service. C: `label` — rs.h:176.
    pub label: Label,
    /// Process name of this service. C: `proc_name` — rs.h:177.
    pub proc_name: Label,
    /// VM call mask. C: `bitchunk_t vm_call_mask[VM_CALL_MASK_SIZE]` — rs.h:179
    /// (filled at boot Step 1 — main.c:317; semantics 03/05).
    pub vm_call_mask: crate::privilege::CallMask,
    /// Device manager id. C: `devman_id` — rs.h:182 (populated by 11).
    pub devman_id: Option<i32>,
}

impl PublicSlot {
    /// Vacant public half (C: `memset` + `in_use = FALSE` — main.c:234).
    pub fn vacant() -> Self {
        Self {
            in_use: false,
            sys_flags: SysFlags::empty(),
            endpoint: Endpoint::NONE,
            old_endpoint: None,
            new_endpoint: None,
            dev_nr: 0,
            nr_domain: 0,
            domain: [0; NR_DOMAIN],
            label: Label::empty(),
            proc_name: Label::empty(),
            vm_call_mask: crate::privilege::CallMask::empty(),
            devman_id: None,
        }
    }
}

// ── ServiceSlot (C: type.h:56-108) ──────────────────────────────────────────

/// One row of the system-service registration table.
///
/// C: `struct rproc` — `minix3/minix/servers/rs/type.h:56-108`. The `r_pub`
/// pointer (type.h:57) is replaced by the embedded [`PublicSlot`] `pub_`
/// (ARCH A-3, 02-rs-process-table.md §3.1). Field groups and mechanism
/// ownership: 02-rs-process-table.md §2.3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSlot {
    /// Public half (`r_pub`). C: type.h:57 + rs.h:165-183.
    pub pub_: PublicSlot,
    /// Old-version slot. C: `r_old_rp` — type.h:58 (ARCH A-3).
    pub old_rp: Option<SlotId>,
    /// New-version slot. C: `r_new_rp` — type.h:59 (ARCH A-3).
    pub new_rp: Option<SlotId>,
    /// Previous replica slot. C: `r_prev_rp` — type.h:60 (ARCH A-3).
    pub prev_rp: Option<SlotId>,
    /// Next replica slot. C: `r_next_rp` — type.h:61 (ARCH A-3).
    pub next_rp: Option<SlotId>,

    /// Process id; `None` = no process (-1). C: `r_pid` — type.h:63.
    pub pid: Option<Pid>,
    /// Number of live updates with ASR. C: `r_asr_count` — type.h:65.
    pub asr_count: i32,
    /// Number of restarts (initially zero). C: `r_restarts` — type.h:66.
    pub restarts: i32,
    /// Periods to wait before revive. C: `r_backoff` — type.h:67.
    pub backoff: i64,
    /// Status and policy flags. C: `r_flags` — type.h:68 (const.h:28-43).
    pub flags: RFlags,
    /// Error code at initialization time. C: `r_init_err` — type.h:69.
    pub init_err: i32,

    /// Heartbeat period (or zero). C: `r_period` — type.h:71 (07).
    pub period: i64,
    /// Timestamp of last check. C: `r_check_tm` — type.h:72 (07).
    pub check_tm: Clock,
    /// Timestamp of last heartbeat. C: `r_alive_tm` — type.h:73 (07).
    pub alive_tm: Clock,
    /// Timestamp of SIGTERM signal. C: `r_stop_tm` — type.h:74 (07).
    pub stop_tm: Clock,

    /// RS_LATEREPLY caller. C: `r_caller` — type.h:75 (06).
    pub caller: Endpoint,
    /// RS_LATEREPLY caller request. C: `r_caller_request` — type.h:76 (06).
    pub caller_request: i32,

    /// Raw command plus arguments. C: `r_cmd` — type.h:78 (08).
    pub cmd: [u8; MAX_COMMAND_LEN],
    /// Null-separated raw command plus arguments. C: `r_args` — type.h:79 (08).
    pub args: [u8; MAX_COMMAND_LEN],
    /// Number of arguments. C: `r_argc` — type.h:82 (08).
    pub argc: i32,
    /// Name of the restart script executable. C: `r_script` — type.h:83 (15).
    pub script: [u8; MAX_SCRIPT_LEN],

    /// Executable image. C: `r_exec`/`r_exec_len` — type.h:85-86 (09, ARCH A-5).
    ///
    /// `Arc<[u8]>` replaces C's raw shared pointer: `share_exec` (manager.c:1357)
    /// clones the `Arc`, `free_exec` uses `Arc::ptr_eq`/`strong_count` instead of
    /// the C full-table pointer scan (09-rs-exec.md §3.1).
    pub exec: Option<Arc<[u8]>>,

    /// Privilege structure to be passed to the kernel. C: `ixfer_priv_s r_priv`
    /// — type.h:88-90 (03-rs-privilege.md; value-embedded, not a pointer).
    pub priv_: Privilege,

    /// User id. C: `r_uid` — type.h:91 (03).
    pub uid: u32,
    /// Scheduler endpoint. C: `r_scheduler` — type.h:92 (03).
    pub scheduler: Endpoint,
    /// Priority (negatives reserved for special meanings). C: `r_priority` — type.h:93 (03).
    pub priority: i32,
    /// Quantum. C: `r_quantum` — type.h:94 (03).
    pub quantum: i32,
    /// CPU affinity. C: `r_cpu` — type.h:95 (03/10).
    pub cpu: i32,

    /// Preallocated mmap address. C: `r_map_prealloc_addr` — type.h:96 (16).
    pub map_prealloc_addr: u64,
    /// Preallocated mmap length. C: `r_map_prealloc_len` — type.h:97 (16).
    pub map_prealloc_len: usize,

    /// Allowed I/O port ranges (priv backup). C: `r_io_tab` — type.h:100 (03).
    pub io_tab: [IoRange; NR_IO_RANGE],
    /// Number of I/O ranges. C: `r_nr_io_range` — type.h:101 (03).
    pub nr_io_range: i32,
    /// Allowed IRQ lines (priv backup). C: `r_irq_tab` — type.h:102 (03).
    pub irq_tab: [i32; NR_IRQ],
    /// Number of IRQ lines. C: `r_nr_irq` — type.h:103 (03).
    pub nr_irq: i32,

    /// IPC target-process-name list. C: `r_ipc_list` — type.h:105 (05).
    pub ipc_list: [u8; MAX_IPC_LIST],
    /// Number of control entries. C: `r_nr_control` — type.h:106 (08).
    pub nr_control: i32,
    /// Control labels. C: `r_control` — type.h:107 (08).
    pub control: [Label; RS_NR_CONTROL],
}

impl ServiceSlot {
    /// A vacant slot — C's "zeroed table row" (main.c:230-237) + `r_pid=-1`.
    ///
    /// C keeps stale data in freed rows and relies on `r_flags == 0` /
    /// `in_use == FALSE`; Rust makes vacancy explicit by zeroing every field
    /// (02-rs-process-table.md §3.6).
    pub fn vacant() -> Self {
        Self {
            pub_: PublicSlot::vacant(),
            old_rp: None,
            new_rp: None,
            prev_rp: None,
            next_rp: None,
            pid: None,
            asr_count: 0,
            restarts: 0,
            backoff: 0,
            flags: RFlags::empty(),
            init_err: 0,
            period: 0,
            check_tm: 0,
            alive_tm: 0,
            stop_tm: 0,
            caller: Endpoint::NONE,
            caller_request: 0,
            cmd: [0; MAX_COMMAND_LEN],
            args: [0; MAX_COMMAND_LEN],
            argc: 0,
            script: [0; MAX_SCRIPT_LEN],
            exec: None,
            priv_: Privilege::vacant(),
            uid: 0,
            scheduler: Endpoint::NONE,
            priority: 0,
            quantum: 0,
            cpu: 0,
            map_prealloc_addr: 0,
            map_prealloc_len: 0,
            io_tab: [IoRange::default(); NR_IO_RANGE],
            nr_io_range: 0,
            irq_tab: [0; NR_IRQ],
            nr_irq: 0,
            ipc_list: [0; MAX_IPC_LIST],
            nr_control: 0,
            control: [Label::empty(); RS_NR_CONTROL],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rflags_bits_match_const_h() {
        // C: const.h:28-43 — every bit value is asserted against the header.
        assert_eq!(RFlags::IN_USE.bits(), 0x001);
        assert_eq!(RFlags::EXITING.bits(), 0x002);
        assert_eq!(RFlags::REFRESHING.bits(), 0x004);
        assert_eq!(RFlags::NOPINGREPLY.bits(), 0x008);
        assert_eq!(RFlags::TERMINATED.bits(), 0x010);
        assert_eq!(RFlags::LATEREPLY.bits(), 0x020);
        assert_eq!(RFlags::INITIALIZING.bits(), 0x040);
        assert_eq!(RFlags::UPDATING.bits(), 0x080);
        assert_eq!(RFlags::PREPARE_DONE.bits(), 0x100);
        assert_eq!(RFlags::INIT_DONE.bits(), 0x200);
        assert_eq!(RFlags::INIT_PENDING.bits(), 0x400);
        assert_eq!(RFlags::ACTIVE.bits(), 0x800);
        assert_eq!(RFlags::DEAD.bits(), 0x1000);
        assert_eq!(RFlags::CLEANUP_DETACH.bits(), 0x2000);
        assert_eq!(RFlags::CLEANUP_SCRIPT.bits(), 0x4000);
        assert_eq!(RFlags::REINCARNATE.bits(), 0x8000);
    }

    #[test]
    fn test_sysflags_bits_match_rs_h() {
        // C: rs.h:191-203.
        assert_eq!(SysFlags::CORE_SRV.bits(), 0x001);
        assert_eq!(SysFlags::SYNCH_BOOT.bits(), 0x002);
        assert_eq!(SysFlags::NEED_COPY.bits(), 0x004);
        assert_eq!(SysFlags::USE_COPY.bits(), 0x008);
        assert_eq!(SysFlags::NEED_REPL.bits(), 0x010);
        assert_eq!(SysFlags::USE_REPL.bits(), 0x020);
        assert_eq!(SysFlags::VM_UPDATE.bits(), 0x040);
        assert_eq!(SysFlags::VM_ROLLBACK.bits(), 0x080);
        assert_eq!(SysFlags::VM_NOMMAP.bits(), 0x100);
        assert_eq!(SysFlags::USE_SCRIPT.bits(), 0x200);
        assert_eq!(SysFlags::DET_RESTART.bits(), 0x400);
        assert_eq!(SysFlags::NORESTART.bits(), 0x800);
        assert_eq!(SysFlags::NO_BIN_EXP.bits(), 0x1000);
    }

    #[test]
    fn test_preset_combinations_match_c() {
        // C: const.h:65-68 + rs.h:205-206.
        assert_eq!(SRV_SF, SysFlags::CORE_SRV);
        assert_eq!(SRVR_SF, SysFlags::CORE_SRV | SysFlags::NEED_REPL);
        assert_eq!(DSRV_SF, SysFlags::empty());
        assert_eq!(VM_SF, SRVR_SF);
        assert_eq!(
            IMM_SF,
            SysFlags::NO_BIN_EXP
                | SysFlags::CORE_SRV
                | SysFlags::SYNCH_BOOT
                | SysFlags::NEED_COPY
                | SysFlags::NEED_REPL
        );
    }

    #[test]
    fn test_is_idle_combinations() {
        // C: RS_SRV_IS_IDLE — const.h:45 truth table.
        assert!(RFlags::DEAD.is_idle(), "DEAD set → idle");
        assert!(
            (RFlags::IN_USE | RFlags::ACTIVE).is_idle(),
            "IN_USE|ACTIVE with no other bits → idle"
        );
        assert!(
            (RFlags::IN_USE | RFlags::ACTIVE | RFlags::CLEANUP_DETACH).is_idle(),
            "CLEANUP_* bits are exempted"
        );
        assert!(
            !(RFlags::IN_USE | RFlags::ACTIVE | RFlags::EXITING).is_idle(),
            "EXITING → not idle"
        );
        assert!(
            !(RFlags::IN_USE | RFlags::ACTIVE | RFlags::INITIALIZING).is_idle(),
            "INITIALIZING → not idle"
        );
        assert!(RFlags::empty().is_idle(), "empty flags → idle");
        assert!(
            !(RFlags::IN_USE | RFlags::ACTIVE | RFlags::LATEREPLY).is_idle(),
            "LATEREPLY → not idle"
        );
    }

    #[test]
    fn test_label_from_bytes_truncates() {
        let long = *b"this-label-is-way-too-long-for-16";
        let label = Label::from_bytes(&long);
        assert_eq!(label.as_str(), Some("this-label-is-wa"));
        assert_eq!(label.as_bytes().len(), RS_MAX_LABEL_LEN);
    }

    #[test]
    fn test_label_as_str_nul_terminated() {
        // NUL-terminated view stops at the first NUL.
        let mut raw = [0u8; RS_MAX_LABEL_LEN];
        raw[..3].copy_from_slice(b"rs\0");
        let label = Label::from_bytes(&raw);
        assert_eq!(label.as_str(), Some("rs"));

        // Invalid UTF-8 → None (fail-closed).
        let mut bad = [0xFFu8; RS_MAX_LABEL_LEN];
        bad[0] = 0xFF;
        assert_eq!(Label::from_bytes(&bad).as_str(), None);
    }

    #[test]
    fn test_label_eq_str() {
        // C: strcmp — manager.c:1948.
        let label = Label::from_bytes(b"vfs");
        assert!(label == "vfs");
        assert!(label != "pm");
        // An all-zero buffer is NUL-terminated at index 0 → the empty string
        // (C: strcmp(empty_label, "") == 0).
        assert!(Label::empty() == "");
    }

    #[test]
    fn test_vacant_slot_is_clean() {
        let slot = ServiceSlot::vacant();
        assert!(slot.flags.is_empty());
        assert!(!slot.pub_.in_use);
        assert_eq!(slot.pub_.endpoint, Endpoint::NONE);
        assert_eq!(slot.pid, None);
        assert_eq!(slot.old_rp, None);
        assert_eq!(slot.cmd, [0u8; MAX_COMMAND_LEN]);
        assert_eq!(slot.control, [Label::empty(); RS_NR_CONTROL]);
    }
}

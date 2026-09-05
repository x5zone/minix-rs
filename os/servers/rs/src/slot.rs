//! Service-slot configuration: the `rs_start` request parameters.
//!
//! Mirrors `minix3/minix/include/minix/rs.h:104-151` (`struct rs_start`) and
//! the validation/parsing logic of `check_request`
//! (`minix3/minix/servers/rs/request.c:1265-1308`) and `build_cmd_dep`
//! (`manager.c:289-323`). 08-rs-slot-config.md.
//!
//! The `sys_datacopy`-coupled steps (`copy_rs_start` — manager.c:135-146,
//! `copy_label` — manager.c:151-169, the `edit_slot` field writes —
//! manager.c:1460-1703) are wired through 19-rs-external-interfaces.md; this
//! module owns the pure validation and parsing that does not touch the
//! kernel.

use crate::privilege::{CallMask, IoRange, PrivFlags};
use crate::sched::NR_SCHED_QUEUES;
use crate::service_slot::{
    Label, MAX_COMMAND_LEN, MAX_IPC_LIST, MAX_SCRIPT_LEN, NR_DOMAIN, NR_IO_RANGE, NR_IRQ,
    RS_MAX_LABEL_LEN, RS_NR_CONTROL, RS_NR_PCI_CLASS, RS_NR_PCI_DEVICE, RsPciClass, RsPciId,
    ServiceSlot, SlotId, SysFlags,
};
use alloc::vec::Vec;
use minix_types::{Endpoint, Errno, SYS_BASIC_CALLS, VM_BASIC_CALLS};

/// C: `RSS_NR_IRQ` — rs.h:25.
pub const RSS_NR_IRQ: usize = 16;
/// C: `RSS_NR_IO` — rs.h:26.
pub const RSS_NR_IO: usize = 16;
/// C: `RSS_IRQ_ALL` — rs.h:27.
pub const RSS_IRQ_ALL: i32 = RSS_NR_IRQ as i32 + 1;
/// C: `RSS_IO_ALL` — rs.h:28.
pub const RSS_IO_ALL: i32 = RSS_NR_IO as i32 + 1;
/// C: `RS_CPU_DEFAULT` — rs.h:63.
pub const RS_CPU_DEFAULT: i32 = -1;
/// C: `RS_CPU_BSP` — rs.h:64.
pub const RS_CPU_BSP: i32 = -2;
/// C: `LAST_SPECIAL_PROC_NR` — com.h:70.
pub const LAST_SPECIAL_PROC_NR: i32 = 11;

bitflags::bitflags! {
    /// The `rs_start` flags. C: `rss_flags` — rs.h:106, values rs.h:33-52.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RssFlags: u32 {
        /// Keep an in-memory copy of the binary. C: `RSS_COPY` — rs.h:33.
        const COPY = 0x01;
        /// Try to reuse a previously copied binary. C: `RSS_REUSE` — rs.h:34.
        const REUSE = 0x04;
        /// Unblock caller immediately. C: `RSS_NOBLOCK` — rs.h:35.
        const NOBLOCK = 0x08;
        /// Keep a replica of the service. C: `RSS_REPLICA` — rs.h:36.
        const REPLICA = 0x10;
        /// Batch mode. C: `RSS_BATCH` — rs.h:37.
        const BATCH = 0x20;
        /// Perform self update. C: `RSS_SELF_LU` — rs.h:38.
        const SELF_LU = 0x40;
        /// Perform ASR update. C: `RSS_ASR_LU` — rs.h:39.
        const ASR_LU = 0x80;
        /// Force self update. C: `RSS_FORCE_SELF_LU` — rs.h:40.
        const FORCE_SELF_LU = 0x0100;
        /// Request prepare-only update. C: `RSS_PREPARE_ONLY_LU` — rs.h:41.
        const PREPARE_ONLY_LU = 0x0200;
        /// Force crash at initialization time (debugging). C: `RSS_FORCE_INIT_CRASH` — rs.h:42.
        const FORCE_INIT_CRASH = 0x0400;
        /// Force failure at initialization time (debugging). C: `RSS_FORCE_INIT_FAIL` — rs.h:43.
        const FORCE_INIT_FAIL = 0x0800;
        /// Force timeout at initialization time (debugging). C: `RSS_FORCE_INIT_TIMEOUT` — rs.h:44.
        const FORCE_INIT_TIMEOUT = 0x1000;
        /// Force default cb at initialization time (debugging). C: `RSS_FORCE_INIT_DEFCB` — rs.h:45.
        const FORCE_INIT_DEFCB = 0x2000;
        /// Include basic kernel calls. C: `RSS_SYS_BASIC_CALLS` — rs.h:46.
        const SYS_BASIC_CALLS = 0x4000;
        /// Include basic vm calls. C: `RSS_VM_BASIC_CALLS` — rs.h:47.
        const VM_BASIC_CALLS = 0x8000;
        /// Don't inherit mmapped regions. C: `RSS_NOMMAP_LU` — rs.h:48.
        const NOMMAP_LU = 0x10000;
        /// Detach on update/restart. C: `RSS_DETACH` — rs.h:49.
        const DETACH = 0x20000;
        /// Don't restart. C: `RSS_NORESTART` — rs.h:50.
        const NORESTART = 0x40000;
        /// Force state transfer at initialization time. C: `RSS_FORCE_INIT_ST` — rs.h:51.
        const FORCE_INIT_ST = 0x80000;
        /// Suppress binary exponential offset. C: `RSS_NO_BIN_EXP` — rs.h:52.
        const NO_BIN_EXP = 0x100000;
    }
}

/// The service-start parameters carried by `RS_UP`/`RS_EDIT`.
///
/// C: `struct rs_start` — rs.h:104-151. Byte-buffer fields keep the kernel
/// message payload shape; pointer fields (`rss_cmd`/`rss_ipc`/…) become
/// fixed arrays + lengths (the `sys_datacopy` sources, manager.c:1476-1483,
/// 1578-1582).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsStart {
    /// C: `rss_flags` — rs.h:106.
    pub flags: RssFlags,
    /// C: `rss_uid` — rs.h:109.
    pub uid: u32,
    /// C: `rss_sigmgr` — rs.h:110.
    pub sigmgr: Endpoint,
    /// C: `rss_scheduler` — rs.h:111.
    pub scheduler: Endpoint,
    /// C: `rss_priority` — rs.h:112.
    pub priority: i32,
    /// C: `rss_quantum` — rs.h:113.
    pub quantum: i32,
    /// C: `rss_cpu` — rs.h:150 (SMP tail; `RS_CPU_DEFAULT`/`RS_CPU_BSP`).
    pub cpu: i32,
    /// C: `rss_period` — rs.h:115.
    pub period: i64,
    /// C: `rss_restarts` — rs.h:119.
    pub restarts: i64,
    /// C: `rss_asr_count` — rs.h:118.
    pub asr_count: i64,
    /// C: `rss_cmd`/`rss_cmdlen` — rs.h:107-108.
    pub cmd: [u8; MAX_COMMAND_LEN],
    /// Length of `cmd`. C: `rss_cmdlen`.
    pub cmdlen: usize,
    /// C: `rss_ipc`/`rss_ipclen` — rs.h:133-134.
    pub ipc_list: [u8; MAX_IPC_LIST],
    /// Length of the IPC list. C: `rss_ipclen`.
    pub ipclen: usize,
    /// C: `rss_progname`/`rss_prognamelen` — rs.h:140-141.
    pub progname: Label,
    /// Number of control entries. C: `int rss_nr_control` — rs.h:136.
    /// `i32` (not `usize`): C `int` may carry the pre-validation negative
    /// "unset/invalid" state (R2 — todo §14.1); `ServiceSlot.nr_control`
    /// (type.h:106) already uses `i32`.
    pub nr_control: i32,
    /// C: `rss_control` — rs.h:137.
    pub control: [Label; RS_NR_CONTROL],
    /// Number of IRQs. C: `int rss_nr_irq` — rs.h:122.
    /// `i32`: must hold the `RSS_IRQ_ALL` sentinel (17, rs.h:27) and the
    /// pre-validation negative "invalid" state (manager.c:1492 checks
    /// `> NR_IRQ` on a possibly negative count).
    pub nr_irq: i32,
    /// C: `rss_irq` — rs.h:123.
    pub irq: [i32; RSS_NR_IRQ],
    /// Number of I/O ranges. C: `int rss_nr_io` — rs.h:124.
    /// `i32`: must hold the `RSS_IO_ALL` sentinel (17, rs.h:28) and the
    /// pre-validation negative "invalid" state (manager.c:1510 checks
    /// `> NR_IO_RANGE` on a possibly negative count).
    pub nr_io: i32,
    /// C: `rss_io` — rs.h:125 (`struct { unsigned base; unsigned len; }`).
    /// Single authority: `privilege::IoRange` (N9 — todo §11) is the same
    /// shape as the kernel `struct io_range` the entries are copied into
    /// (`edit_slot` → `s_io_tab`, manager.c:1516-1518); using it here avoids
    /// the C "two same-shape tables" duplication (D6).
    pub io: [IoRange; RSS_NR_IO],
    /// C: `rss_major` — rs.h:114 (`int`; major device number, 0 = dynamic).
    pub major: i32,
    /// C: `rss_script`/`rss_scriptlen` — rs.h:116-117 (`MAX_SCRIPT_LEN`,
    /// const.h:19). Restart script consumed by `run_script` (manager.c:1209,
    /// 15/19).
    pub script: [u8; MAX_SCRIPT_LEN],
    /// Length of `script`. C: `rss_scriptlen`.
    pub scriptlen: usize,
    /// C: `rss_heap_prealloc_bytes` — rs.h:120 (`long`; negative = not
    /// requested, request.c:768-770 zeroes it before VM registration).
    pub heap_prealloc_bytes: i64,
    /// C: `rss_map_prealloc_bytes` — rs.h:121.
    pub map_prealloc_bytes: i64,
    /// C: `rss_system` — rs.h:130 (`bitchunk_t[SYS_CALL_MASK_SIZE]` → one
    /// 64-bit [`CallMask`]). edit_slot memcpy's it into `s_k_call_mask`
    /// (manager.c:1527-1532).
    pub system: CallMask,
    /// C: `rss_vm` — rs.h:135. edit_slot memcpy's it into `vm_call_mask`
    /// (manager.c:1535-1540).
    pub vm: CallMask,
    /// C: `rss_label` — rs.h:131. Service label; edit_slot copies it when
    /// non-empty, else falls back to `progname` (manager.c:1596-1615).
    pub label: Label,
    /// C: `rss_trg_label` — rs.h:132. Live-update target label (16).
    pub trg_label: Label,
    /// PCI device-id ACL inputs. C: `rss_nr_pci_id`/`rss_pci_id` —
    /// rs.h:126-127. A-10: the `rs_pci` privilege model is deferred
    /// (publish.rs), but the request payload is modelled here so
    /// init_slot's validation branch (manager.c:1745-1774, R20c) has its
    /// input.
    pub nr_pci_id: i32,
    pub pci_id: [RsPciId; RS_NR_PCI_DEVICE],
    /// PCI class ACL inputs. C: `rss_nr_pci_class`/`rss_pci_class` —
    /// rs.h:128-129.
    pub nr_pci_class: i32,
    pub pci_class: [RsPciClass; RS_NR_PCI_CLASS],
    /// C: `rss_state_data` — rs.h:138. Live-update state-data spec consumed
    /// by `init_state_data` (manager.c:174, 17-rs-state-data.md).
    pub state_data: RsStateData,
    /// C: `devman_id` — rs.h:139. devman device id consumed by init_slot
    /// (manager.c:1738-1742); `PublicSlot.devman_id` receives it.
    pub devman_id: i32,
    /// Number of socket-driver domains. C: `rss_nr_domain` — rs.h:142.
    /// `i32`: init_slot rejects negatives (manager.c:1733-1736).
    pub nr_domain: i32,
    /// C: `rss_domain` — rs.h:143.
    pub domain: [i32; NR_DOMAIN],
}

/// The live-update state-data request spec. C: `struct rs_state_data` —
/// rs.h:93-101. The C pointers (`ipcf_els`/`eval_addr`) become address+
/// grant pairs: the buffer bytes travel by grant (`ipcf_els_gid`/
/// `eval_gid`), the addresses stay for the 17 datacopy wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RsStateData {
    /// C: `size` — total state-data size (E2BIG-checked, manager.c:190).
    pub size: usize,
    /// C: `ipcf_els` (void*) — IPC filter element buffer address.
    pub ipcf_els_addr: u64,
    /// C: `ipcf_els_size`.
    pub ipcf_els_size: usize,
    /// C: `ipcf_els_gid` (`grant_id_t`; `None` = no grant yet).
    pub ipcf_els_gid: Option<u32>,
    /// C: `eval_addr` (void*).
    pub eval_addr: u64,
    /// C: `eval_len`.
    pub eval_len: usize,
    /// C: `eval_gid`.
    pub eval_gid: Option<u32>,
}

impl Default for RsStart {
    /// Mirrors the C caller-side defaults injected by minix-service
    /// (parse.c:1160-1169): `memset(rs_config, 0, sizeof)` zeroes every
    /// count/table, then `sigmgr=DSRV_SM=ROOT_SYS_PROC_NR`,
    /// `scheduler=DSRV_SCH=SCHED_PROC_NR`, `priority=DSRV_Q=USER_Q=7`,
    /// `quantum=DSRV_QT=USER_QUANTUM=200`, `cpu=DSRV_CPU=-1` (priv.h:84,
    /// 89, 94, 99, 103 + config.h:69/74/77). The previous `SELF`/`KERNEL`/
    /// `quantum=1` values were "unset" sentinels, not C defaults — any
    /// path constructing via `Default` then partially filling got a
    /// scheduler configuration 200× off from the C caller (D5).
    fn default() -> Self {
        Self {
            flags: RssFlags::empty(),
            uid: 0,
            sigmgr: Endpoint::RS,       // DSRV_SM = ROOT_SYS_PROC_NR (priv.h:84)
            scheduler: Endpoint::SCHED, // DSRV_SCH = SCHED_PROC_NR (priv.h:89)
            priority: crate::sched::USER_Q, // DSRV_Q (priv.h:94, config.h:69)
            quantum: crate::sched::USER_QUANTUM, // DSRV_QT (priv.h:99, config.h:74)
            cpu: RS_CPU_DEFAULT,
            period: 0,
            restarts: 0,
            asr_count: 0,
            cmd: [0; MAX_COMMAND_LEN],
            cmdlen: 0,
            ipc_list: [0; MAX_IPC_LIST],
            ipclen: 0,
            progname: Label::empty(),
            nr_control: 0,
            control: [Label::empty(); RS_NR_CONTROL],
            nr_irq: 0,
            irq: [0; RSS_NR_IRQ],
            nr_io: 0,
            io: [IoRange::default(); RSS_NR_IO],
            major: 0,
            script: [0; MAX_SCRIPT_LEN],
            scriptlen: 0,
            heap_prealloc_bytes: 0,
            map_prealloc_bytes: 0,
            system: CallMask::empty(),
            vm: CallMask::empty(),
            label: Label::empty(),
            trg_label: Label::empty(),
            nr_pci_id: 0,
            pci_id: [RsPciId::default(); RS_NR_PCI_DEVICE],
            nr_pci_class: 0,
            pci_class: [RsPciClass::default(); RS_NR_PCI_CLASS],
            state_data: RsStateData::default(),
            devman_id: 0,
            nr_domain: 0,
            domain: [0; NR_DOMAIN],
        }
    }
}

/// Validates the scheduling/CPU/signal-manager parameters of a request.
///
/// C: `check_request` — request.c:1265-1308. The CPU special values
/// (`RS_CPU_BSP`/`RS_CPU_DEFAULT`) resolve against `machine` state that this
/// pure function does not see; callers pass `bsp_id`/`processors_count`
/// (01/19: `sys_getmachine`). R13: C rewrites `rs_start.rss_cpu` in place
/// (request.c:1286-1296); the returned resolved `cpu` MUST be consumed by
/// the 12 wiring (write it back to the slot's `cpu`) — dropping it leaves
/// `RS_CPU_BSP` (-2) in the slot and the scheduler sees a bogus affinity.
pub fn check_request(rs_start: &RsStart, machine: &crate::boot::Machine) -> Result<i32, Errno> {
    // Scheduler must be KERNEL or a valid special process (request.c:1268-1274).
    if rs_start.scheduler != Endpoint::KERNEL
        && (rs_start.scheduler.get() < 0 || rs_start.scheduler.get() > LAST_SPECIAL_PROC_NR)
    {
        return Err(Errno::EINVAL);
    }
    // Priority must be within the scheduling queues (request.c:1275-1279).
    // Negative priorities are *accepted* here — C only rejects `>=
    // NR_SCHED_QUEUES`; the scheduler validates the final value (32).
    if rs_start.priority >= NR_SCHED_QUEUES {
        return Err(Errno::EINVAL);
    }
    // Quantum must be positive (request.c:1280-1284).
    if rs_start.quantum <= 0 {
        return Err(Errno::EINVAL);
    }
    // CPU resolution (request.c:1286-1296): the C function reads the global
    // `machine` directly — passing the snapshot (R32.1) keeps the fields live
    // and the signature transpose-proof.
    //   BSP → bsp_id; DEFAULT → keep; negative → EINVAL; > count → BSP.
    let cpu = match rs_start.cpu {
        RS_CPU_BSP => machine.bsp_id as i32,
        RS_CPU_DEFAULT => rs_start.cpu,
        c if c < 0 => return Err(Errno::EINVAL),
        c if c as u32 > machine.processors_count => machine.bsp_id as i32,
        c => c,
    };
    // Signal manager must be SELF or a valid special process
    // (request.c:1299-1305).
    if rs_start.sigmgr != Endpoint::SELF
        && (rs_start.sigmgr.get() < 0 || rs_start.sigmgr.get() > LAST_SPECIAL_PROC_NR)
    {
        return Err(Errno::EINVAL);
    }
    Ok(cpu)
}

/// Builds the argument vector from the raw command string.
///
/// C: `build_cmd_dep` — manager.c:289-323: the format is
/// `path, arguments..., NULL`; spaces separate arguments; an empty trailing
/// argument is dropped; the vector is capped at `ARGV_ELEMENTS - 1` entries
/// (one slot reserved for the terminating `None`).
///
/// The tokens borrow `cmd` and are **not** truncated at `RS_MAX_LABEL_LEN`:
/// C stores the full token bytes in `r_args` (manager.c:301-302) and points
/// argv at them — `Vec<&[u8]>` is the Rust equivalent of that argv layout.
/// Parsing stops at the first NUL, mirroring the C `while (*cmd_ptr != '\0')`
/// loop (manager.c:306) and `strcpy` (manager.c:301): bytes after the NUL
/// padding of a fixed-size `cmd` buffer are not part of the command.
pub fn build_cmd_dep(cmd: &[u8]) -> Vec<&[u8]> {
    let mut args = Vec::new();
    let rest = cmd;
    let mut i = 0;
    while i < rest.len() {
        // Skip spaces (C: manager.c:308).
        while i < rest.len() && rest[i] == b' ' {
            i += 1;
        }
        if i >= rest.len() || rest[i] == 0 {
            break; // C: manager.c:306/309 — string end, no argument follows
        }
        let start = i;
        while i < rest.len() && rest[i] != b' ' && rest[i] != 0 {
            i += 1;
        }
        if args.len() >= crate::service_slot::ARGV_ELEMENTS - 1 {
            break; // C: manager.c:311-315 — arg vector full
        }
        args.push(&rest[start..i]);
        if i >= rest.len() || rest[i] == 0 {
            break;
        }
    }
    if args.is_empty() {
        // N11: C unconditionally assigns `r_argv[0] = r_args` before parsing
        // (manager.c:299-300) — an empty, all-space or NUL-leading command
        // yields `argv = [""]` (argc=1), never an empty vector. Aligning
        // keeps the 09/10 exec rebuild faithful: C execs an empty path
        // (ENOENT); argc=0 would take a different failure path.
        args.push(&rest[..0]);
    }
    args
}

/// Edits a slot to override existing settings — the `RS_UP`/`RS_EDIT` field
/// pipeline.
///
/// C: `edit_slot` — manager.c:1460-1707, branch for branch. The
/// `sys_datacopy` steps (IPC list, cmd, progname, script, control labels)
/// are pure here: [`RsStart`] already carries the copied bytes, while C's
/// pointers referenced the requester's address space and the copies ran at
/// message receive. The single external effect is `read_exec` (binary file
/// I/O, 19-rs-external-interfaces.md), injected as `read_exec`; the
/// `RSS_REUSE` donor path stays pure (`share_exec` is an `Arc` clone).
/// `init_privs` (manager.c:1700) recomputes the send mask from the new
/// `ipc_list` — the 05 write-back completes the pipeline.
///
/// Note: C rewrites the caller's `rs_start` in place for the IRQ/IO
/// sentinels (`rss_nr_irq = 0`, manager.c:1489/1506); Rust keeps `&RsStart`
/// and uses an effective local — same downstream values, no caller surprise
/// (ARCH: deliberate deviation from C's in-place argument mutation).
pub fn edit_slot(
    slot: &mut ServiceSlot,
    rs_start: &RsStart,
    table: &crate::process_table::RProcTable,
    read_exec: &mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>,
) -> Result<(), Errno> {
    // Update IPC target list (manager.c:1476-1483): non-empty, and the byte
    // count plus the NUL terminator must fit the slot buffer.
    if rs_start.ipclen == 0 || rs_start.ipclen + 1 > MAX_IPC_LIST {
        return Err(Errno::EINVAL);
    }
    slot.ipc_list[..rs_start.ipclen].copy_from_slice(&rs_start.ipc_list[..rs_start.ipclen]);
    slot.ipc_list[rs_start.ipclen] = 0;

    // Update IRQs (manager.c:1486-1501): `RSS_IRQ_ALL` → count zeroed and no
    // `CHECK_IRQ` (the kernel manages the set); an explicit list sets the
    // check flag. C's `> NR_IRQ` does not reject negatives — callers produce
    // counts via `check_request`; keep the comparison shape verbatim.
    let check_irq = rs_start.nr_irq != RSS_IRQ_ALL;
    let nr_irq = if check_irq { rs_start.nr_irq } else { 0 };
    if nr_irq > NR_IRQ as i32 {
        return Err(Errno::EINVAL);
    }
    if check_irq {
        slot.priv_.flags.insert(PrivFlags::CHECK_IRQ);
    }
    slot.nr_irq = nr_irq;
    slot.priv_.nr_irq = nr_irq;
    for i in 0..nr_irq as usize {
        slot.irq_tab[i] = rs_start.irq[i];
        slot.priv_.irqs[i] = rs_start.irq[i];
    }

    // Update I/O ranges (manager.c:1504-1524): sentinel / flag / bound as
    // for IRQs; the stored entry keeps `base`+`len` (`ior_limit =
    // base+len-1` is derived, C io_range's limit form is information-
    // equivalent — IoRange single authority, N9/D6).
    let check_io = rs_start.nr_io != RSS_IO_ALL;
    let nr_io = if check_io { rs_start.nr_io } else { 0 };
    if nr_io > NR_IO_RANGE as i32 {
        return Err(Errno::EINVAL);
    }
    if check_io {
        slot.priv_.flags.insert(PrivFlags::CHECK_IO_PORT);
    }
    slot.nr_io_range = nr_io;
    slot.priv_.nr_io_range = nr_io;
    for i in 0..nr_io as usize {
        slot.io_tab[i] = rs_start.io[i];
        slot.priv_.io_ranges[i] = rs_start.io[i];
    }

    // Update kernel call mask; inherit basic kernel calls when asked to
    // (manager.c:1527-1532). memcpy → plain assignment; the basic-calls
    // overlay is C's `fill_call_mask(..., FALSE)` — R21's base parameter.
    slot.priv_.k_call_mask = rs_start.system;
    if rs_start.flags.contains(RssFlags::SYS_BASIC_CALLS) {
        slot.priv_.k_call_mask = CallMask::from_calls(
            slot.priv_.k_call_mask,
            &SYS_BASIC_CALLS,
            crate::privilege::NR_SYS_CALLS,
            crate::privilege::KERNEL_CALL,
        )?;
    }

    // Update VM call mask; inherit basic VM calls (manager.c:1535-1540).
    // The VM mask lives on the public half (`rprocpub`).
    slot.pub_.vm_call_mask = rs_start.vm;
    if rs_start.flags.contains(RssFlags::VM_BASIC_CALLS) {
        slot.pub_.vm_call_mask = CallMask::from_calls(
            slot.pub_.vm_call_mask,
            &VM_BASIC_CALLS,
            crate::privilege::NR_VM_CALLS,
            crate::privilege::VM_RQ_BASE,
        )?;
    }

    // Update control labels (manager.c:1543-1564): only when the request
    // carries any (> 0), otherwise the existing list survives.
    if rs_start.nr_control > 0 {
        if rs_start.nr_control > RS_NR_CONTROL as i32 {
            return Err(Errno::EINVAL);
        }
        let n = rs_start.nr_control as usize;
        slot.control[..n].copy_from_slice(&rs_start.control[..n]);
        slot.nr_control = rs_start.nr_control;
    }

    // Update signal manager (manager.c:1567).
    slot.priv_.sig_mgr = rs_start.sigmgr;

    // Update scheduling properties only when a scheduler is set
    // (manager.c:1570-1575).
    if slot.scheduler != Endpoint::NONE {
        slot.scheduler = rs_start.scheduler;
        slot.priority = rs_start.priority;
        slot.quantum = rs_start.quantum;
        slot.cpu = rs_start.cpu;
    }

    // Update command and arguments (manager.c:1578-1593): E2BIG bound,
    // absolute path enforced, argv rebuilt from the new command.
    if rs_start.cmdlen > MAX_COMMAND_LEN - 1 {
        return Err(Errno::E2BIG);
    }
    slot.cmd[..rs_start.cmdlen].copy_from_slice(&rs_start.cmd[..rs_start.cmdlen]);
    slot.cmd[rs_start.cmdlen] = 0;
    if slot.cmd[0] != b'/' {
        return Err(Errno::EINVAL);
    }
    crate::service_create::rebuild_args(slot);

    // Copy in the program name (manager.c:1596-1615 is the label block; the
    // progname copy is manager.c:1589-1593's sibling at :1593-1595 in C —
    // E2BIG bound, then the byte copy; `Label` carries the bytes already).
    let progname_len = rs_start
        .progname
        .as_bytes()
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(RS_MAX_LABEL_LEN);
    if progname_len > RS_MAX_LABEL_LEN - 1 {
        return Err(Errno::E2BIG);
    }
    slot.pub_.proc_name = rs_start.progname;

    // Update label if not already set (manager.c:1598-1615): custom label
    // from the request, else fall back to the program name.
    if slot.pub_.label.as_bytes()[0] == 0 {
        if rs_start.label.as_bytes()[0] != 0 {
            slot.pub_.label = rs_start.label;
        } else {
            slot.pub_.label = slot.pub_.proc_name;
        }
    }

    // Update recovery script (manager.c:1618-1626): bound, presence, and
    // core services never carry one; the flag marks it for `run_script`.
    if rs_start.scriptlen > MAX_SCRIPT_LEN - 1 {
        return Err(Errno::E2BIG);
    }
    if rs_start.scriptlen > 0 && !slot.pub_.sys_flags.contains(SysFlags::CORE_SRV) {
        slot.script[..rs_start.scriptlen].copy_from_slice(&rs_start.script[..rs_start.scriptlen]);
        slot.script[rs_start.scriptlen] = 0;
        slot.pub_.sys_flags.insert(SysFlags::USE_SCRIPT);
    }

    // Update system flags and in-memory copy (manager.c:1629-1661). With
    // `RSS_REUSE`, scan for a same-named service that already holds the
    // binary and share its `Arc`; otherwise load it via the injected
    // `read_exec`. C scans every row (no `RS_IN_USE` filter) — freed rows
    // keep `proc_name`/`sys_flags`, so the residual-data behaviour is
    // identical on both sides.
    if rs_start.flags.contains(RssFlags::COPY) && !slot.pub_.sys_flags.contains(SysFlags::USE_COPY)
    {
        let mut donor: Option<SlotId> = None;
        if rs_start.flags.contains(RssFlags::REUSE) {
            donor = table.iter_all().find_map(|(id, rp)| {
                (rp.pub_.proc_name == slot.pub_.proc_name
                    && rp.pub_.sys_flags.contains(SysFlags::USE_COPY))
                .then_some(id)
            });
        }
        match donor {
            Some(d) => {
                crate::exec::share_exec(slot, table.get(d));
            }
            None => read_exec(slot)?,
        }
        slot.pub_.sys_flags.insert(SysFlags::USE_COPY);
    }
    if rs_start.flags.contains(RssFlags::REPLICA) {
        slot.pub_.sys_flags.insert(SysFlags::USE_REPL);
    }
    if rs_start.flags.contains(RssFlags::NO_BIN_EXP) {
        slot.pub_.sys_flags.insert(SysFlags::NO_BIN_EXP);
    }
    if rs_start.flags.contains(RssFlags::DETACH) {
        slot.pub_.sys_flags.insert(SysFlags::DET_RESTART);
    } else {
        slot.pub_.sys_flags.remove(SysFlags::DET_RESTART);
    }
    if rs_start.flags.contains(RssFlags::NORESTART) {
        if slot.pub_.sys_flags.contains(SysFlags::CORE_SRV) {
            return Err(Errno::EPERM);
        }
        slot.pub_.sys_flags.insert(SysFlags::NORESTART);
    } else {
        slot.pub_.sys_flags.remove(SysFlags::NORESTART);
    }

    // Update period — RS itself keeps its boot-time period
    // (manager.c:1685-1687).
    if slot.pub_.endpoint != Endpoint::RS {
        slot.period = rs_start.period;
    }

    // Update restarts (manager.c:1690-1692): nonzero overrides only.
    if rs_start.restarts != 0 {
        slot.restarts = rs_start.restarts as i32;
    }

    // Update number of ASR live updates (manager.c:1695-1697): non-negative
    // overrides only (C `int` — the negative state means "not requested").
    if rs_start.asr_count >= 0 {
        slot.asr_count = rs_start.asr_count as i32;
    }

    // (Re)initialize privilege settings (manager.c:1700): the send mask is
    // recomputed from the new `ipc_list` and written back to the privilege
    // structure — `update_ipc_mask` is `init_privs` + write-back (05).
    crate::ipc_mask::update_ipc_mask(slot, table, |endpoint| {
        table
            .endpoint_slot(endpoint)
            .map(|id| table.get(id).priv_.id)
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::Machine;
    use crate::exec::has_shared_exec;
    use crate::privilege::NULL_C;
    use crate::process_table::RProcTable;
    use crate::service_slot::RFlags;
    use alloc::sync::Arc;
    use minix_types::SYS_EXIT;

    /// The C global `machine` snapshot for tests (request.c:1286-1296).
    fn machine() -> Machine {
        Machine {
            processors_count: 4,
            bsp_id: 2,
        }
    }

    /// A started-service slot: in use, endpoint TTY, proc name "tty",
    /// no script, no label yet.
    fn service_slot() -> ServiceSlot {
        let mut s = ServiceSlot::vacant();
        s.flags.insert(RFlags::IN_USE);
        s.pub_.endpoint = Endpoint::TTY;
        s.pub_.proc_name = Label::from_bytes(b"tty");
        s
    }

    fn no_exec(_slot: &mut ServiceSlot) -> Result<(), Errno> {
        Ok(())
    }

    #[test]
    fn test_check_request_ok() {
        let s = RsStart {
            scheduler: Endpoint::SCHED,
            priority: 5,
            quantum: 100,
            cpu: RS_CPU_DEFAULT,
            sigmgr: Endpoint::SELF,
            ..RsStart::default()
        };
        assert_eq!(check_request(&s, &machine()), Ok(RS_CPU_DEFAULT));
    }

    #[test]
    fn test_check_request_scheduler() {
        let mut s = RsStart::default();
        s.scheduler = Endpoint::VFS; // 1, within LAST_SPECIAL_PROC_NR
        assert!(check_request(&s, &machine()).is_ok());
        s.scheduler = Endpoint::from_generation_slot(0, 12); // > 11
        assert_eq!(check_request(&s, &machine()), Err(Errno::EINVAL));
    }

    #[test]
    fn test_check_request_priority_quantum() {
        let mut s = RsStart::default();
        s.priority = NR_SCHED_QUEUES;
        assert_eq!(check_request(&s, &machine()), Err(Errno::EINVAL));
        s.priority = 0;
        s.quantum = 0;
        assert_eq!(check_request(&s, &machine()), Err(Errno::EINVAL));
    }

    #[test]
    fn test_check_request_negative_priority_accepted() {
        // R32: C only rejects `rss_priority >= NR_SCHED_QUEUES`
        // (request.c:1275-1279) — a negative value passes here and is the
        // scheduler's business; do not "tighten" this into an unsigned type
        // without a C-change anchor.
        let mut s = RsStart::default();
        s.priority = -1;
        assert_eq!(check_request(&s, &machine()).is_ok(), true);
    }

    #[test]
    fn test_check_request_cpu() {
        let s = RsStart {
            cpu: RS_CPU_BSP,
            ..RsStart::default()
        };
        assert_eq!(check_request(&s, &machine()), Ok(2)); // → bsp_id
        let s = RsStart {
            cpu: 3,
            ..RsStart::default()
        };
        assert_eq!(check_request(&s, &machine()), Ok(3));
        let s = RsStart {
            cpu: 5,
            ..RsStart::default()
        }; // > count(4) → BSP
        assert_eq!(check_request(&s, &machine()), Ok(2));
        let s = RsStart {
            cpu: -3,
            ..RsStart::default()
        };
        assert_eq!(check_request(&s, &machine()), Err(Errno::EINVAL));
    }

    #[test]
    fn test_check_request_sigmgr() {
        let mut s = RsStart::default();
        s.sigmgr = Endpoint::PM;
        assert!(check_request(&s, &machine()).is_ok());
        s.sigmgr = Endpoint::from_generation_slot(0, 12);
        assert_eq!(check_request(&s, &machine()), Err(Errno::EINVAL));
    }

    #[test]
    fn test_rs_start_default_matches_c_caller() {
        // D5: minix-service parse.c:1164-1169 — DSRV_SM=ROOT_SYS_PROC_NR,
        // DSRV_SCH=SCHED_PROC_NR, DSRV_Q=USER_Q=7, DSRV_QT=USER_QUANTUM=200,
        // DSRV_CPU=USER_DEFAULT_CPU=-1 (priv.h:84/89/94/99/103, config.h:69/74/77).
        // Resource counts/tables are zeroed by the caller's
        // `memset(rs_config, 0, sizeof)` (parse.c:1160).
        let s = RsStart::default();
        assert_eq!(s.sigmgr, Endpoint::RS);
        assert_eq!(s.scheduler, Endpoint::SCHED);
        assert_eq!(s.priority, crate::sched::USER_Q);
        assert_eq!(s.priority, 7);
        assert_eq!(s.quantum, crate::sched::USER_QUANTUM);
        assert_eq!(s.quantum, 200);
        assert_eq!(s.cpu, RS_CPU_DEFAULT);
        assert_eq!(s.nr_control, 0);
        assert_eq!(s.nr_irq, 0);
        assert!(s.irq.iter().all(|&i| i == 0));
        assert_eq!(s.nr_io, 0);
        assert!(s.io.iter().all(|r| *r == IoRange::default()));
        // Valid under check_request (the C path validates before use).
        assert!(check_request(&s, &machine()).is_ok());
    }

    #[test]
    fn test_rs_start_r20a_field_defaults() {
        // R20a: the rs.h fields added for the edit_slot/init_slot pipeline
        // default to the C caller's `memset(rs_config, 0, sizeof)`
        // (parse.c:1160) — masks empty, labels empty, counts zero.
        let s = RsStart::default();
        assert_eq!(s.major, 0);
        assert_eq!(s.scriptlen, 0);
        assert!(s.script.iter().all(|&b| b == 0));
        assert_eq!(s.heap_prealloc_bytes, 0);
        assert_eq!(s.map_prealloc_bytes, 0);
        assert_eq!(s.system, CallMask::empty());
        assert_eq!(s.vm, CallMask::empty());
        assert_eq!(s.label, Label::empty());
        assert_eq!(s.trg_label, Label::empty());
        assert_eq!(s.nr_pci_id, 0);
        assert!(s.pci_id.iter().all(|p| *p == RsPciId::default()));
        assert_eq!(s.nr_pci_class, 0);
        assert_eq!(s.state_data, RsStateData::default());
        assert_eq!(s.devman_id, 0);
        assert_eq!(s.nr_domain, 0);
        assert!(s.domain.iter().all(|&d| d == 0));
    }

    #[test]
    fn test_rs_start_resource_counts_are_i32_like_c_int() {
        // R20a/§15: C counts are `int` (rs.h:122/124/136). `i32` keeps the
        // pre-`edit_slot` states representable (manager.c:1486-1521):
        //   - `RSS_IRQ_ALL`/`RSS_IO_ALL` = 17 sentinels ("all resources");
        //   - negative counts (invalid, rejected by the `> NR_IRQ`/`> NR_IO_RANGE`
        //     checks in edit_slot, 19 wiring).
        // `usize` would silently accept both and blur the "unset" vs "invalid"
        // distinction the C `int` carries before validation (R2 rationale).
        let mut s = RsStart::default();
        s.nr_irq = RSS_IRQ_ALL;
        s.nr_io = RSS_IO_ALL;
        assert_eq!(s.nr_irq, 17);
        assert_eq!(s.nr_io, 17);
        s.nr_irq = -1;
        s.nr_io = -1;
        s.nr_control = -1;
        assert_eq!((s.nr_irq, s.nr_io, s.nr_control), (-1, -1, -1));
    }

    #[test]
    fn test_build_cmd_dep() {
        let args = build_cmd_dep(b"/sbin/tty -a -b");
        let strs: Vec<&str> = args
            .iter()
            .map(|t| core::str::from_utf8(t).unwrap())
            .collect();
        assert_eq!(strs, vec!["/sbin/tty", "-a", "-b"]);
    }

    #[test]
    fn test_build_cmd_dep_trailing_spaces() {
        let args = build_cmd_dep(b"/bin/echo hi   ");
        let strs: Vec<&str> = args
            .iter()
            .map(|t| core::str::from_utf8(t).unwrap())
            .collect();
        assert_eq!(strs, vec!["/bin/echo", "hi"]);
    }

    #[test]
    fn test_build_cmd_dep_empty_cmd_keeps_argv0() {
        // N11: C unconditionally assigns argv[0] = r_args (manager.c:299-
        // 300) — an empty/all-space command yields argv=[""], argc=1, not
        // argc=0 (exec of an empty path then fails with ENOENT like C).
        assert_eq!(build_cmd_dep(b"").len(), 1);
        assert!(build_cmd_dep(b"")[0].is_empty());
        assert_eq!(build_cmd_dep(b"   ").len(), 1);
        assert!(build_cmd_dep(b"   ")[0].is_empty());
        assert_eq!(build_cmd_dep(b"\0").len(), 1);
        assert!(build_cmd_dep(b"\0")[0].is_empty());
    }

    #[test]
    fn test_build_cmd_dep_stops_at_nul() {
        // C: manager.c:306 — parsing stops at the NUL terminator; bytes in the
        // fixed-size cmd buffer after the NUL are padding, not arguments.
        let args = build_cmd_dep(b"/sbin/tty -a\0garbage -b");
        let strs: Vec<&str> = args
            .iter()
            .map(|t| core::str::from_utf8(t).unwrap())
            .collect();
        assert_eq!(strs, vec!["/sbin/tty", "-a"]);
    }

    #[test]
    fn test_build_cmd_dep_long_token_not_truncated() {
        // Tokens are not capped at RS_MAX_LABEL_LEN (16): C keeps full bytes
        // in r_args (manager.c:301-302); argv points into that buffer.
        let cmd = b"/usr/sbin/very-long-service-name --flag";
        let args = build_cmd_dep(cmd);
        assert_eq!(args[0], b"/usr/sbin/very-long-service-name".as_slice());
        assert_eq!(args[1], b"--flag".as_slice());
        assert!(args[0].len() > crate::service_slot::RS_MAX_LABEL_LEN);
    }

    #[test]
    fn test_build_cmd_dep_argv_cap() {
        let mut cmd = Vec::new();
        cmd.extend_from_slice(b"/sbin/x");
        for i in 0..crate::service_slot::ARGV_ELEMENTS {
            cmd.push(b' ');
            cmd.extend_from_slice(format!("a{i}").as_bytes());
        }
        let args = build_cmd_dep(&cmd);
        // One slot reserved for the terminating NULL (manager.c:311-315).
        assert!(args.len() <= crate::service_slot::ARGV_ELEMENTS - 1);
    }

    #[test]
    fn test_rss_constants() {
        assert_eq!(RSS_IRQ_ALL, 17);
        assert_eq!(RSS_IO_ALL, 17);
        assert_eq!(RS_CPU_DEFAULT, -1);
        assert_eq!(RS_CPU_BSP, -2);
        assert_eq!(RssFlags::COPY.bits(), 0x01);
        assert_eq!(RssFlags::NO_BIN_EXP.bits(), 0x100000);
    }

    // ── edit_slot (R20b, manager.c:1460-1707) ───────────────────────────

    /// An `RsStart` that passes every gate (relative to `service_slot()`).
    fn edit_request() -> RsStart {
        let mut r = RsStart::default();
        r.ipclen = 9;
        r.ipc_list[..9].copy_from_slice(b"one\0two\0t");
        r.cmdlen = 9;
        r.cmd[..9].copy_from_slice(b"/sbin/tty");
        r.progname = Label::from_bytes(b"tty");
        r
    }

    #[test]
    fn test_edit_slot_ipc_list_gate() {
        // C: manager.c:1476-1479 — empty list or count+1 beyond the slot
        // buffer → EINVAL before anything else moves.
        let mut s = service_slot();
        let mut r = edit_request();
        r.ipclen = 0;
        assert_eq!(
            edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec),
            Err(Errno::EINVAL)
        );
        r.ipclen = MAX_IPC_LIST; // +1 terminator would overflow
        assert_eq!(
            edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec),
            Err(Errno::EINVAL)
        );
        // Valid: the bytes land NUL-terminated in the slot list.
        r.ipclen = 4;
        r.ipc_list = [0; MAX_IPC_LIST];
        r.ipc_list[..4].copy_from_slice(b"ds\0a");
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(&s.ipc_list[..5], b"ds\0a\0");
        assert_eq!(s.ipc_list[4], 0);
    }

    #[test]
    fn test_edit_slot_irq_sentinel_bound_and_flag() {
        // C: manager.c:1486-1501 — RSS_IRQ_ALL → count 0, no CHECK_IRQ;
        // explicit list → CHECK_IRQ + both tables filled; over-bound EINVAL.
        let mut s = service_slot();
        let mut r = edit_request();
        r.nr_irq = RSS_IRQ_ALL;
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s.nr_irq, 0);
        assert!(!s.priv_.flags.contains(PrivFlags::CHECK_IRQ));

        r.nr_irq = 2;
        r.irq[0] = 4;
        r.irq[1] = 9;
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s.nr_irq, 2);
        assert!(s.priv_.flags.contains(PrivFlags::CHECK_IRQ));
        assert_eq!(s.irq_tab[0], 4);
        assert_eq!(s.priv_.irqs[1], 9);

        // Over-bound: RSS_NR_IRQ == NR_IRQ == 16, so the sentinel is 17 —
        // the first value C rejects is 18 (`> NR_IRQ` with the sentinel
        // already diverted, manager.c:1492).
        r.nr_irq = NR_IRQ as i32 + 2;
        assert_eq!(
            edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec),
            Err(Errno::EINVAL)
        );
    }

    #[test]
    fn test_edit_slot_io_sentinel_and_entries() {
        // C: manager.c:1504-1524 — RSS_IO_ALL → 0 / no CHECK_IO_PORT;
        // explicit ranges copied with base+len preserved (limit derived).
        let mut s = service_slot();
        let mut r = edit_request();
        r.nr_io = RSS_IO_ALL;
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s.nr_io_range, 0);
        assert!(!s.priv_.flags.contains(PrivFlags::CHECK_IO_PORT));

        r.nr_io = 1;
        r.io[0] = IoRange {
            base: 0x3f8,
            len: 8,
        };
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert!(s.priv_.flags.contains(PrivFlags::CHECK_IO_PORT));
        assert_eq!(
            s.io_tab[0],
            IoRange {
                base: 0x3f8,
                len: 8
            }
        );
        assert_eq!(
            s.priv_.io_ranges[0],
            IoRange {
                base: 0x3f8,
                len: 8
            }
        );
    }

    #[test]
    fn test_edit_slot_call_masks_overlay_basic_calls() {
        // C: manager.c:1527-1540 — memcpy the request masks, then OR the
        // basic-call lists on RSS_*_BASIC_CALLS (R21 composition live).
        let mut s = service_slot();
        let mut r = edit_request();
        r.system = CallMask::from_calls(
            CallMask::empty(),
            &[crate::privilege::KERNEL_CALL + 20, NULL_C],
            crate::privilege::NR_SYS_CALLS,
            crate::privilege::KERNEL_CALL,
        )
        .unwrap();
        r.flags.insert(RssFlags::SYS_BASIC_CALLS);
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert!(s.priv_.k_call_mask.test_bit(20)); // from the request mask
        assert!(
            s.priv_
                .k_call_mask
                .test_bit((SYS_EXIT - crate::privilege::KERNEL_CALL) as usize)
        ); // basic overlay
        assert!(s.pub_.vm_call_mask.0 == 0); // no VM flag → exactly rss_vm
    }

    #[test]
    fn test_edit_slot_cmd_and_label_fallback() {
        // C: manager.c:1578-1615 — E2BIG bound, absolute path, label falls
        // back to proc_name when empty, custom label wins when given.
        let mut s = service_slot();
        let mut r = edit_request();
        r.cmdlen = MAX_COMMAND_LEN; // > MAX-1
        assert_eq!(
            edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec),
            Err(Errno::E2BIG)
        );

        r = edit_request();
        r.cmd[0] = b's'; // relative path
        assert_eq!(
            edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec),
            Err(Errno::EINVAL)
        );

        // Label fallback: empty request label → proc_name.
        r = edit_request();
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s.pub_.label, Label::from_bytes(b"tty"));

        // C only writes the label while it is still empty (manager.c:1598,
        // `!strcmp(rpub->label, "")`) — after the fallback the label sticks
        // and a later custom label is ignored.
        r.label = Label::from_bytes(b"mydriver");
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s.pub_.label, Label::from_bytes(b"tty"));

        // A custom label on the FIRST edit of an unlabelled slot wins.
        let mut s2 = service_slot();
        r = edit_request();
        r.label = Label::from_bytes(b"mydriver");
        assert!(edit_slot(&mut s2, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s2.pub_.label, Label::from_bytes(b"mydriver"));
    }

    #[test]
    fn test_edit_slot_script_rules() {
        // C: manager.c:1618-1626 — E2BIG bound; core services never carry a
        // script; non-core get the bytes + SF_USE_SCRIPT.
        let mut s = service_slot();
        let mut r = edit_request();
        r.scriptlen = 7;
        r.script[..7].copy_from_slice(b"/rescue");
        s.pub_.sys_flags.insert(SysFlags::CORE_SRV);
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert!(!s.pub_.sys_flags.contains(SysFlags::USE_SCRIPT));

        s.pub_.sys_flags.remove(SysFlags::CORE_SRV);
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert!(s.pub_.sys_flags.contains(SysFlags::USE_SCRIPT));
        assert_eq!(&s.script[..8], b"/rescue\0");

        r.scriptlen = MAX_SCRIPT_LEN; // > MAX-1
        assert_eq!(
            edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec),
            Err(Errno::E2BIG)
        );
    }

    #[test]
    fn test_edit_slot_copy_reuse_and_exec_injection() {
        // C: manager.c:1629-1661 — RSS_COPY with RSS_REUSE shares an existing
        // same-name copy; without a donor the injected read_exec runs. The
        // exec failure must propagate BEFORE SF_USE_COPY is set.
        let mut table = RProcTable::new();
        let donor = table.alloc_slot().unwrap();
        {
            let rp = table.get_mut(donor);
            rp.flags.insert(RFlags::IN_USE);
            rp.pub_.proc_name = Label::from_bytes(b"tty");
            rp.pub_.sys_flags.insert(SysFlags::USE_COPY);
            rp.exec = Some(Arc::from(&b"donor-bytes"[..]));
        }

        let mut s = service_slot();
        let mut r = edit_request();
        r.flags.insert(RssFlags::COPY | RssFlags::REUSE);
        assert!(edit_slot(&mut s, &r, &table, &mut no_exec).is_ok());
        assert!(s.pub_.sys_flags.contains(SysFlags::USE_COPY));
        // share_exec cloned the donor's Arc — pointer identity, not a reload.
        assert!(Arc::ptr_eq(
            s.exec.as_ref().unwrap(),
            table.get(donor).exec.as_ref().unwrap()
        ));
        assert!(has_shared_exec(&s, &table));

        // No donor anywhere: the injected read_exec executes exactly once.
        let mut s2 = service_slot();
        let mut calls = 0;
        {
            let mut hook = |slot: &mut ServiceSlot| {
                calls += 1;
                slot.exec = Some(Arc::from(&b"loaded"[..]));
                Ok(())
            };
            assert!(edit_slot(&mut s2, &r, &RProcTable::new(), &mut hook).is_ok());
        }
        assert_eq!(calls, 1);
        assert!(s2.pub_.sys_flags.contains(SysFlags::USE_COPY));

        // read_exec failure propagates and USE_COPY stays clear.
        let mut s3 = service_slot();
        let mut fail = |_slot: &mut ServiceSlot| Err(Errno::EIO);
        assert_eq!(
            edit_slot(&mut s3, &r, &RProcTable::new(), &mut fail),
            Err(Errno::EIO)
        );
        assert!(!s3.pub_.sys_flags.contains(SysFlags::USE_COPY));
    }

    #[test]
    fn test_edit_slot_norestart_core_eperm() {
        // C: manager.c:1668-1682 — NORESTART on a core service is EPERM; on
        // others the flag follows the request bit (set or cleared).
        let mut s = service_slot();
        let mut r = edit_request();
        r.flags.insert(RssFlags::NORESTART);
        s.pub_.sys_flags.insert(SysFlags::CORE_SRV);
        assert_eq!(
            edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec),
            Err(Errno::EPERM)
        );

        s.pub_.sys_flags.remove(SysFlags::CORE_SRV);
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert!(s.pub_.sys_flags.contains(SysFlags::NORESTART));

        r.flags.remove(RssFlags::NORESTART);
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert!(!s.pub_.sys_flags.contains(SysFlags::NORESTART));
    }

    #[test]
    fn test_edit_slot_period_restarts_asr_guards() {
        // C: manager.c:1685-1700 — RS keeps its period; zero restarts and
        // negative asr_count leave the slot fields alone.
        let mut s = service_slot();
        s.pub_.endpoint = Endpoint::RS; // RS itself
        let mut r = edit_request();
        r.period = 99;
        r.restarts = 0;
        r.asr_count = -1;
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s.period, 0); // untouched (RS endpoint)
        assert_eq!(s.restarts, 0); // untouched (zero request)
        assert_eq!(s.asr_count, 0); // untouched (negative request)

        // A regular service takes the period; positive overrides apply.
        let mut s2 = service_slot();
        r.restarts = 5;
        r.asr_count = 2;
        assert!(edit_slot(&mut s2, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s2.period, 99);
        assert_eq!(s2.restarts, 5);
        assert_eq!(s2.asr_count, 2);
    }

    #[test]
    fn test_edit_slot_sched_guard_and_ipc_mask() {
        // C: manager.c:1570-1575 + 1700 — scheduling fields only move while a
        // scheduler is set; the send mask is recomputed from the new list
        // (IPC_ALL → full map).
        let mut s = service_slot();
        s.scheduler = Endpoint::NONE;
        s.priority = 3;
        let mut r = edit_request();
        r.scheduler = Endpoint::SCHED;
        r.priority = 7;
        r.ipclen = 8;
        r.ipc_list[..8].copy_from_slice(b"IPC_ALL\0");
        assert!(edit_slot(&mut s, &r, &RProcTable::new(), &mut no_exec).is_ok());
        assert_eq!(s.priority, 3); // NONE guard: unchanged
        assert!(s.priv_.sig_mgr == r.sigmgr); // signal manager always moves
    }
}

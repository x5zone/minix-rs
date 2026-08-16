//! Privilege structure model — the kernel `struct priv` and its RS-side
//! construction/update surface.
//!
//! Mirrors `minix3/minix/kernel/priv.h` (`struct priv`), `include/minix/priv.h`
//! (static priv ids + default macros), `include/minix/const.h:143-153`
//! (`s_flags` bits), and `include/minix/com.h:342-353` (`SYS_PRIV_*` opcodes).
//! The boot-time construction (boot Step 1) and the `sys_privctl` operation
//! surface are documented in 03-rs-privilege.md.
//!
//! # Field modeling scope
//!
//! Only the fields RS actually reads/writes are modeled (03-rs-privilege.md
//! §3.2). The kernel-internal state fields (`s_notify_pending`,
//! `s_asyn_pending`, `s_int_pending`, `s_sig_pending`, `s_ipcf`,
//! `s_alarm_timer`, `s_stack_guard`, `s_diag_sig`, `s_grant_*`, `s_state_*`,
//! `s_asyntab`/`s_asynsize`/`s_asynendpoint`) are not modeled — they are
//! kernel runtime state; the 19-rs-external-interfaces.md wiring serializes
//! the full C layout when `minix-sys` lands.

use crate::service_slot::{NR_IO_RANGE, NR_IRQ, NR_MEM_RANGE};
use core::fmt;
use minix_types::{Endpoint, Errno};

/// Base for kernel calls to SYSTEM. C: `KERNEL_CALL` — com.h:205.
pub const KERNEL_CALL: i32 = 0x600;
/// Number of kernel calls. C: `NR_SYS_CALLS` — com.h:270.
pub const NR_SYS_CALLS: usize = 58;
/// Base for VM calls. C: `VM_RQ_BASE` — com.h:627.
pub const VM_RQ_BASE: i32 = 0xC00;
/// Number of VM calls. C: `NR_VM_CALLS` — com.h:769.
pub const NR_VM_CALLS: usize = 49;

// ── Sentinel values (C: include/minix/priv.h:24-30) ─────────────────────────

/// No targets allowed. C: `NO_M` — priv.h:24.
pub const NO_M: i32 = -1;
/// All targets allowed. C: `ALL_M` — priv.h:25.
pub const ALL_M: i32 = -2;
/// No calls allowed. C: `NO_C` — priv.h:28.
pub const NO_C: i32 = -1;
/// All calls allowed. C: `ALL_C` — priv.h:29.
pub const ALL_C: i32 = -2;
/// Null call entry (terminates a call list). C: `NULL_C` — priv.h:30.
pub const NULL_C: i32 = -3;

// ── Static priv id (C: include/minix/priv.h:10-21) ──────────────────────────

/// Static privilege id — a fixed index into the kernel priv table.
///
/// C: `sys_id_t s_id` (kernel/priv.h:23). Boot services get
/// `static_priv_id(endpoint_slot)` = `NR_TASKS + slot` (priv.h:12), so the
/// mapping is stable and predictable. The id space has
/// `NR_STATIC_PRIV_IDS = NR_BOOT_PROCS` entries (priv.h:10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct PrivId(pub i32);

impl PrivId {
    /// Unassigned priv id. C: `NULL_PRIV_ID` — priv.h:21.
    pub const NONE: PrivId = PrivId(-1);

    /// Static priv id of an endpoint slot. C: `static_priv_id(n) = NR_TASKS + (n)` — priv.h:12.
    ///
    /// `endpoint.slot()` is `_ENDPOINT_P(e)` (endpoint.rs:88-92). For boot
    /// services the slot equals the process number (RS=2 → id 7).
    pub const fn static_priv_id(endpoint_slot: i32) -> PrivId {
        PrivId(minix_types::NR_TASKS as i32 + endpoint_slot)
    }
}

impl fmt::Display for PrivId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── s_flags bits (C: include/minix/const.h:143-153) ─────────────────────────

bitflags::bitflags! {
    /// Privilege flags (`s_flags`).
    ///
    /// C: bits for `s_flags` — `minix3/minix/include/minix/const.h:143-153`.
    /// Bits are orthogonal (a service can be `SYS_PROC|PREEMPTIBLE|
    /// ROOT_SYS_PROC` at once), so a bitset mirrors the C semantics.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PrivFlags: u16 {
        /// Kernel tasks are not preemptible. C: `PREEMPTIBLE` — const.h:143.
        const PREEMPTIBLE = 0x002;
        /// Some processes are not billable. C: `BILLABLE` — const.h:144.
        const BILLABLE = 0x004;
        /// Privilege id assigned dynamically. C: `DYN_PRIV_ID` — const.h:145.
        const DYN_PRIV_ID = 0x008;
        /// System processes have their own priv structure. C: `SYS_PROC` — const.h:147.
        const SYS_PROC = 0x010;
        /// Check if I/O request is allowed. C: `CHECK_IO_PORT` — const.h:148.
        const CHECK_IO_PORT = 0x020;
        /// Check if IRQ can be used. C: `CHECK_IRQ` — const.h:149.
        const CHECK_IRQ = 0x040;
        /// Check if (VM) mem map request is allowed. C: `CHECK_MEM` — const.h:150.
        const CHECK_MEM = 0x080;
        /// This is a root system process instance. C: `ROOT_SYS_PROC` — const.h:151.
        const ROOT_SYS_PROC = 0x100;
        /// This is a vm system process instance. C: `VM_SYS_PROC` — const.h:152.
        const VM_SYS_PROC = 0x200;
        /// This is a live updated sys proc instance. C: `LU_SYS_PROC` — const.h:153.
        const LU_SYS_PROC = 0x400;
        /// This is a restarted sys proc instance. C: `RST_SYS_PROC` — const.h:154.
        const RST_SYS_PROC = 0x800;
    }
}

/// System services. C: `SRV_F` — priv.h:45.
pub const SRV_F: PrivFlags = PrivFlags::SYS_PROC.union(PrivFlags::PREEMPTIBLE);
/// Dynamic system services. C: `DSRV_F` — priv.h:46.
pub const DSRV_F: PrivFlags = SRV_F.union(PrivFlags::DYN_PRIV_ID);
/// Root sys proc. C: `RSYS_F` — priv.h:47.
pub const RSYS_F: PrivFlags = SRV_F.union(PrivFlags::ROOT_SYS_PROC);
/// VM. C: `VM_F` — priv.h:48.
pub const VM_F: PrivFlags = PrivFlags::SYS_PROC.union(PrivFlags::VM_SYS_PROC);
/// User processes. C: `USR_F` — priv.h:49.
pub const USR_F: PrivFlags = PrivFlags::BILLABLE.union(PrivFlags::PREEMPTIBLE);
/// Immutable bits. C: `IMM_F` — priv.h:50.
pub const IMM_F: PrivFlags = PrivFlags::ROOT_SYS_PROC
    .union(PrivFlags::VM_SYS_PROC)
    .union(PrivFlags::PREEMPTIBLE);

// ── Trap mask (C: s_trap_mask, kernel/priv.h:34) ────────────────────────────

bitflags::bitflags! {
    /// Allowed system call traps (`s_trap_mask`).
    ///
    /// C: `short s_trap_mask` (kernel/priv.h:34); bit positions are the IPC
    /// call numbers (`include/minix/ipcconst.h:7-13`). `SENDA` (16) does not
    /// fit the C `short` and is never set via this field.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TrapMask: u16 {
        /// Blocking send. C: `SEND` — ipcconst.h:7.
        const SEND = 1 << 1;
        /// Blocking receive. C: `RECEIVE` — ipcconst.h:8.
        const RECEIVE = 1 << 2;
        /// SEND + RECEIVE. C: `SENDREC` — ipcconst.h:9.
        const SENDREC = 1 << 3;
        /// Asynchronous notify. C: `NOTIFY` — ipcconst.h:10.
        const NOTIFY = 1 << 4;
        /// Nonblocking send. C: `SENDNB` — ipcconst.h:11.
        const SENDNB = 1 << 5;
    }
}

impl TrapMask {
    /// System services: all traps allowed. C: `SRV_T = ~0` — priv.h:61.
    pub const SRV_T: TrapMask = TrapMask::from_bits_truncate(!0);
    /// Clock and system tasks. C: `CSK_T = (1 << RECEIVE)` — priv.h:60.
    pub const CSK_T: TrapMask = TrapMask::RECEIVE;
    /// User processes. C: `USR_T = (1 << SENDREC)` — priv.h:63.
    pub const USR_T: TrapMask = TrapMask::SENDREC;

    /// `SRV_OR_USR(rp, SRV_T, USR_T)` — main.c:271.
    pub const fn srv_or_usr(is_sys_proc: bool) -> TrapMask {
        if is_sys_proc {
            TrapMask::SRV_T
        } else {
            TrapMask::USR_T
        }
    }
}

// ── Bitmaps (C: bitchunk_t[2] — 64-bit covers both call spaces) ─────────────

/// A 64-bit call/target bitmap.
///
/// C: `bitchunk_t s_k_call_mask[SYS_CALL_MASK_SIZE]` (kernel/priv.h:38) and
/// `bitchunk_t vm_call_mask[VM_CALL_MASK_SIZE]` (rs.h:176) are 2×u32 chunks
/// (`BITMAP_CHUNKS(58)=BITMAP_CHUNKS(49)=2`, com.h:270-272,769-770). Rust
/// merges them into one u64 — the bit semantics are identical and neither call
/// space exceeds 64 bits (ARCH A-3 style simplification; serialization
/// boundary belongs to 19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CallMask(pub u64);

impl CallMask {
    /// All bits set (C: `call_mask[i] = ~0` for every chunk — utility.c:131-133).
    pub const fn all() -> CallMask {
        CallMask(u64::MAX)
    }

    /// No bits set.
    pub const fn empty() -> CallMask {
        CallMask(0)
    }

    /// Sets the `offset`-th bit.
    ///
    /// Panics on `offset >= 64` in **all** builds (N7 — todo §11): the old
    /// `1u64 << offset` silently masked the shift in release builds (x86 shl
    /// semantics), setting the wrong bit — for a call mask that meant
    /// "wrong syscall allowed". Callers bound-check first (`from_calls`
    /// returns `Err(EINVAL)`; priv ids are kernel-bounded).
    pub const fn set_bit(mut self, offset: usize) -> CallMask {
        assert!(offset < 64, "CallMask bit offset out of range");
        self.0 |= 1u64 << offset;
        self
    }

    /// Tests the `offset`-th bit.
    pub const fn test_bit(self, offset: usize) -> bool {
        assert!(offset < 64, "CallMask bit offset out of range");
        self.0 & (1u64 << offset) != 0
    }

    /// Fills a call mask from an unordered set of calls.
    ///
    /// C: `fill_call_mask` — `minix3/minix/servers/rs/utility.c:100-141`.
    /// `calls` is the unordered call list terminated by `NULL_C`; a single
    /// `ALL_C` entry fills the mask completely; otherwise each entry sets
    /// bit `calls[i] - call_base` (`SET_BIT(call_mask, calls[i] - call_base)`,
    /// utility.c:139). `is_init` clears the mask first (utility.c:133-137).
    pub fn from_calls(
        calls: &[i32],
        tot_nr_calls: usize,
        call_base: i32,
        is_init: bool,
    ) -> Result<CallMask, Errno> {
        // Count non-NULL_C entries (utility.c:116-121).
        let nr_calls = calls.iter().take_while(|&&c| c != NULL_C).count();

        // Single ALL_C entry → completely filled mask (utility.c:122-129).
        if nr_calls == 1 && calls[0] == ALL_C {
            let mut m = CallMask::all();
            // C fills `call_mask_size` chunks of ~0; bits beyond the call
            // space are also set there but never consulted.
            // N7: `(1u64 << tot_nr_calls) - 1` overflowed at 64 bits — a
            // future raise of NR_SYS_CALLS/NR_VM_CALLS would have wrapped.
            let mask = if tot_nr_calls >= 64 {
                u64::MAX
            } else {
                (1u64 << tot_nr_calls) - 1
            };
            m.0 &= mask;
            return Ok(m);
        }

        let mut m = if is_init {
            CallMask::empty()
        } else {
            // C does not zero when `is_init` is false; the caller is expected
            // to pass a pre-zeroed mask (boot uses TRUE; 05 composes masks).
            CallMask(0)
        };
        for &c in calls.iter().take(nr_calls) {
            let offset = (c - call_base) as usize;
            // N7: fail closed on an out-of-range call number (was a
            // debug-only assert — release silently set the wrong bit, which
            // for a call mask means "wrong syscall allowed"). The call list
            // originates from boot tables / RS_UP messages (attacker-
            // influenceable); EINVAL instead of a panic.
            if offset >= tot_nr_calls {
                return Err(Errno::EINVAL);
            }
            m = m.set_bit(offset);
        }
        Ok(m)
    }
}

/// The IPC send target map (`s_ipc_to`).
///
/// C: `sys_map_t s_ipc_to` — kernel/priv.h:35; `NR_SYS_PROCS` = 64 bits
/// (`sys_config.h:9`), one bit per target priv id. The bit-level composition
/// semantics (`r_ipc_list`, `add_forward_ipc`) belong to 05-rs-ipc-sendmask.md;
/// this type only carries the 64-bit map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SysMap(pub u64);

impl SysMap {
    /// All bits set — `fill_send_mask(mask, TRUE)` (utility.c:82-98).
    pub const fn all() -> SysMap {
        SysMap(u64::MAX)
    }

    /// All bits cleared — `fill_send_mask(mask, FALSE)`.
    pub const fn empty() -> SysMap {
        SysMap(0)
    }

    /// Tests the `priv_id`-th bit (`get_sys_bit` — kernel/const.h:20).
    pub const fn test(self, priv_id: usize) -> bool {
        assert!(priv_id < 64, "SysMap priv id out of range");
        self.0 & (1u64 << priv_id) != 0
    }

    /// Sets the `priv_id`-th bit (`set_sys_bit` — kernel/const.h:24).
    pub const fn set(mut self, priv_id: usize) -> SysMap {
        assert!(priv_id < 64, "SysMap priv id out of range");
        self.0 |= 1u64 << priv_id;
        self
    }
}

// ── Resource ranges (C: kernel/priv.h:54,57,60) ─────────────────────────────

/// I/O port range. C: `struct io_range` — kernel/priv.h:13-16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IoRange {
    pub base: u32,
    pub len: u32,
}

/// Memory range. C: `struct minix_mem_range` — kernel/priv.h:57 (via minix type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemRange {
    pub base: u64,
    pub len: u64,
}

// ── SRV_OR_USR (C: servers/rs/const.h:71) ───────────────────────────────────

/// `SRV_OR_USR(rp, X, Y)` — `minix3/minix/servers/rs/const.h:71`.
///
/// Selects the SRV_* default for system processes (`SYS_PROC` set) and the
/// USR_* default otherwise. C reads `rp->r_priv.s_flags` for the test; the
/// Rust call site passes the already-computed `is_sys_proc` to break the
/// "read the field we are about to write" cycle.
pub const fn srv_or_usr<T: Copy>(is_sys_proc: bool, srv: T, usr: T) -> T {
    if is_sys_proc { srv } else { usr }
}

// ── The privilege structure (C: struct priv, kernel/priv.h:21-65) ───────────

/// The privilege structure passed to the kernel.
///
/// C: `struct priv` — `minix3/minix/kernel/priv.h:21-65`; RS stores it
/// value-embedded as `ixfer_priv_s r_priv` (`servers/rs/type.h:88`) and passes
/// it whole to `sys_privctl` (via `data_copy`, do_privctl.c:123-126).
/// Construction: 03-rs-privilege.md §4.2 (`boot_priv`); updates:
/// `set_sig_mgrs` (sched.rs) and `do_edit` (08).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Privilege {
    /// Static priv id. C: `s_id` — kernel/priv.h:23.
    pub id: PrivId,
    /// Priv flags. C: `s_flags` — kernel/priv.h:24.
    pub flags: PrivFlags,
    /// Initialization flags given to the process. C: `s_init_flags` — kernel/priv.h:25.
    pub init_flags: u32,
    /// Allowed system call traps. C: `s_trap_mask` — kernel/priv.h:34.
    pub trap_mask: TrapMask,
    /// Allowed destination processes. C: `s_ipc_to` — kernel/priv.h:35 (05).
    pub ipc_to: SysMap,
    /// Allowed kernel calls. C: `s_k_call_mask` — kernel/priv.h:38.
    pub k_call_mask: CallMask,
    /// Signal manager for system signals. C: `s_sig_mgr` — kernel/priv.h:40.
    pub sig_mgr: Endpoint,
    /// Backup signal manager. C: `s_bak_sig_mgr` — kernel/priv.h:41.
    pub bak_sig_mgr: Endpoint,

    // Resource white-lists (driver-facing; RS passes them through).
    /// Allowed I/O ports. C: `s_io_tab` — kernel/priv.h:54.
    pub io_ranges: [IoRange; NR_IO_RANGE],
    /// Number of I/O ranges. C: `s_nr_io_range` — kernel/priv.h:53 (`int`).
    pub nr_io_range: i32,
    /// Allowed memory ranges. C: `s_mem_tab` — kernel/priv.h:57.
    pub mem_ranges: [MemRange; NR_MEM_RANGE],
    /// Number of memory ranges. C: `s_nr_mem_range` — kernel/priv.h:56 (`int`).
    pub nr_mem_range: i32,
    /// Allowed IRQ lines. C: `s_irq_tab` — kernel/priv.h:60.
    pub irqs: [u32; NR_IRQ],
    /// Number of IRQ lines. C: `s_nr_irq` — kernel/priv.h:59 (`int`).
    pub nr_irq: i32,
}

impl Privilege {
    /// A zeroed privilege structure — C's "cleared" row before Step 1 fills it.
    pub fn vacant() -> Privilege {
        Privilege {
            id: PrivId::NONE,
            flags: PrivFlags::empty(),
            init_flags: 0,
            trap_mask: TrapMask::empty(),
            ipc_to: SysMap::empty(),
            k_call_mask: CallMask::empty(),
            sig_mgr: Endpoint::NONE,
            bak_sig_mgr: Endpoint::NONE,
            io_ranges: [IoRange::default(); NR_IO_RANGE],
            nr_io_range: 0,
            mem_ranges: [MemRange::default(); NR_MEM_RANGE],
            nr_mem_range: 0,
            irqs: [0; NR_IRQ],
            nr_irq: 0,
        }
    }

    /// Boot Step 1 priv assembly.
    ///
    /// C: main.c:258-280 — the "Set privileges" block of `sef_cb_init_fresh`.
    /// `entry` is one row of `boot_image_priv_table` (table.c:15-30);
    /// `is_sys_proc` is `entry.flags & SYS_PROC != 0` (SRV_OR_USR test,
    /// const.h:71). The VM call mask (main.c:317) targets `rprocpub`, not the
    /// priv structure, so it is filled at the call site (boot.rs Step 1).
    pub fn boot_priv(flags: PrivFlags, endpoint_slot: i32) -> Privilege {
        let is_sys_proc = flags.contains(PrivFlags::SYS_PROC);
        Privilege {
            id: PrivId::static_priv_id(endpoint_slot), // main.c:265-266
            flags,                                     // main.c:269
            init_flags: 0,                             // SRV_I/USR_I = 0 (main.c:270)
            trap_mask: TrapMask::srv_or_usr(is_sys_proc), // main.c:271
            // main.c:272-273: `ipc_to = SRV_OR_USR(rp, SRV_M, USR_M)` —
            //   both are ALL_M, so `fill_send_mask(mask, TRUE)` sets all bits.
            ipc_to: SysMap::all(),
            // main.c:278-280: `calls = SRV_OR_USR(rp, SRV_KC, USR_KC) == ALL_C
            //   ? all_c : no_c` — boot services use the SRV_KC=ALL_C branch.
            k_call_mask: CallMask::from_calls(&[ALL_C, NULL_C], NR_SYS_CALLS, KERNEL_CALL, true)
                .expect("boot call list is a constant in range"),
            // main.c:274: `s_sig_mgr = SRV_OR_USR(rp, SRV_SM, USR_SM)` —
            //   RS (2) for system services, PM (0) for user processes.
            sig_mgr: if is_sys_proc {
                Endpoint::RS
            } else {
                Endpoint::PM
            },
            bak_sig_mgr: Endpoint::NONE, // main.c:275
            io_ranges: [IoRange::default(); NR_IO_RANGE],
            nr_io_range: 0,
            mem_ranges: [MemRange::default(); NR_MEM_RANGE],
            nr_mem_range: 0,
            irqs: [0; NR_IRQ],
            nr_irq: 0,
        }
    }

    /// Is this a system process? C: `rp->r_priv.s_flags & SYS_PROC` (const.h:71).
    pub const fn is_sys_proc(&self) -> bool {
        self.flags.contains(PrivFlags::SYS_PROC)
    }

    /// Fail-closed count validation before the structure is handed to the
    /// kernel. C: `sys_privctl` rejects negative or over-limit counts with
    /// `EINVAL` (do_privctl.c:308-309, 319-320, 330-331); the 19 wiring calls
    /// this before `data_copy`. The C width is `int` (priv.h:53/56/59), so
    /// counts are `i32` here and validated at the boundary.
    pub fn validate(&self) -> Result<(), Errno> {
        let in_range = |v: i32, limit: usize| (0..=limit as i32).contains(&v);
        if !in_range(self.nr_io_range, NR_IO_RANGE)
            || !in_range(self.nr_mem_range, NR_MEM_RANGE)
            || !in_range(self.nr_irq, NR_IRQ)
        {
            return Err(Errno::EINVAL);
        }
        Ok(())
    }
}

// ── privctl operations (C: include/minix/com.h:342-353) ─────────────────────

/// `sys_privctl` operation codes.
///
/// Values align with `minix3/minix/include/minix/com.h:342-353`. RS uses
/// `SetSys`/`Allow`/`Disallow`/`SetUser`/`UpdateSys`/`Yield`/`ClearIpcRefs`;
/// the driver-facing `AddIo`/`AddMem`/`AddIrq`/`QueryMem` are retained for
/// message-layer completeness (03-rs-privilege.md §3.3) but are not called by
/// RS — minix-rs has no driver surface yet (ARCH A-10 defer, fail-closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivCtlOp {
    /// Allow process to run. C: `SYS_PRIV_ALLOW` — com.h:342.
    Allow = 1,
    /// Disallow process from running. C: `SYS_PRIV_DISALLOW` — com.h:343.
    Disallow = 2,
    /// Set a system privilege structure. C: `SYS_PRIV_SET_SYS` — com.h:344.
    SetSys = 3,
    /// Set a user privilege structure. C: `SYS_PRIV_SET_USER` — com.h:345.
    SetUser = 4,
    /// Add I/O range. C: `SYS_PRIV_ADD_IO` — com.h:346 (driver-facing, defer).
    AddIo = 5,
    /// Add memory range. C: `SYS_PRIV_ADD_MEM` — com.h:347 (driver-facing, defer).
    AddMem = 6,
    /// Add IRQ. C: `SYS_PRIV_ADD_IRQ` — com.h:349 (driver-facing, defer).
    AddIrq = 7,
    /// Verify memory privilege. C: `SYS_PRIV_QUERY_MEM` — com.h:350 (driver-facing, defer).
    QueryMem = 8,
    /// Update a sys privilege structure. C: `SYS_PRIV_UPDATE_SYS` — com.h:351.
    UpdateSys = 9,
    /// Allow process to run and suspend the caller. C: `SYS_PRIV_YIELD` — com.h:352.
    Yield = 10,
    /// Clear pending IPC for the process. C: `SYS_PRIV_CLEAR_IPC_REFS` — com.h:353.
    ClearIpcRefs = 11,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priv_flags_bit_values() {
        // C: include/minix/const.h:143-153.
        assert_eq!(PrivFlags::PREEMPTIBLE.bits(), 0x002);
        assert_eq!(PrivFlags::BILLABLE.bits(), 0x004);
        assert_eq!(PrivFlags::DYN_PRIV_ID.bits(), 0x008);
        assert_eq!(PrivFlags::SYS_PROC.bits(), 0x010);
        assert_eq!(PrivFlags::CHECK_IO_PORT.bits(), 0x020);
        assert_eq!(PrivFlags::CHECK_IRQ.bits(), 0x040);
        assert_eq!(PrivFlags::CHECK_MEM.bits(), 0x080);
        assert_eq!(PrivFlags::ROOT_SYS_PROC.bits(), 0x100);
        assert_eq!(PrivFlags::VM_SYS_PROC.bits(), 0x200);
        assert_eq!(PrivFlags::LU_SYS_PROC.bits(), 0x400);
        assert_eq!(PrivFlags::RST_SYS_PROC.bits(), 0x800);
    }

    #[test]
    fn test_priv_flag_presets() {
        // C: include/minix/priv.h:45-50.
        assert_eq!(SRV_F, PrivFlags::SYS_PROC | PrivFlags::PREEMPTIBLE);
        assert_eq!(DSRV_F, SRV_F | PrivFlags::DYN_PRIV_ID);
        assert_eq!(RSYS_F, SRV_F | PrivFlags::ROOT_SYS_PROC);
        assert_eq!(VM_F, PrivFlags::SYS_PROC | PrivFlags::VM_SYS_PROC);
        assert_eq!(USR_F, PrivFlags::BILLABLE | PrivFlags::PREEMPTIBLE);
        assert_eq!(
            IMM_F,
            PrivFlags::ROOT_SYS_PROC | PrivFlags::VM_SYS_PROC | PrivFlags::PREEMPTIBLE
        );
    }

    #[test]
    fn test_static_priv_id() {
        // C: static_priv_id(n) = NR_TASKS + (n) — priv.h:12; NR_TASKS=5 (com.h:56).
        assert_eq!(PrivId::static_priv_id(2).0, 7); // RS
        assert_eq!(PrivId::static_priv_id(11).0, 16); // INIT == USER_PRIV_ID (priv.h:18)
        assert_eq!(PrivId::NONE.0, -1); // NULL_PRIV_ID (priv.h:21)
    }

    #[test]
    fn test_sentinels() {
        assert_eq!(NO_M, -1);
        assert_eq!(ALL_M, -2);
        assert_eq!(NO_C, -1);
        assert_eq!(ALL_C, -2);
        assert_eq!(NULL_C, -3);
    }

    #[test]
    fn test_trap_mask() {
        // C: SRV_T=~0 (priv.h:61), USR_T=(1<<SENDREC)=0x8 (priv.h:63),
        //    CSK_T=(1<<RECEIVE)=0x4 (priv.h:60).
        assert_eq!(TrapMask::SRV_T, TrapMask::from_bits_truncate(0xFFFF));
        assert!(TrapMask::USR_T.contains(TrapMask::SENDREC));
        assert_eq!(TrapMask::USR_T.bits(), 0x8);
        assert_eq!(TrapMask::CSK_T, TrapMask::RECEIVE);
        assert_eq!(TrapMask::srv_or_usr(true), TrapMask::SRV_T);
        assert_eq!(TrapMask::srv_or_usr(false), TrapMask::USR_T);
    }

    #[test]
    fn test_call_mask_from_calls() {
        // ALL_C → full mask within the call space.
        let m = CallMask::from_calls(&[ALL_C, NULL_C], NR_SYS_CALLS, KERNEL_CALL, true).unwrap();
        assert_eq!(m.0, (1u64 << NR_SYS_CALLS) - 1);

        // Single call: bit (call - call_base).
        let m = CallMask::from_calls(&[KERNEL_CALL + 4, NULL_C], NR_SYS_CALLS, KERNEL_CALL, true)
            .unwrap();
        assert!(m.test_bit(4));
        assert!(!m.test_bit(3));

        // NULL_C terminates.
        let m = CallMask::from_calls(
            &[KERNEL_CALL + 1, NULL_C, KERNEL_CALL + 2],
            NR_SYS_CALLS,
            KERNEL_CALL,
            true,
        )
        .unwrap();
        assert!(m.test_bit(1));
        assert!(!m.test_bit(2));

        // N7: an out-of-range call number fails closed with EINVAL instead
        // of silently setting the wrong bit in release builds.
        assert_eq!(
            CallMask::from_calls(
                &[KERNEL_CALL + 200, NULL_C],
                NR_SYS_CALLS,
                KERNEL_CALL,
                true
            ),
            Err(Errno::EINVAL)
        );
        assert_eq!(
            CallMask::from_calls(&[KERNEL_CALL - 5, NULL_C], NR_SYS_CALLS, KERNEL_CALL, true),
            Err(Errno::EINVAL)
        );
        // A call space >= 64 bits must not shift-overflow (N7).
        assert_eq!(
            CallMask::from_calls(&[ALL_C, NULL_C], 64, KERNEL_CALL, true)
                .unwrap()
                .0,
            u64::MAX
        );
    }

    #[test]
    fn test_sys_map() {
        let m = SysMap::all();
        assert!(m.test(0));
        assert!(m.test(63));
        let m = SysMap::empty().set(7);
        assert!(m.test(7));
        assert!(!m.test(8));
    }

    #[test]
    fn test_srv_or_usr() {
        assert_eq!(srv_or_usr(true, 1u8, 2u8), 1);
        assert_eq!(srv_or_usr(false, 1u8, 2u8), 2);
    }

    #[test]
    fn test_privctl_op_discriminants() {
        // C: com.h:342-354.
        assert_eq!(PrivCtlOp::Allow as i32, 1);
        assert_eq!(PrivCtlOp::Disallow as i32, 2);
        assert_eq!(PrivCtlOp::SetSys as i32, 3);
        assert_eq!(PrivCtlOp::SetUser as i32, 4);
        assert_eq!(PrivCtlOp::AddIo as i32, 5);
        assert_eq!(PrivCtlOp::AddMem as i32, 6);
        assert_eq!(PrivCtlOp::AddIrq as i32, 7);
        assert_eq!(PrivCtlOp::QueryMem as i32, 8);
        assert_eq!(PrivCtlOp::UpdateSys as i32, 9);
        assert_eq!(PrivCtlOp::Yield as i32, 10);
        assert_eq!(PrivCtlOp::ClearIpcRefs as i32, 11);
    }

    #[test]
    fn test_boot_priv_sys_proc() {
        // System service (e.g. PM): SRV_* defaults — main.c:258-280.
        let p = Privilege::boot_priv(SRV_F, Endpoint::PM.slot());
        assert_eq!(p.id, PrivId::static_priv_id(0)); // NR_TASKS + 0 = 5
        assert!(p.is_sys_proc());
        assert_eq!(p.trap_mask, TrapMask::SRV_T);
        assert_eq!(p.ipc_to, SysMap::all());
        assert_eq!(p.sig_mgr, Endpoint::RS);
        assert_eq!(p.bak_sig_mgr, Endpoint::NONE);
        assert_eq!(p.k_call_mask.0, (1u64 << NR_SYS_CALLS) - 1);
    }

    #[test]
    fn test_boot_priv_user_proc() {
        // User process (INIT): USR_* defaults.
        let p = Privilege::boot_priv(USR_F, Endpoint::INIT.slot());
        assert_eq!(p.id, PrivId::static_priv_id(11)); // 16 == USER_PRIV_ID
        assert!(!p.is_sys_proc());
        assert_eq!(p.trap_mask, TrapMask::USR_T);
        assert_eq!(p.sig_mgr, Endpoint::PM);
    }

    #[test]
    fn test_validate_range_counts() {
        // C: do_privctl.c:308-309, 319-320, 330-331 — negative or over-limit
        // counts are rejected with EINVAL; the count is `int` (priv.h:53/56/59).
        let mut p = Privilege::vacant();
        assert_eq!(p.validate(), Ok(())); // zeroed counts are valid
        p.nr_io_range = NR_IO_RANGE as i32;
        assert_eq!(p.validate(), Ok(())); // exactly the table limit
        p.nr_io_range = NR_IO_RANGE as i32 + 1;
        assert_eq!(p.validate(), Err(Errno::EINVAL));
        p.nr_io_range = -1; // C int semantics: negative is rejected, not wrapped
        assert_eq!(p.validate(), Err(Errno::EINVAL));
        p.nr_io_range = 0;
        p.nr_mem_range = -1;
        assert_eq!(p.validate(), Err(Errno::EINVAL));
        p.nr_mem_range = 0;
        p.nr_irq = NR_IRQ as i32 + 5;
        assert_eq!(p.validate(), Err(Errno::EINVAL));
    }
}

//! Data-acquisition channels (04-is-data-acquisition.md §4.1).
//!
//! The five ways IS pulls debug data: `sys_getinfo` (kernel tables),
//! `sys_diagctl` (stack traces), the kernel-message channel (`[ARCH: A-3]`),
//! `getsysinfo` (peer-server tables), `vm_info` (address spaces). Each
//! channel is a trait; production implementations land with the
//! `minix-sys`/kernel wiring (forward references, the 01 section 3 pattern).
//!
//! Data outlets: C copies each table *into the caller*
//! (`sys_getinfo` stores at `endpt = SELF`; `getsysinfo` servers
//! `sys_datacopy` to the caller), so every fetch method carries a typed
//! output slice — one method per C `sys_get*`/`getsysinfo` shorthand, with
//! the slice length as the caller-declared capacity. There is no
//! request-code enum to select a method: the methods are the table of
//! contents, and requests IS never issues (GET_KENV, GET_MACHINE) are
//! structurally inexpressible.
//!
//! Error-surface rule (04 §2.6): acquisition failure is *recoverable* —
//! dumps warn and continue. Only transport failure (01: receive/send)
//! panics. Traits therefore return raw Minix status codes (`i32`,
//! `!= OK` as in C), not `Result`.

use crate::dump_ds::DsEntrySnap;
use crate::dump_kernel::{
    BootImageStruct, IrqHookStruct, KmessagesSnap, KinfoStruct, PrivInfoStruct, ProcInfoStruct,
};
use crate::dump_pm::MProcSnap;
use crate::dump_rs::{RprocSnap, RprocpubSnap};
use crate::dump_vfs::{DmapSnap, FProcSnap};
use crate::dump_vm::{VmRegionSnap, VmStatsSnap, VmUsageSnap};
use minix_sys::ipc::{DirectTrapTransport, IpcTransport};
use minix_sys::syscall::{
    perform_taskcall, sys_getinfo_into, DirectKernelCallTransport,
};

/// Total GET_KMESSAGES snapshot size (a mirror of the kernel's
/// `kmess::KMESS_SNAPSHOT_SIZE`: an 8-byte cursor header + a 10000-byte ring
/// body, sys_config.h:22). The kernel-side constant is `pub(crate)`, so it is
/// anchored locally on the same wire contract and locked by tests.
const KMESS_SNAPSHOT_SIZE: usize = 10008;
use minix_types::{
    DS_GETSYSINFO, Endpoint, Message, PM_GETSYSINFO, RS_GETSYSINFO, SI_DATA_STORE, SI_DMAP_TAB,
    SI_PROCPUB_TAB, SI_PROC_TAB, VFS_GETSYSINFO,
};

/// A `getsysinfo` table selector IS actually issues.
///
/// C: `SI_*` — `minix3/minix/include/minix/sysinfo.h:11-17`, restricted to
/// the four tables IS pulls (dmp_pm/fs/rs/ds.c). This enum is a wire-encoding
/// helper for the production transport (it renders `what` on the message),
/// not part of the seam signature — the seam is the typed methods of
/// [`GetSysinfoTransport`]. MIB-only tables (`SI_CALL_STATS` etc.) have no
/// variant: out-of-scope values are inexpressible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiWhat {
    ProcTab,
    DmapTab,
    ProcPubTab,
    DataStore,
}

impl SiWhat {
    /// Wire `what` value. C: sysinfo.h:12-16.
    pub const fn code(self) -> i32 {
        match self {
            SiWhat::ProcTab => SI_PROC_TAB,
            SiWhat::DmapTab => SI_DMAP_TAB,
            SiWhat::ProcPubTab => SI_PROCPUB_TAB,
            SiWhat::DataStore => SI_DATA_STORE,
        }
    }
}

/// Every `(server, table)` pull IS performs.
///
/// C call sites: dmp_pm.c:47/82, dmp_fs.c:31/71, dmp_rs.c:33-34,
/// dmp_ds.c:15. Note the trap this table exists to name: `SI_PROC_TAB`
/// is pulled from both PM and RS — the owner is a property of the call
/// site, not of the table, so no `SiWhat::owner()` helper exists
/// (a single-owner mapping would misroute the RS leg). The production
/// transport consults this table (via [`getsysinfo_call`]) inside each
/// typed [`GetSysinfoTransport`] method.
pub const IS_GETSYSINFO_CALLS: &[(Endpoint, SiWhat)] = &[
    (Endpoint::PM, SiWhat::ProcTab),
    (Endpoint::VFS, SiWhat::ProcTab),
    (Endpoint::VFS, SiWhat::DmapTab),
    (Endpoint::RS, SiWhat::ProcPubTab),
    (Endpoint::RS, SiWhat::ProcTab),
    (Endpoint::DS, SiWhat::DataStore),
];

/// Kernel-table channel: one typed method per C `sys_get*` shorthand.
///
/// C: `sys_getinfo(request, ptr, ...)` — `minix3/minix/lib/libsys/sys_getinfo.c`
/// (`_kernel_call(SYS_GETINFO)`; `endpt = SELF`, i.e. the kernel always
/// stores at the caller). Each method mirrors a shorthand from
/// `syslib.h:175-187` (`sys_getproctab(dst)` ↔ [`SysGetinfoTransport::get_proctab`]);
/// the out-slice is the caller-owned destination the kernel fills, its
/// length the declared capacity. Requests IS never issues have no method:
/// `sys_getkenv` (kenv_dmp reads kinfo only — dmp_kernel.c:197-211) and
/// `sys_getmachine` (fetched but never read in C — 05 §2.6 exclusion) are
/// structurally absent.
pub trait SysGetinfoTransport {
    /// Kernel info structure. C: `sys_getkinfo` — dmp_kernel.c:197.
    fn get_kinfo(&mut self, out: &mut KinfoStruct) -> i32;
    /// Boot image table. C: `sys_getimage` — dmp_kernel.c:174.
    fn get_image(&mut self, out: &mut [BootImageStruct]) -> i32;
    /// Kernel process table. C: `sys_getproctab` — dmp_kernel.c:265/328/368,
    /// dmp_vm.c:83.
    fn get_proctab(&mut self, out: &mut [ProcInfoStruct]) -> i32;
    /// Boot monitor parameters (NUL-separated string blob).
    /// C: `sys_getmonparams` — dmp_kernel.c:101.
    fn get_monparams(&mut self, out: &mut [u8]) -> i32;
    /// IRQ hook table. C: `sys_getirqhooks` — dmp_kernel.c:129.
    fn get_irqhooks(&mut self, out: &mut [IrqHookStruct]) -> i32;
    /// IRQ mask table. C: `sys_getirqactids` — dmp_kernel.c:133.
    fn get_irqactids(&mut self, out: &mut [i32]) -> i32;
    /// Privilege table. C: `sys_getprivtab` — dmp_kernel.c:261.
    fn get_privtab(&mut self, out: &mut [PrivInfoStruct]) -> i32;
}

/// `sys_diagctl` stack-trace channel.
///
/// C: `sys_diagctl_stacktrace(ep)` = `sys_diagctl(DIAGCTL_CODE_STACKTRACE,
/// NULL, ep)` — syslib.h:166-168 → sys_diagctl.c (`endpt` reuses `arg2`).
/// Kernel side is currently ENOSYS (32-stack-tracing.md forward ref);
/// callers treat failure as warn-and-continue (04 §2.6). No outlet by
/// design: the trace prints into the kernel log and returns no buffer.
pub trait DiagctlTransport {
    fn stacktrace(&mut self, proc: Endpoint) -> i32;
}

/// Kernel-message channel (`[ARCH: A-3]`).
///
/// C chain: libc constructor `__minix_init` → `ipc_minix_kerninfo()` →
/// usermapped page, magic-checked (`minix3/minix/lib/libc/sys/init.c:22-27`)
/// → `get_minix_kerninfo()->kmessages` ring read (dmp_kernel.c:71).
/// minix-rs 64-bit does not port `.usermapped`
/// (`28-usermapped-data.md`); the production implementation is the
/// `GET_KMESSAGES` `sys_getinfo` sub-request (kernel edge E-ISKMESS): the
/// kernel copies the raw ring into `ring` and the cursor fields into
/// `meta`, and IS computes the print start via `kmess_start` (05 section 3).
/// The `DIAGCTL_CODE_STACKTRACE` channel is NOT reused: it prints into the
/// kernel log and returns no buffer (different semantics).
pub trait KerninfoTransport {
    /// Kernel-message ring snapshot. C: `kmessages_dmp` read face —
    /// dmp_kernel.c:63-93.
    fn kmessages(&mut self, meta: &mut KmessagesSnap, ring: &mut [u8]) -> i32;
}

/// Cross-service table channel: one typed method per `(server, table)` pair.
///
/// C: `getsysinfo(who, what, where, size)` — `minix3/minix/lib/libsys/getsysinfo.c`:
/// `who`→call-number map (PM/VFS/RS/DS, else `ENOSYS`), then `_taskcall`.
/// Server side (`pm/misc.c:105-145`, `vfs/misc.c:61-113`): root gate
/// (`EPERM`) + exact `len == size` match (`EINVAL`) + `sys_datacopy`.
/// IS-side obligations (04 section 3): always root (IS uid 0), exact
/// `size_of` lengths (the out-slice length), in-scope tables only. The
/// RS double pull is one all-or-nothing method (dmp_rs.c:33-34).
pub trait GetSysinfoTransport {
    /// PM process table. C: dmp_pm.c:47/82 (`getsysinfo(PM, SI_PROC_TAB)`).
    fn pm_proc_tab(&mut self, out: &mut [MProcSnap]) -> i32;
    /// VFS process table. C: dmp_fs.c:31.
    fn vfs_proc_tab(&mut self, out: &mut [FProcSnap]) -> i32;
    /// VFS device/driver table. C: dmp_fs.c:71.
    fn vfs_dmap_tab(&mut self, out: &mut [DmapSnap]) -> i32;
    /// RS public + private tables, all-or-nothing. C: dmp_rs.c:33-34.
    fn rs_tables(
        &mut self,
        pub_out: &mut [RprocpubSnap],
        priv_out: &mut [RprocSnap],
    ) -> i32;
    /// Data-store contents. C: dmp_ds.c:15 (consumer half of DS A-10).
    fn ds_data_store(&mut self, out: &mut [DsEntrySnap]) -> i32;
}

/// Maps a server to its `getsysinfo` call number.
///
/// C: the `switch (who)` — getsysinfo.c:14-24 (`PM_GETSYSINFO` callnr.h:60,
/// `VFS_GETSYSINFO` callnr.h:120, `RS_GETSYSINFO` com.h:476,
/// `DS_GETSYSINFO` com.h:507; `default: return ENOSYS`). Wire helper for
/// the production transport, not a seam signature.
pub const fn getsysinfo_call(who: Endpoint) -> i32 {
    match who {
        Endpoint::PM => PM_GETSYSINFO,
        Endpoint::VFS => VFS_GETSYSINFO,
        Endpoint::RS => RS_GETSYSINFO,
        Endpoint::DS => DS_GETSYSINFO,
        _ => minix_types::ENOSYS,
    }
}

/// VM address-space channel.
///
/// C: `vm_info_stats/usage/region` — `minix3/minix/lib/libsys/vm_info.c`
/// (`_taskcall(VM_PROC_NR, VM_INFO)`; `VMIW_STATS/USAGE/REGION` —
/// com.h:729-734, re-exported from `minix-types::ipc::vm`). `region`
/// takes the caller's batch window as the slice and returns
/// `(status, next_out, count_out)`: the `next` cursor writeback +
/// actual count (vm_info.c:39-58); paging over the cursor belongs to 10
/// (`prev_base`/`prev_i` — dmp_vm.c:61-62,88-94).
pub trait VmInfoTransport {
    /// System memory statistics. C: `vm_info_stats` — dmp_vm.c:66.
    fn vm_stats(&mut self, out: &mut VmStatsSnap) -> i32;
    /// One process's usage. C: `vm_info_usage` — dmp_vm.c:110.
    fn vm_usage(&mut self, who: Endpoint, out: &mut VmUsageSnap) -> i32;
    /// One region batch. C: `vm_info_region` — dmp_vm.c:94/131.
    fn vm_region(
        &mut self,
        who: Endpoint,
        out: &mut [VmRegionSnap],
        next: u64,
    ) -> (i32, u64, i32);
}

/// Kernel clock channel.
///
/// C: `getticks()` — `minix3/minix/lib/libsys/getticks.c` reads
/// `kerninfo->kclockinfo->uptime` (ticks since boot, wraps on overflow).
/// minix-rs builds no usermapped page; the production transport routes
/// the equivalent kernel call (SYS_TIMES family). Consumer: 06
/// (sigaction's alarm countdown, dmp_pm.c:78/101-102).
pub trait ClockTransport {
    /// Ticks since boot. C: `getticks()` — getticks.c:8-12.
    fn uptime(&mut self) -> u32;
}

/// The five channels bundled for the orchestrator.
///
/// The per-channel traits stay fine-grained (04 section 3: each dump domain
/// consumes a subset), but `IsServer` — the only component that talks to
/// all of them — holds one value behind this supertrait (RS `KernelApi`
/// five-domain precedent, 03-stage-rs). Dump bodies never take this
/// bundle: they are free functions over already-fetched snapshots.
pub trait Acquires:
    SysGetinfoTransport
    + DiagctlTransport
    + KerninfoTransport
    + GetSysinfoTransport
    + VmInfoTransport
    + ClockTransport
{
}

impl<T> Acquires for T where
    T: SysGetinfoTransport
        + DiagctlTransport
        + KerninfoTransport
        + GetSysinfoTransport
        + VmInfoTransport
        + ClockTransport
{
}

/// The production acquisition face: the SYS_GETINFO family called directly
/// (the structs are minix-types' wire authority, and their repr(C) layout goes
/// to the kernel byte for byte), SYS_DIAGCTL's stacktrace (code 2, payload
/// endpoint), SYS_TIMES' uptime (`real_ticks`, the same quantity C's `getticks`
/// reads from kclockinfo->uptime; this tree builds no usermapped page), and the
/// `GET_KMESSAGES` snapshot ([`KernelKmessTransport`]). The five `GetSysinfo`
/// legs go through [`SysGetsysinfo`] and the three `VM_INFO` queries through
/// [`SysVmInfo`] -- all six channels are in place, with no fail-closed
/// placeholder.
#[derive(Debug, Default)]
pub struct SysAcquires {
    /// Production client for the five getsysinfo legs.
    getsys: SysGetsysinfo,
}

impl SysAcquires {
    /// The shared carrier for SYS_GETINFO: `Ok` -> OK(0), `Err(e)` -> the
    /// negative errno as-is (the face where C tests `r != OK`).
    fn getinfo(request: i32, buf: &mut [u8], endpt: i32) -> i32 {
        match minix_sys::syscall::sys_getinfo_into(
            &DirectKernelCallTransport,
            request,
            buf,
            endpt,
        ) {
            Ok(()) => minix_types::OK,
            Err(e) => e,
        }
    }
}

/// Byte view of a struct slice (the output buffer; the structs are all repr(C)
/// wire authority).
///
/// # Safety
/// `T` must be POD (repr(C), no padding semantics relied upon); the slice is
/// exclusive for the duration of the call.
unsafe fn out_bytes<T>(v: &mut [T]) -> &mut [u8] {
    // SAFETY: the caller guarantees a POD slice -> byte view.
    unsafe {
        core::slice::from_raw_parts_mut(
            v.as_mut_ptr().cast::<u8>(),
            core::mem::size_of_val(v),
        )
    }
}

impl SysGetinfoTransport for SysAcquires {
    fn get_kinfo(&mut self, out: &mut KinfoStruct) -> i32 {
        Self::getinfo(
            minix_types::GET_KINFO,
            unsafe { out_bytes(core::slice::from_mut(out)) },
            Endpoint::NONE.0,
        )
    }

    fn get_image(&mut self, out: &mut [BootImageStruct]) -> i32 {
        Self::getinfo(minix_types::GET_IMAGE, unsafe { out_bytes(out) }, Endpoint::NONE.0)
    }

    fn get_proctab(&mut self, out: &mut [ProcInfoStruct]) -> i32 {
        Self::getinfo(minix_types::GET_PROCTAB, unsafe { out_bytes(out) }, Endpoint::NONE.0)
    }

    fn get_monparams(&mut self, out: &mut [u8]) -> i32 {
        Self::getinfo(minix_types::GET_MONPARAMS, out, Endpoint::NONE.0)
    }

    fn get_irqhooks(&mut self, out: &mut [IrqHookStruct]) -> i32 {
        Self::getinfo(minix_types::GET_IRQHOOKS, unsafe { out_bytes(out) }, Endpoint::NONE.0)
    }

    fn get_irqactids(&mut self, out: &mut [i32]) -> i32 {
        Self::getinfo(minix_types::GET_IRQACTIDS, unsafe { out_bytes(out) }, Endpoint::NONE.0)
    }

    fn get_privtab(&mut self, out: &mut [PrivInfoStruct]) -> i32 {
        Self::getinfo(minix_types::GET_PRIVTAB, unsafe { out_bytes(out) }, Endpoint::NONE.0)
    }
}

impl DiagctlTransport for SysAcquires {
    fn stacktrace(&mut self, proc: Endpoint) -> i32 {
        // C: sys_diagctl_stacktrace(ep) (dmp_kernel.c:378) -> the SYS_DIAGCTL
        // code 2 payload endpoint lands in `d.endpt` (minix-sys writes arg2 on
        // the non-code1 branch).
        match minix_sys::syscall::sys_diagctl(
            &DirectKernelCallTransport,
            2, // DIAGCTL_CODE_STACKTRACE — com.h:413
            0,
            proc.0,
        ) {
            Ok(()) => minix_types::OK,
            Err(e) => e,
        }
    }
}

impl ClockTransport for SysAcquires {
    fn uptime(&mut self) -> u32 {
        // C getticks(): kclockinfo->uptime (the usermapped page, which this
        // tree does not build). SYS_TIMES' real_ticks is the same quantity
        // (real ticks since boot); on failure return 0 (C's call site has no
        // failure face).
        match minix_sys::syscall::sys_times(&DirectKernelCallTransport, Endpoint::SELF.0) {
            Ok(t) => t.real_ticks as u32,
            Err(_) => 0,
        }
    }
}

impl KerninfoTransport for SysAcquires {
    fn kmessages(&mut self, meta: &mut KmessagesSnap, ring: &mut [u8]) -> i32 {
        // The `GET_KMESSAGES` snapshot channel is really wired -- the 10008-byte
        // snapshot (an 8-byte cursor header + a 10000-byte ring body) is fetched
        // once via `sys_getinfo` and then unpacked; hosted builds honestly
        // surface -EIO (the kernel's `kmess` arm is only reachable under
        // real-trap).
        KernelKmessTransport.kmessages(meta, ring)
    }
}

impl GetSysinfoTransport for SysAcquires {
    fn pm_proc_tab(&mut self, out: &mut [MProcSnap]) -> i32 {
        self.getsys.pm_proc_tab(out)
    }

    fn vfs_proc_tab(&mut self, out: &mut [FProcSnap]) -> i32 {
        self.getsys.vfs_proc_tab(out)
    }

    fn vfs_dmap_tab(&mut self, out: &mut [DmapSnap]) -> i32 {
        self.getsys.vfs_dmap_tab(out)
    }

    fn rs_tables(&mut self, pub_out: &mut [RprocpubSnap], priv_out: &mut [RprocSnap]) -> i32 {
        self.getsys.rs_tables(pub_out, priv_out)
    }

    fn ds_data_store(&mut self, out: &mut [DsEntrySnap]) -> i32 {
        self.getsys.ds_data_store(out)
    }
}

impl VmInfoTransport for SysAcquires {
    fn vm_stats(&mut self, out: &mut VmStatsSnap) -> i32 {
        SysVmInfo::vm_stats_via(&DirectTrapTransport, out)
    }

    fn vm_usage(&mut self, who: Endpoint, out: &mut VmUsageSnap) -> i32 {
        SysVmInfo::vm_usage_via(&DirectTrapTransport, who, out)
    }

    fn vm_region(&mut self, who: Endpoint, out: &mut [VmRegionSnap], next: u64) -> (i32, u64, i32) {
        SysVmInfo::vm_region_via(&DirectTrapTransport, who, out, next)
    }
}

/// Shared test double for orchestrator-level tests (lib.rs): fills
/// outlets with recognisable values when the scripted status is OK, so
/// dump-body tests assert rendered content, not just statuses.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use minix_types::OK;

/// Scriptable fake for all five channels: fills outputs with
/// recognisable values when the scripted status is OK, so body tests
/// can assert on rendered content, not just statuses.
pub(crate) struct FakeAcquires {
    pub(crate) getinfo_status: i32,
    pub(crate) diag_status: i32,
    pub(crate) kmessages_status: i32,
    pub(crate) getsys_status: i32,
    pub(crate) vm_status: i32,
    pub(crate) region_answer: (i32, u64, i32),
    pub(crate) uptime_ticks: u32,
    pub(crate) seen: Vec<&'static str>,
}

impl FakeAcquires {
    pub(crate) fn ok() -> Self {
        Self {
            getinfo_status: OK,
            diag_status: OK,
            kmessages_status: OK,
            getsys_status: OK,
            vm_status: OK,
            region_answer: (OK, 0, 3),
            uptime_ticks: 1000,
            seen: Vec::new(),
        }
    }

}

impl SysGetinfoTransport for FakeAcquires {
    fn get_kinfo(&mut self, out: &mut KinfoStruct) -> i32 {
        self.seen.push("kinfo");
        if self.getinfo_status == OK {
            *out = KinfoStruct::default();
        }
        self.getinfo_status
    }

    fn get_image(&mut self, out: &mut [BootImageStruct]) -> i32 {
        self.seen.push("image");
        if self.getinfo_status == OK {
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = BootImageStruct { proc_nr: i as i32, ..Default::default() };
            }
        }
        self.getinfo_status
    }

    fn get_proctab(&mut self, out: &mut [ProcInfoStruct]) -> i32 {
        self.seen.push("proctab");
        if self.getinfo_status == OK {
            for (i, slot) in out.iter_mut().enumerate() {
                slot.p_nr = i as i32;
            }
        }
        self.getinfo_status
    }

    fn get_monparams(&mut self, out: &mut [u8]) -> i32 {
        self.seen.push("monparams");
        if self.getinfo_status == OK && out.len() >= 3 {
            out[..3].copy_from_slice(b"ab\n");
        }
        self.getinfo_status
    }

    fn get_irqhooks(&mut self, out: &mut [IrqHookStruct]) -> i32 {
        self.seen.push("irqhooks");
        if self.getinfo_status == OK {
            for slot in out.iter_mut() {
                *slot = IrqHookStruct::default();
            }
        }
        self.getinfo_status
    }

    fn get_irqactids(&mut self, out: &mut [i32]) -> i32 {
        self.seen.push("irqactids");
        if self.getinfo_status == OK {
            for slot in out.iter_mut() {
                *slot = 0;
            }
        }
        self.getinfo_status
    }

    fn get_privtab(&mut self, out: &mut [PrivInfoStruct]) -> i32 {
        self.seen.push("privtab");
        if self.getinfo_status == OK {
            for slot in out.iter_mut() {
                *slot = PrivInfoStruct::default();
            }
        }
        self.getinfo_status
    }
}

impl DiagctlTransport for FakeAcquires {
    fn stacktrace(&mut self, _proc: Endpoint) -> i32 {
        self.seen.push("stacktrace");
        self.diag_status
    }
}


impl KerninfoTransport for FakeAcquires {
    fn kmessages(&mut self, meta: &mut KmessagesSnap, ring: &mut [u8]) -> i32 {
        self.seen.push("kmessages");
        if self.kmessages_status == OK {
            *meta = KmessagesSnap { km_next: 0, km_size: 0 };
            for b in ring.iter_mut() {
                *b = 0;
            }
        }
        self.kmessages_status
    }
}

impl GetSysinfoTransport for FakeAcquires {
    fn pm_proc_tab(&mut self, out: &mut [MProcSnap]) -> i32 {
        self.seen.push("pm_proc_tab");
        if self.getsys_status == OK {
            for (i, slot) in out.iter_mut().enumerate() {
                slot.mp_pid = i as i32;
            }
        }
        self.getsys_status
    }

    fn vfs_proc_tab(&mut self, out: &mut [FProcSnap]) -> i32 {
        self.seen.push("vfs_proc_tab");
        if self.getsys_status == OK {
            for (i, slot) in out.iter_mut().enumerate() {
                slot.fp_pid = i as i32;
            }
        }
        self.getsys_status
    }

    fn vfs_dmap_tab(&mut self, out: &mut [DmapSnap]) -> i32 {
        self.seen.push("vfs_dmap_tab");
        if self.getsys_status == OK {
            for slot in out.iter_mut() {
                *slot = DmapSnap::default();
            }
        }
        self.getsys_status
    }

    fn rs_tables(
        &mut self,
        pub_out: &mut [RprocpubSnap],
        priv_out: &mut [RprocSnap],
    ) -> i32 {
        self.seen.push("rs_tables");
        if self.getsys_status == OK {
            for slot in pub_out.iter_mut() {
                *slot = RprocpubSnap::default();
            }
            for slot in priv_out.iter_mut() {
                *slot = RprocSnap::default();
            }
        }
        self.getsys_status
    }

    fn ds_data_store(&mut self, out: &mut [DsEntrySnap]) -> i32 {
        self.seen.push("ds_data_store");
        if self.getsys_status == OK {
            for slot in out.iter_mut() {
                *slot = DsEntrySnap::default();
            }
        }
        self.getsys_status
    }
}

impl crate::acquire::ClockTransport for FakeAcquires {
    fn uptime(&mut self) -> u32 {
        self.seen.push("uptime");
        self.uptime_ticks
    }
}

impl VmInfoTransport for FakeAcquires {
    fn vm_stats(&mut self, out: &mut VmStatsSnap) -> i32 {
        self.seen.push("vm_stats");
        if self.vm_status == OK {
            *out = VmStatsSnap::default();
        }
        self.vm_status
    }

    fn vm_usage(&mut self, _who: Endpoint, out: &mut VmUsageSnap) -> i32 {
        self.seen.push("vm_usage");
        if self.vm_status == OK {
            *out = VmUsageSnap::default();
        }
        self.vm_status
    }

    fn vm_region(
        &mut self,
        _who: Endpoint,
        out: &mut [VmRegionSnap],
        _next: u64,
    ) -> (i32, u64, i32) {
        self.seen.push("vm_region");
        if self.region_answer.0 == OK {
            for slot in out.iter_mut() {
                *slot = VmRegionSnap::default();
            }
        }
        let count = self.region_answer.2.min(out.len() as i32);
        (self.region_answer.0, self.region_answer.1, count)
    }
}
}
/// Production `KerninfoTransport`: pulls the kernel ring-buffer snapshot
/// through `sys_getinfo`'s `GET_KMESSAGES` sub-request (E-ISKMESS;
/// minix-types `sysinfo::GET_KMESSAGES` = 7) and unpacks it.
///
/// Snapshot wire shape (kernel `kmess::KMESS_SNAPSHOT_SIZE` = 10008):
/// `km_next` (i32) @0, `km_size` (i32) @4, then the ring body of 10000 bytes
/// laid out in order. After unpacking, `meta` carries the cursor and `ring`
/// carries the prefix of the ring body that fits in the caller's buffer.
///
/// In hosted builds (without `real-trap`) the transport honestly returns
/// `-EIO`, and this implementation surfaces it unchanged -- the caller
/// handles it with the existing fail-closed semantics, the same gated shape as
/// `minix-rt::DirectTrapSource`.
pub struct KernelKmessTransport;

impl KerninfoTransport for KernelKmessTransport {
    fn kmessages(&mut self, meta: &mut KmessagesSnap, ring: &mut [u8]) -> i32 {
        // A fixed-capacity 10008-byte snapshot buffer (the kernel arm copies it
        // out in one block), allocated on the stack -- the IS user stack is
        // 64 KiB (loader stack), the snapshot takes ~15%, so there is ample
        // headroom; C's kmessages_dmp print_buf is a buffer of the same order
        // (dmp_kernel.c:74).
        let mut snap = [0u8; KMESS_SNAPSHOT_SIZE];
        match sys_getinfo_into(
            &DirectKernelCallTransport,
            minix_types::GET_KMESSAGES,
            &mut snap,
            Endpoint::NONE.get(),
        ) {
            Ok(()) => {
                split_kmess_snapshot(&snap, meta, ring);
                minix_types::OK
            }
            Err(code) => code,
        }
    }
}

/// Split a GET_KMESSAGES snapshot: the 8-byte header (`km_next`/`km_size`, LE
/// i32) goes into `meta`, and the ring body fills the prefix that fits in
/// `ring` (truncated when the caller's buffer is shorter than 10000 --
/// `kmessages_dmp`'s print assembly reorders by the `km_size` cursor
/// independently of that capacity).
fn split_kmess_snapshot(snap: &[u8], meta: &mut KmessagesSnap, ring: &mut [u8]) {
    meta.km_next = i32::from_le_bytes([snap[0], snap[1], snap[2], snap[3]]);
    meta.km_size = i32::from_le_bytes([snap[4], snap[5], snap[6], snap[7]]);
    let body = &snap[8..];
    let n = body.len().min(ring.len());
    ring[..n].copy_from_slice(&body[..n]);
}

/// Production `GetSysinfoTransport`: all five legs ride the message shape
/// `getsysinfo(who, what, where, size)`.
///
/// C: `minix3/minix/lib/libsys/getsysinfo.c:14-31` -- `who` selects the call
/// number (PM/VFS/RS/DS, `ENOSYS` otherwise), then
/// `_taskcall(who, call_nr, &m)`; `m_lsys_getsysinfo`'s three fields are "what
/// to fetch" (`what`), "where to put it" (the caller's virtual address
/// `where`), and "how long" (`size`). The server side (PM `misc.c:108-144`,
/// VFS `misc.c:59-113`, RS `request.c:1095-1142`, DS `store.c:653-678`)
/// applies a root gate + **exact size match** (one byte off is `EINVAL`) +
/// `sys_datacopy` back to the caller -- so the `size` the caller declares must
/// equal the output slice's byte length, and `what` must be sent with the C
/// value (`minix_types::SI_*`).
///
/// All five legs use the shared snapshot types as their row width
/// (`[ARCH: A-4]`, single authority): the PM leg `MProcSnap` (aligned), the VFS
/// leg `FProcSnap` (aligned under E-MIBPROD), and the **producer** alignment of
/// the RS pair and the DS table still belongs to E-MIBPROD/E-ISPROD's remaining
/// items -- before alignment the server answers `EINVAL` at the exact-size
/// gate, and the dump body prints one error line and continues with C's
/// warn-and-continue (04 section 2.6), neither panicking nor fabricating data.
///
/// Each leg has a `*_via(transport, …)` shape: production passes
/// [`DirectTrapTransport`], tests pass a scripted double (in hosted builds the
/// real trap honestly surfaces `-EIO`).
#[derive(Debug, Default)]
pub struct SysGetsysinfo;

impl SysGetsysinfo {
    /// The shared carrier for one leg.
    ///
    /// The message is packed into three raw lanes (what @0..4, where @8..16,
    /// size @16..24, byte-identical to `MessLsysGetsysinfo`'s LP64 layout) and
    /// sent through the shared [`minix_sys::syscall::perform_taskcall`]:
    /// transport-level failures come back as `Err(Errno)` (the TrapStatus sign
    /// contract -- both the target's reply registers and the host fallback arm
    /// report a positive errno), a successful sendrec leaves the raw reply's
    /// `m_type` in `Ok`, and a negative value is the server's error code (C
    /// `_taskcall` semantics).
    ///
    /// `where` is **the caller's own buffer address** (the server fills it back
    /// SELF->caller via `sys_datacopy`), so `out` must stay exclusive for the
    /// duration of the call.
    fn fetch<Row>(
        transport: &impl IpcTransport,
        who: Endpoint,
        what: i32,
        out: &mut [Row],
    ) -> Result<i32, minix_types::Errno> {
        let mut msg = Message::default();
        // SAFETY: the raw three-lane view of m_lsys_getsysinfo (C
        // getsysinfo.c:27-29's what/where/size).
        unsafe {
            msg.m_u.raw[..4].copy_from_slice(&what.to_ne_bytes());
            msg.m_u.raw[8..16].copy_from_slice(&(out.as_mut_ptr() as u64).to_ne_bytes());
            msg.m_u.raw[16..24]
                .copy_from_slice(&(core::mem::size_of_val(out) as u64).to_ne_bytes());
        }
        perform_taskcall(transport, who, getsysinfo_call(who), &mut msg)
    }

    /// The outward shape of one leg: a transport-level `Err` folds back to a
    /// negative errno, merging with the shape where sendrec succeeds but the
    /// server answers a negative `m_type` (the trait's C integer contract).
    fn leg<Row>(transport: &impl IpcTransport, who: Endpoint, what: i32, out: &mut [Row]) -> i32 {
        match Self::fetch(transport, who, what, out) {
            Ok(reply) => reply,
            Err(e) => -e.to_i32(),
        }
    }

    /// The PM process-table leg. C: `dmp_pm.c:47/82`.
    pub fn pm_proc_tab_via(transport: &impl IpcTransport, out: &mut [MProcSnap]) -> i32 {
        Self::leg(transport, Endpoint::PM, SI_PROC_TAB, out)
    }

    /// The RS double-pull leg. C: `dmp_rs.c:33-34` -- PUB first, then PRIV, and
    /// a PUB failure short-circuits (`||` semantics: without PUB there is no
    /// identity column to reconcile against, so pulling PRIV afterwards is
    /// pointless).
    pub fn rs_tables_via(
        transport: &impl IpcTransport,
        pub_out: &mut [RprocpubSnap],
        priv_out: &mut [RprocSnap],
    ) -> i32 {
        let pub_status = Self::leg(transport, Endpoint::RS, SI_PROCPUB_TAB, pub_out);
        if pub_status != minix_types::OK {
            return pub_status;
        }
        Self::leg(transport, Endpoint::RS, SI_PROC_TAB, priv_out)
    }

    /// The DS data-store leg. C: `dmp_ds.c:15`.
    pub fn ds_data_store_via(transport: &impl IpcTransport, out: &mut [DsEntrySnap]) -> i32 {
        Self::leg(transport, Endpoint::DS, SI_DATA_STORE, out)
    }

    /// The VFS fproc_tab leg. C: `dmp_fs.c:31`.
    ///
    /// VFS's runtime answer face is in place (`dispatch_syscall`'s `Getsysinfo`
    /// arm plus the `send_reply` at the tail of the `run()` loop), so this leg
    /// sends and receives normally.
    pub fn vfs_proc_tab_via(transport: &impl IpcTransport, out: &mut [FProcSnap]) -> i32 {
        Self::leg(transport, Endpoint::VFS, SI_PROC_TAB, out)
    }

    /// The VFS dmap_tab leg. C: `dmp_fs.c:71`.
    pub fn vfs_dmap_tab_via(transport: &impl IpcTransport, out: &mut [DmapSnap]) -> i32 {
        Self::leg(transport, Endpoint::VFS, SI_DMAP_TAB, out)
    }
}

impl GetSysinfoTransport for SysGetsysinfo {
    fn pm_proc_tab(&mut self, out: &mut [MProcSnap]) -> i32 {
        Self::pm_proc_tab_via(&DirectTrapTransport, out)
    }

    fn vfs_proc_tab(&mut self, out: &mut [FProcSnap]) -> i32 {
        Self::vfs_proc_tab_via(&DirectTrapTransport, out)
    }

    fn vfs_dmap_tab(&mut self, out: &mut [DmapSnap]) -> i32 {
        Self::vfs_dmap_tab_via(&DirectTrapTransport, out)
    }

    fn rs_tables(&mut self, pub_out: &mut [RprocpubSnap], priv_out: &mut [RprocSnap]) -> i32 {
        Self::rs_tables_via(&DirectTrapTransport, pub_out, priv_out)
    }

    fn ds_data_store(&mut self, out: &mut [DsEntrySnap]) -> i32 {
        Self::ds_data_store_via(&DirectTrapTransport, out)
    }
}

/// Production `VmInfoTransport`: the **value-channel** client for the three
/// VM_INFO queries.
///
/// C: `minix3/minix/lib/libsys/vm_info.c:10-57` -- `_taskcall(VM_PROC_NR,
/// VM_INFO, &m)`, with request fields `what`/`ep`/`count`/`ptr`/`next`; C's
/// server copies the results into the caller's buffer with `sys_datacopy`
/// (`minix3/minix/servers/vm/utility.c:169-182`, pinning the target page with
/// `handle_memory_once` first to avoid deadlock). minix-rs's VM replaces that
/// step with a **value channel** (`[ARCH: 26-D1]`,
/// `02-stage-vm/26-vm-queries.md`): results are encoded in the reply message's
/// m1 slots and `ptr` does not participate (so this client sends 0).
///
/// Slot mapping (producer: `encode_reply_data` in
/// `os/servers/vm/src/ipc/encode.rs`):
/// - STATS: `m1p1` = page size, `m1i1` = total pages, `m1i2` = free pages,
///   `m1i3` = largest contiguous block, `m1p2` = cached pages;
/// - USAGE: `m1p1`/`m1p2`/`m1p3` = total/common/shared (**bytes**, i.e. C
///   `struct vm_usage_info`'s `vui_total`/`vui_common`/`vui_shared`);
/// - REGION: `m1i1` = actual entry count, `m1i2` = new cursor (low 32 bits --
///   region addresses live in the low 4 GiB user range, so C's `vir_bytes`
///   cursor loses no precision here).
///
/// REGION's entry array is inlined in the reply in **value-channel batches**
/// (`[ARCH: 26-D7]`, [`SysVmInfo::VRI_PER_REPLY`] entries per packet, a
/// compact 20-byte layout), and [`SysVmInfo::vm_region_via`] stitches the
/// batches for the caller -- C's "one call moves one batch" semantics are
/// unchanged; only the number of round trips depends on the packet size.
#[derive(Debug, Default)]
pub struct SysVmInfo;

impl SysVmInfo {
    /// The request wrapper shared by the three queries: fill
    /// `what`/`ep`/`count`/`next`, then taskcall.
    ///
    /// The reply lands on the same `msg` (`_taskcall` reuses the message buffer,
    /// as in C), so the caller reads `msg.m_u.m_m1` for the result slots.
    fn call(
        transport: &impl IpcTransport,
        what: i32,
        ep: Endpoint,
        count: i32,
        next: u64,
    ) -> Result<(i32, Message), minix_types::Errno> {
        let mut msg = Message::default();
        // `m_lsys_vm_info` is VM_INFO's payload domain (C ipc.h:1494-1502's
        // what/ep/count/ptr/next; `ptr` does not participate under minix-rs's
        // value channel) -- that union member is a plain POD field, so a whole
        // assignment needs no unsafe.
        msg.m_u.m_lsys_vm_info = minix_types::ipc::MessLsysVmInfo {
            what,
            ep: ep.get(),
            count,
            _pad: 0,
            ptr: 0,
            next,
            _padding: [0; 24],
        };
        match perform_taskcall(transport, Endpoint::VM, minix_sys::vm::VM_CALL_INFO, &mut msg) {
            Ok(status) => Ok((status, msg)),
            Err(e) => Err(e),
        }
    }

    /// Fold taskcall's two failure levels into C's integer status face.
    fn status(r: Result<(i32, Message), minix_types::Errno>) -> (i32, Option<Message>) {
        match r {
            Ok((minix_types::OK, msg)) => (minix_types::OK, Some(msg)),
            Ok((status, _)) => (status, None),
            Err(e) => (-e.to_i32(), None),
        }
    }

    pub fn vm_stats_via(transport: &impl IpcTransport, out: &mut VmStatsSnap) -> i32 {
        let (status, msg) = Self::status(Self::call(
            transport,
            minix_types::VMIW_STATS,
            Endpoint::NONE,
            0,
            0,
        ));
        let Some(msg) = msg else { return status };
        // SAFETY: VM's STATS reply uses the m1 slots (encode.rs's InfoStats
        // arm).
        let m1 = unsafe { &msg.m_u.m_m1 };
        *out = VmStatsSnap {
            vsi_pagesize: m1.m1p1 as u32,
            vsi_total: m1.m1i1 as u64,
            vsi_free: m1.m1i2 as u64,
            vsi_largest: m1.m1i3 as u64,
            vsi_cached: m1.m1p2,
        };
        minix_types::OK
    }

    pub fn vm_usage_via(
        transport: &impl IpcTransport,
        who: Endpoint,
        out: &mut VmUsageSnap,
    ) -> i32 {
        let (status, msg) = Self::status(Self::call(
            transport,
            minix_types::VMIW_USAGE,
            who,
            0,
            0,
        ));
        let Some(msg) = msg else { return status };
        // SAFETY: VM's USAGE reply uses the m1 slots (encode.rs's InfoUsage
        // arm).
        let m1 = unsafe { &msg.m_u.m_m1 };
        *out = VmUsageSnap {
            vui_total: m1.m1p1,
            vui_common: m1.m1p2,
            vui_shared: m1.m1p3,
        };
        minix_types::OK
    }

    /// The most entries one REGION reply can carry (the same contract as
    /// `MAX_VRI_PER_REPLY` in `os/servers/vm/src/ipc/encode.rs`; consumers
    /// continue by cursor and do not use this constant for correctness
    /// decisions).
    pub const VRI_PER_REPLY: usize = 2;

    /// Decode one REGION reply packet: `(entries in this packet, new cursor,
    /// entry byte region)`.
    ///
    /// Layout (the producer's `pack_region_reply` encodes it the same way):
    /// `[0..4)` count, `[4..12)` cursor (LE u64), `[12..)` 20 bytes per entry
    /// (addr/length/prot).
    fn decode_region_payload(msg: &Message) -> (usize, u64, &[u8]) {
        // SAFETY: a raw view of the reply payload; this function only reads,
        // and the length is checked against the packet's count.
        let raw = unsafe { &msg.m_u.raw };
        let count = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]).max(0) as usize;
        let mut next_bytes = [0u8; 8];
        next_bytes.copy_from_slice(&raw[4..12]);
        let next = u64::from_le_bytes(next_bytes);
        let want = count.min(Self::VRI_PER_REPLY);
        let body = &raw[12..12 + want * 20];
        (want, next, body)
    }

    /// The region batch leg: **keep taking batches** until the caller's buffer
    /// is full, the server reports empty, or the cursor stops advancing (loop
    /// guard).
    ///
    /// C's `vm_info_region` moves `count` entries per call (`utility.c:142-160`
    /// uses `MIN(count, MAX_VRI_COUNT)`) and the count is bounded by the
    /// caller's buffer; minix-rs's IPC model inlines the entries in the reply
    /// (`[ARCH: 26-D7]` value channel, [`Self::VRI_PER_REPLY`] entries per
    /// packet), so this stitches the batches for the caller -- the caller sees
    /// the same semantics as C (entry count + new cursor); only the number of
    /// round trips differs.
    ///
    /// Returns `(status, new cursor, entries received)`: when `status != OK`,
    /// the untouched part of `out` keeps its previous contents and the cursor
    /// falls back to the last applied packet.
    pub fn vm_region_via(
        transport: &impl IpcTransport,
        who: Endpoint,
        out: &mut [VmRegionSnap],
        next: u64,
    ) -> (i32, u64, i32) {
        let mut cursor = next;
        let mut filled = 0usize;
        while filled < out.len() {
            let want = (out.len() - filled).min(Self::VRI_PER_REPLY);
            let (status, msg) = Self::status(Self::call(
                transport,
                minix_types::VMIW_REGION,
                who,
                want as i32,
                cursor,
            ));
            let Some(msg) = msg else {
                // The server refused: report the entries received so far
                // truthfully, and leave the cursor at the last applied packet.
                return (status, cursor, filled as i32);
            };
            let (got, next_out, body) = Self::decode_region_payload(&msg);
            for i in 0..got {
                let at = i * 20;
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&body[at..at + 8]);
                let addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&body[at + 8..at + 16]);
                let length = u64::from_le_bytes(b8);
                let mut b4 = [0u8; 4];
                b4.copy_from_slice(&body[at + 16..at + 20]);
                out[filled] = VmRegionSnap {
                    vri_addr: addr,
                    vri_length: length,
                    vri_prot: i32::from_le_bytes(b4),
                    vri_flags: 0, // C's vri_flags is always zero (region.c never writes it)
                };
                filled += 1;
            }
            if got == 0 {
                // An empty packet means enumeration is exhausted: that is what
                // C's `get_region_info` returning 0 means.
                return (minix_types::OK, next_out, filled as i32);
            }
            if next_out == cursor {
                // The cursor did not advance = the server made no progress;
                // stop batching to avoid a loop.
                return (minix_types::OK, cursor, filled as i32);
            }
            cursor = next_out;
        }
        (minix_types::OK, cursor, filled as i32)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sys_acquires_hosted_negative_errno_and_uptime_zero() {
        // Hosted: the direct trap honestly returns negative errno (the face
        // where C tests `r`); uptime returns 0 (C's getticks has no failure
        // face, so a failure is recorded as 0).
        use super::{ClockTransport, DiagctlTransport, SysAcquires, SysGetinfoTransport};
        let mut a = SysAcquires::default();
        let mut kinfo = KinfoStruct::default();
        assert!(a.get_kinfo(&mut kinfo) < 0);
        let mut img = [BootImageStruct::default(); 4];
        assert!(a.get_image(&mut img) < 0);
        let mut mon = [0u8; 16];
        assert!(a.get_monparams(&mut mon) < 0);
        assert!(a.stacktrace(Endpoint(9)) < 0);
        assert_eq!(a.uptime(), 0);
    }

    #[test]
    fn test_sys_acquires_kmessages_leg_is_live_not_fail_closed() {
        // Witness that the kmessages leg is live: it has been switched to
        // KernelKmessTransport -- hosted builds truthfully answer -EIO
        // (DirectKernelCallTransport's real-trap gate), no longer a fail-closed
        // panic (the placeholder double was deleted together with the full
        // wiring of all six channels).
        use super::{KerninfoTransport, SysAcquires};
        let mut a = SysAcquires::default();
        let mut meta = KmessagesSnap::default();
        let mut ring = [0u8; 64];
        assert_eq!(a.kmessages(&mut meta, &mut ring), -minix_types::EIO);
    }

    #[test]
    fn test_sys_acquires_getinfo_wire_shape() {
        // CannedKernelCallTransport asserts SYS_GETINFO's fields: the request
        // and val_len (the size_of view) follow the struct's real shape.
        use super::{SysAcquires, SysGetinfoTransport};
        use minix_sys::syscall::CannedKernelCallTransport;

        let mut a = SysAcquires::default();
        // Observability through the carrier: assert the shape via
        // sys_getinfo_into directly, bypassing the struct method (the method
        // goes through the same carrier).
        let canned = CannedKernelCallTransport::new();
        let mut kinfo = KinfoStruct::default();
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                (&mut kinfo as *mut KinfoStruct).cast::<u8>(),
                core::mem::size_of::<KinfoStruct>(),
            )
        };
        //[SD19-gate] best-effort
        #[allow(clippy::let_underscore_must_use)]
        let _ = minix_sys::syscall::sys_getinfo_into(
            &canned,
            minix_types::GET_KINFO,
            bytes,
            Endpoint::NONE.0,
        );
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        // SAFETY: the assertion side reads in m_lsys_krn_sys_getinfo field
        // order.
        let gi = unsafe { &sent[0].m_u.m_lsys_krn_sys_getinfo };
        assert_eq!(gi.request, minix_types::GET_KINFO);
        assert_eq!(gi.val_len as usize, core::mem::size_of::<KinfoStruct>());
        assert_eq!(gi.endpt, Endpoint::NONE.0);
        //[SD19-gate] best-effort
        #[allow(clippy::let_underscore_must_use)]
        let _ = &mut a; // the method face matches the carrier (the direct call above shows the wire)
    }

    use super::fake::FakeAcquires;
    use super::*;
    use minix_types::{DIAGCTL_CODE_STACKTRACE, OK};

    #[test]
    fn test_si_what_codes_and_call_table() {
        // C: sysinfo.h:11-17 + IS call sites (dmp_pm/fs/rs/ds.c). SiWhat is
        // the wire-encoding helper for the production transport.
        assert_eq!(SiWhat::ProcTab.code(), SI_PROC_TAB);
        assert_eq!(SiWhat::DmapTab.code(), SI_DMAP_TAB);
        assert_eq!(SiWhat::ProcPubTab.code(), SI_PROCPUB_TAB);
        assert_eq!(SiWhat::DataStore.code(), SI_DATA_STORE);
        // The RS double-pull trap: same table, two owners.
        assert_eq!(IS_GETSYSINFO_CALLS.len(), 6);
        assert!(IS_GETSYSINFO_CALLS.contains(&(Endpoint::RS, SiWhat::ProcTab)));
        assert!(IS_GETSYSINFO_CALLS.contains(&(Endpoint::PM, SiWhat::ProcTab)));
    }

    #[test]
    fn test_getsysinfo_call_map() {
        // C: switch (who) — getsysinfo.c:14-24.
        assert_eq!(getsysinfo_call(Endpoint::PM), PM_GETSYSINFO);
        assert_eq!(getsysinfo_call(Endpoint::VFS), VFS_GETSYSINFO);
        assert_eq!(getsysinfo_call(Endpoint::RS), RS_GETSYSINFO);
        assert_eq!(getsysinfo_call(Endpoint::DS), DS_GETSYSINFO);
        assert_eq!(getsysinfo_call(Endpoint::TTY), minix_types::ENOSYS);
        assert_eq!(getsysinfo_call(Endpoint::VM), minix_types::ENOSYS);
    }

    #[test]
    fn test_channels_ok_and_err_paths() {
        // 04 §2.6: acquisition failure is recoverable — statuses flow back
        // to the caller (05~10 warn-and-continue), never panic here.
        let mut f = FakeAcquires::ok();
        let mut kinfo = KinfoStruct::default();
        assert_eq!(f.get_kinfo(&mut kinfo), OK);
        assert_eq!(f.stacktrace(Endpoint::PM), OK);
        let mut meta = KmessagesSnap::default();
        let mut ring = [0u8; 4];
        assert_eq!(f.kmessages(&mut meta, &mut ring), OK);
        let mut tab = [MProcSnap::default(); 2];
        assert_eq!(f.pm_proc_tab(&mut tab), OK);
        assert_eq!(tab[1].mp_pid, 1, "OK fetch fills the outlet");
        let mut pubt = [RprocpubSnap::default(); 2];
        let mut privt = [RprocSnap::default(); 2];
        assert_eq!(f.rs_tables(&mut pubt, &mut privt), OK);
        let mut usage = VmUsageSnap::default();
        assert_eq!(f.vm_usage(Endpoint::PM, &mut usage), OK);
        let mut regions = [VmRegionSnap::default(); 8];
        assert_eq!(f.vm_region(Endpoint::PM, &mut regions, 0), (OK, 0, 3));
        assert_eq!(
            f.seen,
            ["kinfo", "stacktrace", "kmessages", "pm_proc_tab", "rs_tables", "vm_usage", "vm_region"]
        );

        f.getinfo_status = minix_types::EFAULT;
        let mut procs = [ProcInfoStruct::default(); 1];
        assert_eq!(f.get_proctab(&mut procs), minix_types::EFAULT);
        assert_eq!(procs[0].p_nr, 0, "failed fetch leaves the outlet untouched");
    }

    #[test]
    fn test_monparams_outlet_writes() {
        // C: sys_getmonparams(val, sizeof(val)) fills a NUL-separated blob —
        // the outlet is the caller's buffer, filled in place.
        let mut f = FakeAcquires::ok();
        let mut buf = [0u8; 8];
        assert_eq!(f.get_monparams(&mut buf), OK);
        assert_eq!(&buf[..3], b"ab\n");
    }

    #[test]
    fn test_diagctl_code_reference_stays_live() {
        // C: com.h:413. The stacktrace method fixes the code internally;
        // the constant stays imported so the wire value is asserted once.
        assert_eq!(DIAGCTL_CODE_STACKTRACE, 2);
    }
}

#[cfg(test)]
mod kerninfo_transport_tests {
    use super::*;

    /// Snapshot unpacking contract: the 8-byte header (km_next/km_size, LE i32)
    /// goes into meta, and the ring body fills the prefix that fits in the
    /// caller's buffer (the same wire shape as the kernel's
    /// `kmess::KMESS_SNAPSHOT_SIZE` -- kernel/src/kmess.rs:29-32).
    #[test]
    fn test_split_kmess_snapshot_round_trip() {
        let mut snap = vec![0u8; 10008];
        snap[0..4].copy_from_slice(&5i32.to_le_bytes());      // km_next
        snap[4..8].copy_from_slice(&10_000i32.to_le_bytes()); // km_size
        for (i, b) in snap[8..].iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }

        let mut meta = KmessagesSnap { km_next: 0, km_size: 0 };
        let mut ring = [0u8; 10_000];
        split_kmess_snapshot(&snap, &mut meta, &mut ring);

        assert_eq!(meta.km_next, 5);
        assert_eq!(meta.km_size, 10_000);
        assert_eq!(ring[0], (0 % 251) as u8);
        assert_eq!(ring[9999], (9999 % 251) as u8);
    }

    /// A caller buffer shorter than the ring body truncates to its capacity --
    /// kmessages_dmp's print assembly lays itself out by the km_size cursor
    /// (dmp_kernel.c:77-85), decoupled from the snapshot's total length.
    #[test]
    fn test_split_kmess_snapshot_truncates_to_ring_capacity() {
        let mut snap = vec![0u8; 10008];
        snap[0..4].copy_from_slice(&7i32.to_le_bytes());
        snap[4..8].copy_from_slice(&10_000i32.to_le_bytes());

        let mut meta = KmessagesSnap { km_next: 0, km_size: 0 };
        let mut ring = [0xABu8; 128];
        split_kmess_snapshot(&snap, &mut meta, &mut ring);

        assert_eq!(meta.km_next, 7);
        assert_eq!(&ring[..4], &[0, 0, 0, 0]); // first 4 body bytes (snapshot zero fill)
        assert_eq!(ring[127], 0);
    }

    /// In hosted builds (without real-trap) DirectKernelCallTransport honestly
    /// returns -EIO: the production transport surfaces it unchanged, neither
    /// swallowing the error nor pretending success -- the caller handles it with
    /// fail-closed semantics (the same gated shape as minix-rt
    /// DirectTrapSource).
    #[test]
    fn test_kernel_kmess_transport_hosted_is_eio() {
        let mut t = KernelKmessTransport;
        let mut meta = KmessagesSnap { km_next: 0, km_size: 0 };
        let mut ring = [0u8; 64];
        let code = t.kmessages(&mut meta, &mut ring);
        assert_eq!(code, -minix_types::EIO);
    }

    /// Snapshot size contract: an 8-byte wire header + a 10000-byte ring body
    /// (sys_config.h:22).
    #[test]
    fn test_kmess_snapshot_size_matches_kernel() {
        assert_eq!(KMESS_SNAPSHOT_SIZE, 10008);
    }
}

#[cfg(test)]
mod vfs_proc_tab_transport_tests {
    use super::fake::FakeAcquires;
    use super::*;
    use minix_sys::ipc::CannedTransport;
    use minix_types::OK;

    /// Wire triple replay (the PM leg, an already-aligned production leg):
    /// the request arm = `getsysinfo(PM, SI_PROC_TAB, buf, 76*len)` --
    /// destination PM, call number `PM_GETSYSINFO`, and the what/where/size lanes
    /// (C getsysinfo.c:14-31 + dmp_pm.c:47); the answer arm surfaces `m_type`
    /// unchanged (`_taskcall` semantics, OK means success).
    #[test]
    fn test_pm_proc_tab_wire_roundtrip_canned() {
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = OK;
        canned.reply_sendrec(Ok(reply));

        let mut tab = [MProcSnap::default(); 4];
        assert_eq!(SysGetsysinfo::pm_proc_tab_via(&canned, &mut tab), OK);

        let sent = canned.sent.borrow();
        let (dest, msg) = &sent[0];
        assert_eq!(*dest, Endpoint::PM);
        assert_eq!(msg.m_type, getsysinfo_call(Endpoint::PM));
        assert_eq!(msg.m_type, PM_GETSYSINFO);
        // SAFETY(test): read back the three packed lanes (the same layout as
        // SysGetsysinfo::fetch).
        unsafe {
            let raw = &msg.m_u.raw;
            let what = i32::from_ne_bytes([raw[0], raw[1], raw[2], raw[3]]);
            let mut where_bytes = [0u8; 8];
            where_bytes.copy_from_slice(&raw[8..16]);
            let where_addr = u64::from_ne_bytes(where_bytes);
            let mut size_bytes = [0u8; 8];
            size_bytes.copy_from_slice(&raw[16..24]);
            let size = u64::from_ne_bytes(size_bytes);
            assert_eq!(what, SI_PROC_TAB);
            assert_eq!(where_addr, tab.as_mut_ptr() as u64);
            assert_eq!(size, (tab.len() * core::mem::size_of::<MProcSnap>()) as u64);
        }
    }

    /// Replay of the RS/DS legs' call numbers and what values: `SI_PROCPUB_TAB`
    /// (11) and `SI_PROC_TAB` (2) are pulled separately (C dmp_rs.c:33-34), and
    /// DS pulls `SI_DATA_STORE` (5) (C dmp_ds.c:15) -- all three call numbers
    /// come from `getsysinfo_call` (the switch in C getsysinfo.c:14-24).
    #[test]
    fn test_rs_and_ds_legs_wire_values_canned() {
        let mut canned = CannedTransport::new();
        let mut ok = Message::default();
        ok.m_type = OK;
        canned.reply_sendrec(Ok(ok));
        canned.reply_sendrec(Ok(ok));

        let mut pubt = [RprocpubSnap::default(); 2];
        let mut privt = [RprocSnap::default(); 2];
        assert_eq!(SysGetsysinfo::rs_tables_via(&canned, &mut pubt, &mut privt), OK);

        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 2, "PUB 与 PRIV 各一次调用");
        assert_eq!(sent[0].0, Endpoint::RS);
        assert_eq!(sent[0].1.m_type, RS_GETSYSINFO);
        assert_eq!(sent[1].0, Endpoint::RS);
        assert_eq!(sent[1].1.m_type, RS_GETSYSINFO);
        // SAFETY(test): what @0..4 (the same layout as fetch).
        let what_of = |m: &Message| unsafe {
            i32::from_ne_bytes([m.m_u.raw[0], m.m_u.raw[1], m.m_u.raw[2], m.m_u.raw[3]])
        };
        assert_eq!(what_of(&sent[0].1), SI_PROCPUB_TAB);
        assert_eq!(what_of(&sent[1].1), SI_PROC_TAB);

        let mut canned = CannedTransport::new();
        let mut ok = Message::default();
        ok.m_type = OK;
        canned.reply_sendrec(Ok(ok));
        let mut store = [DsEntrySnap::default(); 2];
        assert_eq!(SysGetsysinfo::ds_data_store_via(&canned, &mut store), OK);
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].0, Endpoint::DS);
        assert_eq!(sent[0].1.m_type, DS_GETSYSINFO);
        assert_eq!(what_of(&sent[0].1), SI_DATA_STORE);
    }

    /// RS double-pull short circuit: a PUB failure returns at once without
    /// sending PRIV (C dmp_rs.c:33-34's `||`).
    #[test]
    fn test_rs_tables_short_circuit_on_pub_failure() {
        let mut canned = CannedTransport::new();
        let mut bad = Message::default();
        bad.m_type = minix_types::EINVAL;
        canned.reply_sendrec(Ok(bad));

        let mut pubt = [RprocpubSnap::default(); 2];
        let mut privt = [RprocSnap::default(); 2];
        assert_eq!(
            SysGetsysinfo::rs_tables_via(&canned, &mut pubt, &mut privt),
            minix_types::EINVAL
        );
        assert_eq!(canned.sent.borrow().len(), 1, "PUB 失败后不再发 PRIV");
    }

    /// Server negative replies (EINVAL = size gate, EPERM = root gate) surface
    /// unchanged -- run_dump's `!= OK` consumer follows 04 section 2.6
    /// warn-and-continue.
    #[test]
    fn test_server_error_reply_passthrough() {
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = -minix_types::EINVAL;
        canned.reply_sendrec(Ok(reply));

        let mut tab = [MProcSnap::default(); 2];
        assert_eq!(
            SysGetsysinfo::pm_proc_tab_via(&canned, &mut tab),
            -minix_types::EINVAL
        );
    }

    /// In hosted builds (without real-trap) DirectTrapTransport honestly
    /// reports failure -- a transport-level failure yields `Err(Errno(EIO))`
    /// (the TrapStatus sign contract), the trait consumer folds it back to the C
    /// integer contract's `-EIO`, and no success is faked (the same gated
    /// verification as `KernelKmessTransport`).
    #[test]
    fn test_getsysinfo_legs_hosted_are_eio() {
        use super::{GetSysinfoTransport, SysGetsysinfo};
        let mut sys = SysGetsysinfo;
        let mut pm = [MProcSnap::default(); 2];
        assert_eq!(sys.pm_proc_tab(&mut pm), -minix_types::EIO);
        let mut pubt = [RprocpubSnap::default(); 2];
        let mut privt = [RprocSnap::default(); 2];
        assert_eq!(sys.rs_tables(&mut pubt, &mut privt), -minix_types::EIO);
        let mut store = [DsEntrySnap::default(); 2];
        assert_eq!(sys.ds_data_store(&mut store), -minix_types::EIO);
    }

    /// The VFS legs' request shape and hosted behavior: destination VFS, call
    /// number `VFS_GETSYSINFO`, what values `SI_PROC_TAB` (2) and `SI_DMAP_TAB`
    /// (3) respectively, and size = the output slice's byte length (the
    /// server's exact-length gate); in hosted builds the real trap is
    /// unreachable, so it truthfully returns `-EIO` (no local fail-closed exit
    /// is needed any more -- VFS's answer face is powered).
    #[test]
    fn test_vfs_legs_wire_and_hosted_eio() {
        use minix_sys::ipc::CannedTransport;
        use super::{GetSysinfoTransport, SysGetsysinfo};

        let mut canned = CannedTransport::new();
        let mut ok = Message::default();
        ok.m_type = OK;
        canned.reply_sendrec(Ok(ok));
        canned.reply_sendrec(Ok(ok));

        let mut fp = [FProcSnap::default(); 3];
        assert_eq!(SysGetsysinfo::vfs_proc_tab_via(&canned, &mut fp), OK);
        let mut dm = [DmapSnap::default(); 2];
        assert_eq!(SysGetsysinfo::vfs_dmap_tab_via(&canned, &mut dm), OK);

        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, Endpoint::VFS);
        assert_eq!(sent[0].1.m_type, VFS_GETSYSINFO);
        assert_eq!(sent[1].0, Endpoint::VFS);
        assert_eq!(sent[1].1.m_type, VFS_GETSYSINFO);
        // SAFETY(test): the what/where/size lanes (the same layout as fetch).
        let lanes = |m: &Message| unsafe {
            let raw = &m.m_u.raw;
            let what = i32::from_ne_bytes([raw[0], raw[1], raw[2], raw[3]]);
            let mut sz = [0u8; 8];
            sz.copy_from_slice(&raw[16..24]);
            (what, u64::from_ne_bytes(sz))
        };
        assert_eq!(
            lanes(&sent[0].1),
            (SI_PROC_TAB, (fp.len() * core::mem::size_of::<FProcSnap>()) as u64)
        );
        assert_eq!(
            lanes(&sent[1].1),
            (SI_DMAP_TAB, (dm.len() * core::mem::size_of::<DmapSnap>()) as u64)
        );

        // Hosted: the real trap is unreachable -> transport-level -EIO (the
        // honest face).
        let mut sys = SysGetsysinfo;
        let mut fp = [FProcSnap::default(); 2];
        assert_eq!(sys.vfs_proc_tab(&mut fp), -minix_types::EIO);
        let mut dm = [DmapSnap::default(); 2];
        assert_eq!(sys.vfs_dmap_tab(&mut dm), -minix_types::EIO);
    }

    /// The three VM_INFO queries' request shape and reply-slot decoding (value
    /// channel, `[ARCH: 26-D1]`): STATS reads m1p1/m1i1/m1i2/m1i3/m1p2, USAGE
    /// reads m1p1/p2/p3 (bytes), and REGION reads m1i1/m1i2. The request side
    /// asserts destination VM, call number `VM_INFO`, the what value (1/2/3 from
    /// C com.h:732-734), and the ep/count fields.
    #[test]
    fn test_vm_info_stats_usage_wire_and_slots() {
        // STATS: the server writes five values into the m1 slots (InfoStats in
        // vm/ipc/encode.rs).
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = OK;
        // SAFETY(test): fill in VM's InfoStats slot order.
        unsafe {
            let m1 = &mut reply.m_u.m_m1;
            m1.m1p1 = 4096; // page size
            m1.m1i1 = 100; // total pages
            m1.m1i2 = 40; // free pages
            m1.m1i3 = 12; // largest contiguous block
            m1.m1p2 = 7; // cached pages
        }
        canned.reply_sendrec(Ok(reply));

        let mut vsi = VmStatsSnap::default();
        assert_eq!(SysVmInfo::vm_stats_via(&canned, &mut vsi), OK);
        assert_eq!(vsi.vsi_pagesize, 4096);
        assert_eq!(vsi.vsi_total, 100);
        assert_eq!(vsi.vsi_free, 40);
        assert_eq!(vsi.vsi_largest, 12);
        assert_eq!(vsi.vsi_cached, 7);

        let sent = canned.sent.borrow();
        assert_eq!(sent[0].0, Endpoint::VM);
        assert_eq!(sent[0].1.m_type, minix_sys::vm::VM_CALL_INFO);
        // SAFETY(test): the request fields (the same layout as SysVmInfo::call).
        let req = unsafe { &sent[0].1.m_u.m_lsys_vm_info };
        assert_eq!(req.what, minix_types::VMIW_STATS);
        assert_eq!(req.ptr, 0, "值通道：不把调用方缓冲地址交给服务端");

        // USAGE: the three byte values go into m1p1/p2/p3 (InfoUsage in
        // encode.rs).
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = OK;
        // SAFETY(test): fill in the InfoUsage slot order.
        unsafe {
            let m1 = &mut reply.m_u.m_m1;
            m1.m1p1 = 8192;
            m1.m1p2 = 4096;
            m1.m1p3 = 1024;
        }
        canned.reply_sendrec(Ok(reply));

        let mut vui = VmUsageSnap::default();
        assert_eq!(SysVmInfo::vm_usage_via(&canned, Endpoint::PM, &mut vui), OK);
        assert_eq!(
            (vui.vui_total, vui.vui_common, vui.vui_shared),
            (8192, 4096, 1024)
        );
        let sent = canned.sent.borrow();
        // SAFETY(test): the request fields.
        let req = unsafe { &sent[0].1.m_u.m_lsys_vm_info };
        assert_eq!(req.what, minix_types::VMIW_USAGE);
        assert_eq!(req.ep, Endpoint::PM.get());
    }

    /// REGION value-channel batching (`[ARCH: 26-D7]`): the client stitches
    /// several packets for the caller -- each packet is decoded by
    /// `count`/`next`/the entry region, and an empty packet (count=0) means
    /// enumeration is exhausted; the request-side `count` is "how many I still
    /// need, capped at one packet's capacity".
    #[test]
    fn test_vm_info_region_batches_until_empty() {
        let packed = |addrs: &[(u64, u64, i32)], next: u64| {
            let mut v = Vec::new();
            v.extend_from_slice(&(addrs.len() as i32).to_le_bytes());
            v.extend_from_slice(&next.to_le_bytes());
            for (a, l, p) in addrs {
                v.extend_from_slice(&a.to_le_bytes());
                v.extend_from_slice(&l.to_le_bytes());
                v.extend_from_slice(&p.to_le_bytes());
            }
            v
        };
        let reply_with = |payload: &[u8]| {
            let mut m = Message::default();
            m.m_type = OK;
            // SAFETY(test): fill the reply payload in the packed layout.
            unsafe {
                m.m_u.raw[..payload.len()].copy_from_slice(payload);
            }
            m
        };

        let mut canned = CannedTransport::new();
        canned.reply_sendrec(Ok(reply_with(&packed(&[(0x1000, 0x1000, 1)], 0x2000))));
        canned.reply_sendrec(Ok(reply_with(&packed(&[(0x2000, 0x1000, 3)], 0x3000))));
        canned.reply_sendrec(Ok(reply_with(&packed(&[], 0x3000))));

        let mut out = [VmRegionSnap::default(); 4];
        let (status, next, count) =
            SysVmInfo::vm_region_via(&canned, Endpoint::PM, &mut out, 0x1000);
        assert_eq!((status, next, count), (OK, 0x3000, 2));
        assert_eq!(out[0].vri_addr, 0x1000);
        assert_eq!(out[0].vri_length, 0x1000);
        assert_eq!(out[0].vri_prot, 1);
        assert_eq!(out[0].vri_flags, 0, "C 的 vri_flags 恒零");
        assert_eq!(out[1].vri_addr, 0x2000);
        assert_eq!(out[1].vri_prot, 3);

        // Three packet requests: count decreases packet by packet to "how many
        // I still need" (<= one packet's capacity), and the cursor advances per
        // packet.
        let sent = canned.sent.borrow();
        let reqs: Vec<_> = sent
            .iter()
            .map(|(_, m)| unsafe {
                let g = &m.m_u.m_lsys_vm_info;
                (g.what, g.count, g.next)
            })
            .collect();
        assert_eq!(reqs[0], (minix_types::VMIW_REGION, 2, 0x1000));
        assert_eq!(reqs[1], (minix_types::VMIW_REGION, 2, 0x2000));
        assert_eq!(reqs[2], (minix_types::VMIW_REGION, 2, 0x3000));
    }

    /// In hosted builds all three VM legs honestly surface -EIO and never fake
    /// success (the same gated shape as `KernelKmessTransport`).
    #[test]
    fn test_vm_info_legs_hosted_are_eio() {
        use super::{SysAcquires, VmInfoTransport};
        let mut a = SysAcquires::default();
        let mut vsi = VmStatsSnap::default();
        assert_eq!(a.vm_stats(&mut vsi), -minix_types::EIO);
        let mut vui = VmUsageSnap::default();
        assert_eq!(a.vm_usage(Endpoint::PM, &mut vui), -minix_types::EIO);
        let mut out = [VmRegionSnap::default(); 2];
        assert_eq!(a.vm_region(Endpoint::PM, &mut out, 0), (-minix_types::EIO, 0, 0));
    }

    /// FakeAcquires still has the vfs_proc_tab leg (the orchestration-layer
    /// tests do not swap it) -- this module verifies only the production
    /// transport itself; the main swap is tracked as E-ISWIRE's third item.
    #[test]
    fn test_fake_acquires_leg_unchanged() {
        let mut f = FakeAcquires::ok();
        let mut tab = [FProcSnap::default(); 2];
        assert_eq!(f.vfs_proc_tab(&mut tab), OK);
        assert_eq!(tab[1].fp_pid, 1);
    }
}

//! Data-acquisition channels (04-is-data-acquisition.md §4.1).
//!
//! The five ways IS pulls debug data: `sys_getinfo` (kernel tables),
//! `sys_diagctl` (stack traces), the kernel-message channel (`[ARCH: A-3]`),
//! `getsysinfo` (peer-server tables), `vm_info` (address spaces). Each
//! channel is a trait; production implementations land with the
//! `minix-sys`/kernel wiring (forward references, 01 §3 D2 pattern).
//!
//! Data outlets (V1-P1-2): C copies each table *into the caller*
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

/// GET_KMESSAGES 快照总大小（kernel `kmess::KMESS_SNAPSHOT_SIZE` 镜像：
/// 8 字节游标头 + 10000 字节环体，sys_config.h:22）。内核侧常量为
/// pub(crate)，此处按同一 wire 契约本地锚定并测试锁定。
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
/// (`28-usermapped-data.md`); the production implementation is a new
/// `GET_KMESSAGES`-equivalent `sys_getinfo` sub-request (kernel edge
/// E-ISKMESS): the kernel copies the raw ring into `ring` and the cursor
/// fields into `meta`, and IS computes the print start via
/// `kmess_start` (05 §3 D4). Until the kernel wiring lands this is
/// fail-closed. The `DIAGCTL_CODE_STACKTRACE` channel is NOT reused: it
/// prints into the kernel log and returns no buffer (different semantics).
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
/// IS-side obligations (04 §3 D4): always root (IS uid 0), exact
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
/// minix-rs has no usermapped page (A-3); the production transport routes
/// the equivalent kernel call (SYS_TIMES family). Consumer: 06
/// (sigaction's alarm countdown, dmp_pm.c:78/101-102).
pub trait ClockTransport {
    /// Ticks since boot. C: `getticks()` — getticks.c:8-12.
    fn uptime(&mut self) -> u32;
}

/// The five channels bundled for the orchestrator.
///
/// The per-channel traits stay fine-grained (04 §3 D2: each dump domain
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

/// Fail-closed bundle until the `minix-sys`/kernel wiring lands
/// (01 `UnimplementedTransport` pattern).
#[derive(Debug, Default)]
pub struct UnimplementedAcquires;

impl SysGetinfoTransport for UnimplementedAcquires {
    fn get_kinfo(&mut self, _out: &mut KinfoStruct) -> i32 {
        panic!("IS acquire: sys_getinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn get_image(&mut self, _out: &mut [BootImageStruct]) -> i32 {
        panic!("IS acquire: sys_getinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn get_proctab(&mut self, _out: &mut [ProcInfoStruct]) -> i32 {
        panic!("IS acquire: sys_getinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn get_monparams(&mut self, _out: &mut [u8]) -> i32 {
        panic!("IS acquire: sys_getinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn get_irqhooks(&mut self, _out: &mut [IrqHookStruct]) -> i32 {
        panic!("IS acquire: sys_getinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn get_irqactids(&mut self, _out: &mut [i32]) -> i32 {
        panic!("IS acquire: sys_getinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn get_privtab(&mut self, _out: &mut [PrivInfoStruct]) -> i32 {
        panic!("IS acquire: sys_getinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }
}

impl DiagctlTransport for UnimplementedAcquires {
    fn stacktrace(&mut self, _proc: Endpoint) -> i32 {
        panic!("IS acquire: sys_diagctl wiring pending (04-is-data-acquisition.md §3 D2)");
    }
}

impl KerninfoTransport for UnimplementedAcquires {
    fn kmessages(&mut self, _meta: &mut KmessagesSnap, _ring: &mut [u8]) -> i32 {
        panic!("IS acquire: GET_KMESSAGES wiring pending ([ARCH: A-3], edge E-ISKMESS)");
    }
}

impl GetSysinfoTransport for UnimplementedAcquires {
    fn pm_proc_tab(&mut self, _out: &mut [MProcSnap]) -> i32 {
        panic!("IS acquire: getsysinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn vfs_proc_tab(&mut self, _out: &mut [FProcSnap]) -> i32 {
        panic!("IS acquire: getsysinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn vfs_dmap_tab(&mut self, _out: &mut [DmapSnap]) -> i32 {
        panic!("IS acquire: getsysinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn rs_tables(
        &mut self,
        _pub_out: &mut [RprocpubSnap],
        _priv_out: &mut [RprocSnap],
    ) -> i32 {
        panic!("IS acquire: getsysinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn ds_data_store(&mut self, _out: &mut [DsEntrySnap]) -> i32 {
        panic!("IS acquire: getsysinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }
}

impl ClockTransport for UnimplementedAcquires {
    fn uptime(&mut self) -> u32 {
        panic!("IS acquire: clock wiring pending (06 sigaction uptime; edge E-ISWIRE)");
    }
}

impl VmInfoTransport for UnimplementedAcquires {
    fn vm_stats(&mut self, _out: &mut VmStatsSnap) -> i32 {
        panic!("IS acquire: vm_info wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn vm_usage(&mut self, _who: Endpoint, _out: &mut VmUsageSnap) -> i32 {
        panic!("IS acquire: vm_info wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn vm_region(
        &mut self,
        _who: Endpoint,
        _out: &mut [VmRegionSnap],
        _next: u64,
    ) -> (i32, u64, i32) {
        panic!("IS acquire: vm_info wiring pending (04-is-data-acquisition.md §3 D2)");
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
/// 生产 `KerninfoTransport`：经 `sys_getinfo` 的 `GET_KMESSAGES`
/// 子请求（E-ISKMESS A-3，minix-types `sysinfo::GET_KMESSAGES` = 7）从
/// 内核拉取环形缓冲快照并拆包。
///
/// 快照 wire 形状（kernel `kmess::KMESS_SNAPSHOT_SIZE` = 10008）：
/// `km_next` (i32) @0、`km_size` (i32) @4、顺序展开的环体 10000 字节。
/// 拆包后 `meta` 携游标、`ring` 携调用方缓冲能容纳的环体前缀。
///
/// 宿主构建（未开 `real-trap`）下 transport 诚实返回 `-EIO`，本实现原样
/// 上浮——调用方按既有 fail-closed 语义处理，与
/// `minix-rt::DirectTrapSource` 同款门控形态。
pub struct KernelKmessTransport;

impl KerninfoTransport for KernelKmessTransport {
    fn kmessages(&mut self, meta: &mut KmessagesSnap, ring: &mut [u8]) -> i32 {
        // 10008 字节快照缓冲：固定容量（内核臂单次整块拷出），栈上分配
        // —— IS 用户栈 64 KiB（loader stack），快照占 ~15%，余量充足；
        // C 的 kmessages_dmp print_buf 为同量级缓冲（dmp_kernel.c:74）。
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

/// 拆 GET_KMESSAGES 快照：8 字节头（`km_next`/`km_size`，LE i32）进
/// `meta`，环体按 `ring` 容量填前缀（调用方缓冲短于 10000 时截断——
/// `kmessages_dmp` 的 print 组装按 `km_size` 游标重排，与容量无关）。
fn split_kmess_snapshot(snap: &[u8], meta: &mut KmessagesSnap, ring: &mut [u8]) {
    meta.km_next = i32::from_le_bytes([snap[0], snap[1], snap[2], snap[3]]);
    meta.km_size = i32::from_le_bytes([snap[4], snap[5], snap[6], snap[7]]);
    let body = &snap[8..];
    let n = body.len().min(ring.len());
    ring[..n].copy_from_slice(&body[..n]);
}

/// 生产 `GetSysinfoTransport` 的 VFS fproc_tab 腿（E-MIBPROD fproc_tab 半，
/// 2026-09-18）。C: `getsysinfo(VFS_PROC_NR, SI_PROC_TAB, fproc,
/// sizeof(fproc))`（libsys/getsysinfo.c:26-31 + dmp_fs.c:31）——what/
/// where/size 三元组经 `m_lsys_getsysinfo` 到 VFS，服务端 `do_getsysinfo`
/// 按 A-4 快照（`FProcSnap` 52B×NR_PROCS，指针数组不出 wire）逐槽拷回。
///
/// 传输 = `DirectTrapTransport`：real-trap 构建下通电即活；宿主构建诚实
/// 上浮 -EIO（`KernelKmessTransport` 同款门控形态）。main 装配挂
/// E-ISWIRE(3)（A-6 诊断通道裁决），当前仍以 `UnimplementedAcquires` 出生。
///
/// 其余四腿不在此武装：pm/rs/ds/vfs_dmap 各归其 producer 域收口后逐腿
/// 接入（DS/PM producer 已闭，IS 侧装配统一随 E-ISWIRE(3)）——域内
/// fail-closed panic 与 `UnimplementedAcquires` 同语义：接线缺口是装配
/// 错误，不是运行时错误。
pub struct VfsProcTabTransport;

impl VfsProcTabTransport {
    /// fproc_tab 腿的可注入传输形态（生产传 [`DirectTrapTransport`]，
    /// 测试传脚本双替身）。
    ///
    /// 消息打包与 `system_info_via` 同布局（what @0..4、where @8..16、
    /// size @16..24），经共享 [`minix_sys::syscall::perform_taskcall`] 上
    /// 浮：传输级失败走 `Err(Errno)`（TrapStatus 符号契约——真机回复寄存
    /// 器与宿主回退臂都报正 errno），sendrec 成功后的原始回复 `m_type`
    /// 走 `Ok`，负值即服务端错误码（C _taskcall 语义）；非零正回复非标
    /// 准，原样上浮交由 `!= OK` 消费面判失败（dmp_fs.c:31）。
    fn vfs_proc_tab_via(
        transport: &impl IpcTransport,
        out: &mut [FProcSnap],
    ) -> Result<i32, minix_types::Errno> {
        let mut msg = Message::default();
        // SAFETY: m_lsys_getsysinfo 的 raw 三 lane 视图（C getsysinfo.c:
        // 27-29 的 what/where/size；where 是调用方缓冲地址，服务端按
        // sys_datacopy SELF→caller 回填）。
        unsafe {
            msg.m_u.raw[..4].copy_from_slice(&SI_PROC_TAB.to_ne_bytes());
            msg.m_u.raw[8..16].copy_from_slice(&(out.as_mut_ptr() as u64).to_ne_bytes());
            msg.m_u.raw[16..24]
                .copy_from_slice(&(core::mem::size_of_val(out) as u64).to_ne_bytes());
        }
        perform_taskcall(transport, Endpoint::VFS, getsysinfo_call(Endpoint::VFS), &mut msg)
    }
}

impl GetSysinfoTransport for VfsProcTabTransport {
    fn vfs_proc_tab(&mut self, out: &mut [FProcSnap]) -> i32 {
        // 消费面维持 C 整型契约（负值 = 失败）：传输级 `Err` 折算回负
        // errno，与 sendrec 成功但服务端回负 `m_type` 的形状合流。
        match Self::vfs_proc_tab_via(&DirectTrapTransport, out) {
            Ok(reply) => reply,
            Err(e) => -e.to_i32(),
        }
    }

    fn pm_proc_tab(&mut self, _out: &mut [MProcSnap]) -> i32 {
        panic!(
            "IS acquire: pm_proc_tab leg not armed in VfsProcTabTransport \
             (PM producer closed by D-29; IS assembly lands with E-ISWIRE(3))"
        );
    }

    fn vfs_dmap_tab(&mut self, _out: &mut [DmapSnap]) -> i32 {
        panic!(
            "IS acquire: vfs_dmap_tab leg not armed in VfsProcTabTransport \
             (DMAP producer pending, dump_vfs TODO(P1); IS assembly lands \
             with E-ISWIRE(3))"
        );
    }

    fn rs_tables(&mut self, _pub_out: &mut [RprocpubSnap], _priv_out: &mut [RprocSnap]) -> i32 {
        panic!(
            "IS acquire: rs_tables leg not armed in VfsProcTabTransport \
             (RS producer lane; IS assembly lands with E-ISWIRE(3))"
        );
    }

    fn ds_data_store(&mut self, _out: &mut [DsEntrySnap]) -> i32 {
        panic!(
            "IS acquire: ds_data_store leg not armed in VfsProcTabTransport \
             (DS producer closed in 07-stage-ds; IS assembly lands with \
             E-ISWIRE(3))"
        );
    }
}

#[cfg(test)]
mod tests {
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
    #[should_panic(expected = "GET_KMESSAGES")]
    fn test_kerninfo_unimplemented_fail_closed() {
        let mut u = UnimplementedAcquires;
        let mut meta = KmessagesSnap::default();
        let mut ring = [0u8; 4];
        let _ = u.kmessages(&mut meta, &mut ring);
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

    /// 快照拆包契约：8 字节头（km_next/km_size，LE i32）进 meta，
    /// 环体按调用方缓冲容量填前缀（kernel kmess::KMESS_SNAPSHOT_SIZE
    /// 同一 wire 形状——kernel/src/kmess.rs:29-32）。
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

    /// 调用方缓冲短于环体时按容量截断——kmessages_dmp 的 print 组装
    /// 按 km_size 游标自排（dmp_kernel.c:77-85），与快照总长解耦。
    #[test]
    fn test_split_kmess_snapshot_truncates_to_ring_capacity() {
        let mut snap = vec![0u8; 10008];
        snap[0..4].copy_from_slice(&7i32.to_le_bytes());
        snap[4..8].copy_from_slice(&10_000i32.to_le_bytes());

        let mut meta = KmessagesSnap { km_next: 0, km_size: 0 };
        let mut ring = [0xABu8; 128];
        split_kmess_snapshot(&snap, &mut meta, &mut ring);

        assert_eq!(meta.km_next, 7);
        assert_eq!(&ring[..4], &[0, 0, 0, 0]); // 体前 4 字节（快照零填充）
        assert_eq!(ring[127], 0);
    }

    /// 宿主构建（未开 real-trap）下 DirectKernelCallTransport 诚实返回
    /// -EIO：生产 transport 原样上浮，不吞错、不假装成功——调用方按
    /// fail-closed 语义处理（minix-rt DirectTrapSource 同款门控形态）。
    #[test]
    fn test_kernel_kmess_transport_hosted_is_eio() {
        let mut t = KernelKmessTransport;
        let mut meta = KmessagesSnap { km_next: 0, km_size: 0 };
        let mut ring = [0u8; 64];
        let code = t.kmessages(&mut meta, &mut ring);
        assert_eq!(code, -minix_types::EIO);
    }

    /// 快照尺寸契约：wire 头 8 字节 + 10000 环体（sys_config.h:22）。
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

    /// wire 三元组回放：请求臂 = `getsysinfo(VFS, SI_PROC_TAB, buf,
    /// 52×len)`——目的地 VFS、调用号 `VFS_GETSYSINFO`、what/where/size 三
    /// lane（C getsysinfo.c:22-31 + dmp_fs.c:31）；应答臂 m_type 原样上浮
    /// （_taskcall 语义，OK(0) 即成功）。
    #[test]
    fn test_vfs_proc_tab_wire_roundtrip_canned() {
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = OK;
        canned.reply_sendrec(Ok(reply));

        let mut tab = [FProcSnap::default(); 4];
        let r = VfsProcTabTransport::vfs_proc_tab_via(&canned, &mut tab);
        assert_eq!(r, Ok(OK));

        let sent = canned.sent.borrow();
        let (dest, msg) = &sent[0];
        assert_eq!(*dest, Endpoint::VFS);
        assert_eq!(msg.m_type, getsysinfo_call(Endpoint::VFS));
        assert_eq!(msg.m_type, VFS_GETSYSINFO);
        // SAFETY(test): 读回打包的三 lane（与 vfs_proc_tab_via 同布局）。
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
            assert_eq!(size, (tab.len() * core::mem::size_of::<FProcSnap>()) as u64);
        }
    }

    /// 服务端负回复（如 ENOSYS/EINVAL）原样上浮为负状态——run_dump 的
    /// `!= OK` 消费面按 04 §2.6 warn-and-continue。
    #[test]
    fn test_vfs_proc_tab_error_reply_passthrough() {
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = -minix_types::EINVAL;
        canned.reply_sendrec(Ok(reply));

        let mut tab = [FProcSnap::default(); 2];
        assert_eq!(
            VfsProcTabTransport::vfs_proc_tab_via(&canned, &mut tab),
            Ok(-minix_types::EINVAL)
        );
    }

    /// 宿主构建（未开 real-trap）下 DirectTrapTransport 诚实报错——
    /// 传输级失败走 `Err(Errno(EIO))`（TrapStatus 符号契约），trait 消费
    /// 面折算回 C 整型契约的 `-EIO`，不假装成功（KernelKmessTransport
    /// 同款门控验证）。
    #[test]
    fn test_vfs_proc_tab_transport_hosted_is_eio() {
        let mut t = VfsProcTabTransport;
        let mut tab = [FProcSnap::default(); 2];
        assert_eq!(t.vfs_proc_tab(&mut tab), -minix_types::EIO);
    }

    /// 未武装的四腿 fail-closed panic（装配错误，不是运行时错误）。
    #[test]
    #[should_panic(expected = "pm_proc_tab leg not armed")]
    fn test_unarmed_pm_leg_panics() {
        let mut t = VfsProcTabTransport;
        let mut tab = [MProcSnap::default(); 1];
        let _ = t.pm_proc_tab(&mut tab);
    }

    /// FakeAcquires 的 vfs_proc_tab 腿仍在（编排层测试不换装）——本模块
    /// 只验证生产 transport 自身；main 换装挂 E-ISWIRE(3)。
    #[test]
    fn test_fake_acquires_leg_unchanged() {
        let mut f = FakeAcquires::ok();
        let mut tab = [FProcSnap::default(); 2];
        assert_eq!(f.vfs_proc_tab(&mut tab), OK);
        assert_eq!(tab[1].fp_pid, 1);
    }
}
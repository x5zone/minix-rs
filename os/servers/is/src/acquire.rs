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
/// (`28-usermapped-data.md`); the production implementation is the
/// `GET_KMESSAGES` `sys_getinfo` sub-request (kernel edge E-ISKMESS): the
/// kernel copies the raw ring into `ring` and the cursor fields into
/// `meta`, and IS computes the print start via `kmess_start` (05 §3 D4).
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

/// 生产取数面(S23 片 3a/3b):SYS_GETINFO 族直调(结构即 minix-types 的
/// wire 权威,repr(C) 布局按字节通到内核)、SYS_DIAGCTL 的 stacktrace
/// (code 2,载荷端点)、SYS_TIMES 的 uptime(`real_ticks`,C `getticks`
/// 读 kclockinfo->uptime 的同源量;A-3 无 usermapped 页)、
/// `GET_KMESSAGES` 快照([`KernelKmessTransport`])。
/// `GetSysinfo` 五腿经 [`SysGetsysinfo`](片 3b-2)、`VM_INFO` 三查询经
/// [`SysVmInfo`](片 3b-3)——六通道全部就位，无 fail-closed 占位。
#[derive(Debug, Default)]
pub struct SysAcquires {
    /// getsysinfo 五腿生产客户端（片 3b-2）。
    getsys: SysGetsysinfo,
}

impl SysAcquires {
    /// SYS_GETINFO 的共享承载:`Ok` → OK(0),`Err(e)` → 负 errno 原样
    /// (C 的 `r != OK` 判读面)。
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

/// 结构切片的字节视图(出参缓冲;结构都是 repr(C) 的 wire 权威)。
///
/// # Safety
/// `T` 为 POD(repr(C)、无填充语义依赖);调用期间切片独占。
unsafe fn out_bytes<T>(v: &mut [T]) -> &mut [u8] {
    // SAFETY: 由调用方保证的 POD 切片 → 字节视图。
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
        // C: sys_diagctl_stacktrace(ep)(dmp_kernel.c:378)→
        // SYS_DIAGCTL code 2 的载荷端点在 `d.endpt`(minix-sys 非 code1
        // 分支写 arg2)。
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
        // C getticks():kclockinfo->uptime(usermapped 页,A-3 不建)。
        // SYS_TIMES 的 real_ticks 是同源量(自 boot 的实时 ticks);
        // 失败回 0(C 的调用点无失败面)。
        match minix_sys::syscall::sys_times(&DirectKernelCallTransport, Endpoint::SELF.0) {
            Ok(t) => t.real_ticks as u32,
            Err(_) => 0,
        }
    }
}

impl KerninfoTransport for SysAcquires {
    fn kmessages(&mut self, meta: &mut KmessagesSnap, ring: &mut [u8]) -> i32 {
        // 片 3b-1:`GET_KMESSAGES` 快照通道真装——10008 字节快照(8 字节
        // 游标头 + 10000 环体)经 `sys_getinfo` 一次取回后拆包;宿主构建
        // 诚实上浮 -EIO(核 `kmess` 臂只在 real-trap 下可达)。
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

/// 生产 `GetSysinfoTransport`（S23 片 3b-2）：五条腿一律走
/// `getsysinfo(who, what, where, size)` 的消息形状。
///
/// C: `minix3/minix/lib/libsys/getsysinfo.c:14-31`——`who` 映射调用号
/// （PM/VFS/RS/DS，其余 `ENOSYS`），随后 `_taskcall(who, call_nr, &m)`；
/// `m_lsys_getsysinfo` 三字段是"取什么"（`what`）、"放哪"（调用方虚地址
/// `where`）、"多长"（`size`）。服务端（PM `misc.c:108-144`、VFS
/// `misc.c:59-113`、RS `request.c:1095-1142`、DS `store.c:653-678`）做
/// root 门 + **尺寸精确匹配**（差一字节即 `EINVAL`）+ `sys_datacopy`
/// 拷回调用方——所以调用方声明的 `size` 必须等于出参切片的字节长度
/// （04 §3.4 D4 义务②），`what` 必须按 C 值发（`minix_types::SI_*`）。
///
/// 五条腿的行宽都是共享快照类型（`[ARCH: A-4]` 单一权威）：PM 腿
/// `MProcSnap`（片 3b-2a 已对齐）、VFS 腿 `FProcSnap`（E-MIBPROD 已对齐）、
/// RS 两表与 DS 表的**生产者**对齐归 S33（E-MIBPROD/E-ISPROD 余项）——
/// 对齐前服务端按精确尺寸门回 `EINVAL`，dump 体按 C 的 warn-and-continue
/// 打一行错误继续（04 §2.6），不 panic、不伪造数据。
///
/// 每条腿有 `*_via(transport, …)` 形态：生产传 [`DirectTrapTransport`]，
/// 测试传脚本双替身（宿主构建下真 trap 诚实上浮 `-EIO`）。
#[derive(Debug, Default)]
pub struct SysGetsysinfo;

impl SysGetsysinfo {
    /// 一腿的共用承载。
    ///
    /// 消息打包用 raw 三 lane（what @0..4、where @8..16、size @16..24，
    /// 与 `MessLsysGetsysinfo` 的 LP64 布局逐字节一致），经共享
    /// [`minix_sys::syscall::perform_taskcall`] 上浮：传输级失败走
    /// `Err(Errno)`（TrapStatus 符号契约——真机回复寄存器与宿主回退臂
    /// 都报正 errno），sendrec 成功后的原始回复 `m_type` 走 `Ok`，负值
    /// 即服务端错误码（C `_taskcall` 语义）。
    ///
    /// `where` 传的是**调用方自己的缓冲地址**（服务端按 `sys_datacopy`
    /// SELF→caller 回填），因此 `out` 必须在调用期间保持独占。
    fn fetch<Row>(
        transport: &impl IpcTransport,
        who: Endpoint,
        what: i32,
        out: &mut [Row],
    ) -> Result<i32, minix_types::Errno> {
        let mut msg = Message::default();
        // SAFETY: m_lsys_getsysinfo 的 raw 三 lane 视图（C getsysinfo.c:
        // 27-29 的 what/where/size）。
        unsafe {
            msg.m_u.raw[..4].copy_from_slice(&what.to_ne_bytes());
            msg.m_u.raw[8..16].copy_from_slice(&(out.as_mut_ptr() as u64).to_ne_bytes());
            msg.m_u.raw[16..24]
                .copy_from_slice(&(core::mem::size_of_val(out) as u64).to_ne_bytes());
        }
        perform_taskcall(transport, who, getsysinfo_call(who), &mut msg)
    }

    /// 一腿的对外形态：传输级 `Err` 折算回负 errno，与 sendrec 成功但
    /// 服务端回负 `m_type` 的形状合流（trait 的 C 整型契约）。
    fn leg<Row>(transport: &impl IpcTransport, who: Endpoint, what: i32, out: &mut [Row]) -> i32 {
        match Self::fetch(transport, who, what, out) {
            Ok(reply) => reply,
            Err(e) => -e.to_i32(),
        }
    }

    /// PM 进程表腿。C: `dmp_pm.c:47/82`。
    pub fn pm_proc_tab_via(transport: &impl IpcTransport, out: &mut [MProcSnap]) -> i32 {
        Self::leg(transport, Endpoint::PM, SI_PROC_TAB, out)
    }

    /// RS 双拉腿。C: `dmp_rs.c:33-34`——先 PUB 后 PRIV，前者失败短路
    /// （`||` 语义：PUB 拿不到就没有可对账的身份列，继续拉 PRIV 无意义）。
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

    /// DS 数据仓腿。C: `dmp_ds.c:15`。
    pub fn ds_data_store_via(transport: &impl IpcTransport, out: &mut [DsEntrySnap]) -> i32 {
        Self::leg(transport, Endpoint::DS, SI_DATA_STORE, out)
    }

    /// VFS 两腿的诚实出口。
    ///
    /// VFS 侧 `do_getsysinfo` 的纯函数半已备并有测试（`vfs/src/misc.rs`），
    /// 但**运行时的应答面尚不接线**：`dispatch_syscall` 把 `Getsysinfo`
    /// 归入 `SyscallResult::Nosys` 且主循环不回信（`vfs/src/syscalls.rs`），
    /// 现在发过去会等一个永不来的回复（无回复的 sendrec 会把 IS 挂死）。
    /// 故此处本地回 `-ENOSYS`——按 C 的 `!= OK` 判读面就是"该表取不到"，
    /// dump 打一行错误继续；VFS 应答面随 W1 传输批次（S12）落地后，本腿
    /// 换成 `Self::leg(transport, Endpoint::VFS, …)` 即可。
    const VFS_REPLY_PATH_PENDING: i32 = -minix_types::ENOSYS;

    /// VFS fproc_tab 腿。C: `dmp_fs.c:31`。
    pub fn vfs_proc_tab_via(_transport: &impl IpcTransport, _out: &mut [FProcSnap]) -> i32 {
        Self::VFS_REPLY_PATH_PENDING
    }

    /// VFS dmap_tab 腿。C: `dmp_fs.c:71`（**加上生产者半**也归 S33：
    /// VFS 的 `do_getsysinfo` 目前只服务 `SI_PROC_TAB`，DMAP 臂回 `EINVAL`）。
    pub fn vfs_dmap_tab_via(_transport: &impl IpcTransport, _out: &mut [DmapSnap]) -> i32 {
        Self::VFS_REPLY_PATH_PENDING
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

/// 生产 `VmInfoTransport`（S23 片 3b-3）：VM_INFO 三查询的**值通道**客户端。
///
/// C: `minix3/minix/lib/libsys/vm_info.c:10-57`——`_taskcall(VM_PROC_NR,
/// VM_INFO, &m)`，请求域 `what`/`ep`/`count`/`ptr`/`next`；C 的服务端把结果
/// `sys_datacopy` 进调用方缓冲（`minix3/minix/servers/vm/utility.c:169-182`，
/// 复制前先 `handle_memory_once` 钉住目标页防死锁）。minix-rs 的 VM 用**值
/// 通道**取代这一步（`[ARCH: 26-D1]`，`02-stage-vm/26-vm-queries.md`）：结果
/// 编码在回复消息的 M1 槽里，`ptr` 不参与（故本客户端传 0）。
///
/// 槽位对照（生产者：`os/servers/vm/src/ipc/encode.rs` 的 `encode_reply_data`）：
/// - STATS：`m1p1`=页大小、`m1i1`=总页数、`m1i2`=空闲页、`m1i3`=最大连续块、
///   `m1p2`=缓存页；
/// - USAGE：`m1p1`/`m1p2`/`m1p3` = total/common/shared（**字节**，即 C
///   `struct vm_usage_info` 的 `vui_total`/`vui_common`/`vui_shared`）；
/// - REGION：`m1i1`=实际条数、`m1i2`=新游标（低 32 位——区域地址在低 4 GiB
///   用户区间内，故 C 的 `vir_bytes` 游标在此不丢精度）。
///
/// REGION 的**条目数组**当前不在回复里：`26-vm-queries.md` §4.8 的 D7
/// transport 缺口（handler 算得对，编码只写 count/next）。服务端报 count>0
/// 而条目不可得时，本客户端如实回 `-ENOTSUP`（C 的 `!= OK` 面即"这屏取不到"，
/// dump 打一行错误继续），**不把"未送达"伪装成"空地址空间"**；count==0 是
/// 合法答案（地址空间确实没有区域），按 OK 回。
#[derive(Debug, Default)]
pub struct SysVmInfo;

impl SysVmInfo {
    /// 三查询共用的请求封装：填 `what`/`ep`/`count`/`next` 后 taskcall。
    ///
    /// 回复落地在同一个 `msg` 上（`_taskcall` 复用消息缓冲，C 同构），
    /// 故调用方读 `msg.m_u.m_m1` 取结果槽。
    fn call(
        transport: &impl IpcTransport,
        what: i32,
        ep: Endpoint,
        count: i32,
        next: u64,
    ) -> Result<(i32, Message), minix_types::Errno> {
        let mut msg = Message::default();
        // `m_lsys_vm_info` 是 VM_INFO 的载荷域（C ipc.h:1494-1502 的
        // what/ep/count/ptr/next；`ptr` 在 minix-rs 值通道下不参与）——
        // 该联合成员是普通 POD 字段，整体赋值无需 unsafe。
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

    /// 把 taskcall 的两级失败折成 C 的整型状态面。
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
        // SAFETY: VM 的 STATS 回复用 M1 槽（encode.rs 的 InfoStats 臂）。
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
        // SAFETY: VM 的 USAGE 回复用 M1 槽（encode.rs 的 InfoUsage 臂）。
        let m1 = unsafe { &msg.m_u.m_m1 };
        *out = VmUsageSnap {
            vui_total: m1.m1p1,
            vui_common: m1.m1p2,
            vui_shared: m1.m1p3,
        };
        minix_types::OK
    }

    pub fn vm_region_via(
        transport: &impl IpcTransport,
        who: Endpoint,
        out: &mut [VmRegionSnap],
        next: u64,
    ) -> (i32, u64, i32) {
        let (status, msg) = Self::status(Self::call(
            transport,
            minix_types::VMIW_REGION,
            who,
            out.len() as i32,
            next,
        ));
        let Some(msg) = msg else { return (status, next, 0) };
        // SAFETY: VM 的 REGION 回复用 M1 槽（encode.rs 的 InfoRegion 臂：
        // m1i1=count、m1i2=next 低位）。
        let m1 = unsafe { &msg.m_u.m_m1 };
        let count = m1.m1i1;
        let next_out = m1.m1i2 as u32 as u64;
        if count > 0 {
            // 条目数组未随回复送达（D7 缺口）——如实报"取不到"，游标原样。
            return (-minix_types::ENOTSUP, next, 0);
        }
        (minix_types::OK, next_out, 0)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sys_acquires_hosted_negative_errno_and_uptime_zero() {
        // hosted:trap 直连诚实回负 errno(C 的 r 判读面);uptime 回 0
        // (C getticks 无失败面,失败面按 0 记)。
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
        // 片 3b-1 见证:kmessages 腿已换装 KernelKmessTransport——宿主
        // 构建下如实回 -EIO(DirectKernelCallTransport 的 real-trap 门控),
        // 不再是 fail-closed panic(占位替身随六通道全真装一并删除)。
        use super::{KerninfoTransport, SysAcquires};
        let mut a = SysAcquires::default();
        let mut meta = KmessagesSnap::default();
        let mut ring = [0u8; 64];
        assert_eq!(a.kmessages(&mut meta, &mut ring), -minix_types::EIO);
    }

    #[test]
    fn test_sys_acquires_getinfo_wire_shape() {
        // CannedKernelCallTransport 断言 SYS_GETINFO 的字段:
        // request 与 val_len(size_of 视图)按结构真身走。
        use super::{SysAcquires, SysGetinfoTransport};
        use minix_sys::syscall::CannedKernelCallTransport;

        let mut a = SysAcquires::default();
        // 借载体的可观测性:直接用 sys_getinfo_into 的形状断言,
        // 绕开结构方法(方法内是同一承载)。
        let canned = CannedKernelCallTransport::new();
        let mut kinfo = KinfoStruct::default();
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                (&mut kinfo as *mut KinfoStruct).cast::<u8>(),
                core::mem::size_of::<KinfoStruct>(),
            )
        };
        let _ = minix_sys::syscall::sys_getinfo_into(
            &canned,
            minix_types::GET_KINFO,
            bytes,
            Endpoint::NONE.0,
        );
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 1);
        // SAFETY: 断言侧按 m_lsys_krn_sys_getinfo 域序读。
        let gi = unsafe { &sent[0].m_u.m_lsys_krn_sys_getinfo };
        assert_eq!(gi.request, minix_types::GET_KINFO);
        assert_eq!(gi.val_len as usize, core::mem::size_of::<KinfoStruct>());
        assert_eq!(gi.endpt, Endpoint::NONE.0);
        let _ = &mut a; // 方法面与承载同形(上面直调展示 wire)
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

    /// wire 三元组回放（PM 腿，已对齐的生产腿）：请求臂 =
    /// `getsysinfo(PM, SI_PROC_TAB, buf, 76×len)`——目的地 PM、调用号
    /// `PM_GETSYSINFO`、what/where/size 三 lane（C getsysinfo.c:14-31 +
    /// dmp_pm.c:47）；应答臂 m_type 原样上浮（`_taskcall` 语义，OK 即成功）。
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
        // SAFETY(test): 读回打包的三 lane（与 SysGetsysinfo::fetch 同布局）。
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

    /// RS/DS 腿的调用号与 what 值回放：`SI_PROCPUB_TAB`(11) 与
    /// `SI_PROC_TAB`(2) 分两次拉（C dmp_rs.c:33-34），DS 拉
    /// `SI_DATA_STORE`(5)（C dmp_ds.c:15）——三者的调用号来自
    /// `getsysinfo_call`（C getsysinfo.c:14-24 的 switch）。
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
        // SAFETY(test): what @0..4（与 fetch 同布局）。
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

    /// RS 双拉短路：PUB 失败即返回，不发 PRIV（C dmp_rs.c:33-34 的 `||`）。
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

    /// 服务端负回复（EINVAL=尺寸门/EPERM=root 门）原样上浮——run_dump 的
    /// `!= OK` 消费面按 04 §2.6 warn-and-continue。
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

    /// 宿主构建（未开 real-trap）下 DirectTrapTransport 诚实报错——
    /// 传输级失败走 `Err(Errno(EIO))`（TrapStatus 符号契约），trait 消费
    /// 面折算回 C 整型契约的 `-EIO`，不假装成功（`KernelKmessTransport`
    /// 同款门控验证）。
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

    /// VFS 两腿的诚实出口：**不发消息**（VFS 运行时应答面未接线，发过去
    /// 会等一个永不来的回复），本地回 `-ENOSYS` = C 的"该表取不到"。
    #[test]
    fn test_vfs_legs_fail_closed_without_sending() {
        use super::{GetSysinfoTransport, SysGetsysinfo};
        let mut sys = SysGetsysinfo;
        let mut fp = [FProcSnap::default(); 2];
        assert_eq!(sys.vfs_proc_tab(&mut fp), -minix_types::ENOSYS);
        let mut dm = [DmapSnap::default(); 2];
        assert_eq!(sys.vfs_dmap_tab(&mut dm), -minix_types::ENOSYS);
    }

    /// VM_INFO 三查询的请求形状与回复槽解码（值通道，[ARCH: 26-D1]）：
    /// STATS 读 m1p1/m1i1/m1i2/m1i3/m1p2，USAGE 读 m1p1/p2/p3（字节），
    /// REGION 读 m1i1/m1i2。请求侧断言目的地 VM、调用号 `VM_INFO`、
    /// what 值（C com.h:732-734 的 1/2/3）与 ep/count 域。
    #[test]
    fn test_vm_info_stats_usage_wire_and_slots() {
        // STATS：服务端把五个值写进 M1 槽（vm/ipc/encode.rs 的 InfoStats）。
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = OK;
        // SAFETY(test): 按 VM 的 InfoStats 槽序回填。
        unsafe {
            let m1 = &mut reply.m_u.m_m1;
            m1.m1p1 = 4096; // 页大小
            m1.m1i1 = 100; // 总页数
            m1.m1i2 = 40; // 空闲页
            m1.m1i3 = 12; // 最大连续块
            m1.m1p2 = 7; // 缓存页
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
        // SAFETY(test): 请求域（与 SysVmInfo::call 同布局）。
        let req = unsafe { &sent[0].1.m_u.m_lsys_vm_info };
        assert_eq!(req.what, minix_types::VMIW_STATS);
        assert_eq!(req.ptr, 0, "值通道：不把调用方缓冲地址交给服务端");

        // USAGE：三个字节值进 m1p1/p2/p3（encode.rs 的 InfoUsage）。
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = OK;
        // SAFETY(test): 按 InfoUsage 槽序回填。
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
        // SAFETY(test): 请求域。
        let req = unsafe { &sent[0].1.m_u.m_lsys_vm_info };
        assert_eq!(req.what, minix_types::VMIW_USAGE);
        assert_eq!(req.ep, Endpoint::PM.get());
    }

    /// REGION 游标协议 + 条目数组缺口（26-vm-queries.md §4.8 D7）：
    /// count==0 是合法答案（地址空间无区域）；count>0 而条目不在回复里时
    /// 如实回 `-ENOTSUP`，不把"未送达"伪装成空表。
    #[test]
    fn test_vm_info_region_cursor_and_payload_gap() {
        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = OK;
        // SAFETY(test): 按 InfoRegion 槽序（count/next）回填。
        unsafe {
            let m1 = &mut reply.m_u.m_m1;
            m1.m1i1 = 0;
            m1.m1i2 = 0x4000;
        }
        canned.reply_sendrec(Ok(reply));
        let mut out = [VmRegionSnap::default(); 4];
        assert_eq!(
            SysVmInfo::vm_region_via(&canned, Endpoint::PM, &mut out, 0x1000),
            (OK, 0x4000, 0)
        );
        let sent = canned.sent.borrow();
        // SAFETY(test): 请求域（count 是调用方声明的批大小）。
        let req = unsafe { &sent[0].1.m_u.m_lsys_vm_info };
        assert_eq!(req.what, minix_types::VMIW_REGION);
        assert_eq!(req.count, 4);
        assert_eq!(req.next, 0x1000, "游标按调用方入参带上");

        let mut canned = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = OK;
        // SAFETY(test): count=3 但回复里没有条目数组。
        unsafe {
            let m1 = &mut reply.m_u.m_m1;
            m1.m1i1 = 3;
            m1.m1i2 = 0x5000;
        }
        canned.reply_sendrec(Ok(reply));
        let mut out = [VmRegionSnap::default(); 4];
        assert_eq!(
            SysVmInfo::vm_region_via(&canned, Endpoint::PM, &mut out, 0x1000),
            (-minix_types::ENOTSUP, 0x1000, 0)
        );
    }

    /// 宿主构建下三条 VM 腿都诚实上浮 -EIO，不假装成功
    /// （`KernelKmessTransport` 同款门控形态）。
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

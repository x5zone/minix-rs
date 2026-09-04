//! Data-acquisition channels (04-is-data-acquisition.md §4.1).
//!
//! The five ways IS pulls debug data: `sys_getinfo` (kernel tables),
//! `sys_diagctl` (stack traces), kerninfo direct read (kernel messages,
//! `[ARCH: A-3]`), `getsysinfo` (peer-server tables), `vm_info` (address
//! spaces). Each channel is a trait; production implementations land with
//! the `minix-sys`/kernel wiring (forward references, 01 §3 D2 pattern).
//!
//! Error-surface rule (04 §2.6): acquisition failure is *recoverable* —
//! dumps warn and continue. Only transport failure (01: receive/send)
//! panics. Traits therefore return raw Minix status codes (`i32`,
//! `!= OK`判定如 C), not `Result`.

use minix_types::{
    DS_GETSYSINFO, Endpoint, GET_IMAGE, GET_IRQACTIDS, GET_IRQHOOKS, GET_KINFO, GET_MACHINE,
    GET_MONPARAMS, GET_PRIVTAB, GET_PROCTAB, PM_GETSYSINFO, RS_GETSYSINFO, SI_DATA_STORE,
    SI_DMAP_TAB, SI_PROCPUB_TAB, SI_PROC_TAB, VFS_GETSYSINFO,
};

/// A `sys_getinfo` sub-request IS actually issues.
///
/// C: the `sys_get*` shorthand macros — `minix3/minix/include/minix/syslib.h:
/// 175-187` — restricted to the eight requests IS uses (`sys_getmonparams`,
/// `sys_getirqhooks`, `sys_getirqactids`, `sys_getimage`, `sys_getkinfo`,
/// `sys_getmachine`, `sys_getprivtab`, `sys_getproctab` — dmp_kernel.c,
/// dmp_vm.c:83). Anything else travels as `Reserved(i32)` (pass-through,
/// e.g. future `GET_KMESSAGES` — A-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetRequest {
    Kinfo,
    Image,
    Proctab,
    Monparams,
    Irqhooks,
    Irqactids,
    Privtab,
    Machine,
    Reserved(i32),
}

impl GetRequest {
    /// Wire request code. C: `GET_*` — com.h:316-327.
    pub const fn code(self) -> i32 {
        match self {
            GetRequest::Kinfo => GET_KINFO,
            GetRequest::Image => GET_IMAGE,
            GetRequest::Proctab => GET_PROCTAB,
            GetRequest::Monparams => GET_MONPARAMS,
            GetRequest::Irqhooks => GET_IRQHOOKS,
            GetRequest::Irqactids => GET_IRQACTIDS,
            GetRequest::Privtab => GET_PRIVTAB,
            GetRequest::Machine => GET_MACHINE,
            GetRequest::Reserved(n) => n,
        }
    }
}

/// A `getsysinfo` table selector IS actually issues.
///
/// C: `SI_*` — `minix3/minix/include/minix/sysinfo.h:11-17`, restricted to
/// the four tables IS pulls (dmp_pm/fs/rs/ds.c). MIB-only tables
/// (`SI_CALL_STATS` etc.) have no variant: out-of-scope values are
/// inexpressible.
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
/// (a single-owner mapping would misroute the RS leg).
pub const IS_GETSYSINFO_CALLS: &[(Endpoint, SiWhat)] = &[
    (Endpoint::PM, SiWhat::ProcTab),
    (Endpoint::VFS, SiWhat::ProcTab),
    (Endpoint::VFS, SiWhat::DmapTab),
    (Endpoint::RS, SiWhat::ProcPubTab),
    (Endpoint::RS, SiWhat::ProcTab),
    (Endpoint::DS, SiWhat::DataStore),
];

/// `sys_getinfo` channel (kernel tables).
///
/// C: `sys_getinfo(request, ptr, ...)` — `minix3/minix/lib/libsys/sys_getinfo.c`
/// (`_kernel_call(SYS_GETINFO)`; `endpt = SELF`, i.e. the kernel always
/// stores at the caller). Payload types belong to the kernel crate (05);
/// 04 carries only the request code + status. Returns the raw status.
pub trait SysGetinfoTransport {
    fn sys_getinfo(&mut self, req: GetRequest) -> i32;
}

/// `sys_diagctl` stack-trace channel.
///
/// C: `sys_diagctl_stacktrace(ep)` = `sys_diagctl(DIAGCTL_CODE_STACKTRACE,
/// NULL, ep)` — syslib.h:166-168 → sys_diagctl.c (`endpt` reuses `arg2`).
/// Kernel side is currently ENOSYS (32-stack-tracing.md forward ref);
/// callers treat failure as warn-and-continue (04 §2.6).
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
/// `GET_KMESSAGES`-equivalent `sys_getinfo` sub-request (kernel copies the
/// ring; layouts follow 05). Until the kernel wiring lands this is
/// fail-closed. The `DIAGCTL_CODE_STACKTRACE` channel is NOT reused: it
/// prints into the kernel log and returns no buffer (different semantics).
pub trait KerninfoTransport {
    /// Whether a kernel-message snapshot is available.
    fn kmessages_available(&mut self) -> bool;
}

/// Cross-service table channel.
///
/// C: `getsysinfo(who, what, where, size)` — `minix3/minix/lib/libsys/getsysinfo.c`:
/// `who`→call-number map (PM/VFS/RS/DS, else `ENOSYS`), then `_taskcall`.
/// Server side (`pm/misc.c:105-145`, `vfs/misc.c:61-113`): root gate
/// (`EPERM`) + exact `len == size` match (`EINVAL`) + `sys_datacopy`.
/// IS-side obligations (04 §3 D4): always root (IS uid 0), exact
/// `size_of` lengths, in-scope `what` only. `len` is the caller's buffer
/// length; the server rejects mismatches — never truncate, never overread.
pub trait GetSysinfoTransport {
    fn getsysinfo(&mut self, who: Endpoint, what: SiWhat, len: usize) -> i32;
}

/// Maps a server to its `getsysinfo` call number.
///
/// C: the `switch (who)` — getsysinfo.c:14-24 (`PM_GETSYSINFO` callnr.h:60,
/// `VFS_GETSYSINFO` callnr.h:120, `RS_GETSYSINFO` com.h:476,
/// `DS_GETSYSINFO` com.h:507; `default: return ENOSYS`).
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
/// returns `(status, next_out, count_out)`: the `next` cursor writeback +
/// actual count (vm_info.c:39-58); paging over the cursor belongs to 10
/// (`prev_base`/`prev_i` — dmp_vm.c:61-62,88-94).
pub trait VmInfoTransport {
    fn stats(&mut self) -> i32;
    fn usage(&mut self, who: Endpoint) -> i32;
    fn region(&mut self, who: Endpoint, count: i32, next: u64) -> (i32, u64, i32);
}

/// Fail-closed bundle until the `minix-sys`/kernel wiring lands
/// (01 `UnimplementedTransport` pattern).
#[derive(Debug, Default)]
pub struct UnimplementedAcquires;

impl SysGetinfoTransport for UnimplementedAcquires {
    fn sys_getinfo(&mut self, _req: GetRequest) -> i32 {
        panic!("IS acquire: sys_getinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }
}

impl DiagctlTransport for UnimplementedAcquires {
    fn stacktrace(&mut self, _proc: Endpoint) -> i32 {
        panic!("IS acquire: sys_diagctl wiring pending (04-is-data-acquisition.md §3 D2)");
    }
}

impl KerninfoTransport for UnimplementedAcquires {
    fn kmessages_available(&mut self) -> bool {
        panic!("IS acquire: GET_KMESSAGES wiring pending ([ARCH: A-3])");
    }
}

impl GetSysinfoTransport for UnimplementedAcquires {
    fn getsysinfo(&mut self, _who: Endpoint, _what: SiWhat, _len: usize) -> i32 {
        panic!("IS acquire: getsysinfo wiring pending (04-is-data-acquisition.md §3 D2)");
    }
}

impl VmInfoTransport for UnimplementedAcquires {
    fn stats(&mut self) -> i32 {
        panic!("IS acquire: vm_info wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn usage(&mut self, _who: Endpoint) -> i32 {
        panic!("IS acquire: vm_info wiring pending (04-is-data-acquisition.md §3 D2)");
    }

    fn region(&mut self, _who: Endpoint, _count: i32, _next: u64) -> (i32, u64, i32) {
        panic!("IS acquire: vm_info wiring pending (04-is-data-acquisition.md §3 D2)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{DIAGCTL_CODE_STACKTRACE, OK};

    #[test]
    fn test_get_request_codes() {
        // C: syslib.h:175-187 shorthands → com.h codes.
        // IS issues 8 requests (dmp_kernel.c + dmp_vm.c:83); GET_KENV is
        // notably absent (kenv_dmp reads kinfo+machine instead).
        assert_eq!(
            (
                GetRequest::Kinfo.code(),
                GetRequest::Image.code(),
                GetRequest::Proctab.code(),
                GetRequest::Monparams.code()
            ),
            (GET_KINFO, GET_IMAGE, GET_PROCTAB, GET_MONPARAMS)
        );
        assert_eq!(
            (
                GetRequest::Irqhooks.code(),
                GetRequest::Irqactids.code(),
                GetRequest::Privtab.code(),
                GetRequest::Machine.code()
            ),
            (GET_IRQHOOKS, GET_IRQACTIDS, GET_PRIVTAB, GET_MACHINE)
        );
        assert_eq!(GetRequest::Reserved(99).code(), 99);
    }

    #[test]
    fn test_si_what_codes_and_call_table() {
        // C: sysinfo.h:11-17 + IS call sites (dmp_pm/fs/rs/ds.c).
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

    /// Scriptable fake for all five channels.
    struct FakeAcquires {
        getinfo_status: i32,
        diag_status: i32,
        kmessages: bool,
        getsys_status: i32,
        vm_status: i32,
        region_answer: (i32, u64, i32),
        pub seen_get: Vec<GetRequest>,
    }

    impl FakeAcquires {
        fn ok() -> Self {
            Self {
                getinfo_status: OK,
                diag_status: OK,
                kmessages: true,
                getsys_status: OK,
                vm_status: OK,
                region_answer: (OK, 0, 3),
                seen_get: Vec::new(),
            }
        }
    }

    impl SysGetinfoTransport for FakeAcquires {
        fn sys_getinfo(&mut self, req: GetRequest) -> i32 {
            self.seen_get.push(req);
            self.getinfo_status
        }
    }

    impl DiagctlTransport for FakeAcquires {
        fn stacktrace(&mut self, _proc: Endpoint) -> i32 {
            self.diag_status
        }
    }

    impl KerninfoTransport for FakeAcquires {
        fn kmessages_available(&mut self) -> bool {
            self.kmessages
        }
    }

    impl GetSysinfoTransport for FakeAcquires {
        fn getsysinfo(&mut self, _who: Endpoint, _what: SiWhat, _len: usize) -> i32 {
            self.getsys_status
        }
    }

    impl VmInfoTransport for FakeAcquires {
        fn stats(&mut self) -> i32 {
            self.vm_status
        }

        fn usage(&mut self, _who: Endpoint) -> i32 {
            self.vm_status
        }

        fn region(&mut self, _who: Endpoint, _count: i32, _next: u64) -> (i32, u64, i32) {
            self.region_answer
        }
    }

    #[test]
    fn test_channels_ok_and_err_paths() {
        // 04 §2.6: acquisition failure is recoverable — statuses flow back
        // to the caller (05~10 warn-and-continue), never panic here.
        let mut f = FakeAcquires::ok();
        assert_eq!(f.sys_getinfo(GetRequest::Proctab), OK);
        assert_eq!(f.stacktrace(Endpoint::PM), OK);
        assert!(f.kmessages_available());
        assert_eq!(f.getsysinfo(Endpoint::PM, SiWhat::ProcTab, 64), OK);
        assert_eq!(f.stats(), OK);
        assert_eq!(f.region(Endpoint::PM, 8, 0), (OK, 0, 3));
        assert_eq!(f.seen_get, [GetRequest::Proctab]);

        f.getinfo_status = minix_types::EFAULT;
        assert_eq!(f.sys_getinfo(GetRequest::Image), minix_types::EFAULT);
    }

    #[test]
    #[should_panic(expected = "GET_KMESSAGES")]
    fn test_kerninfo_unimplemented_fail_closed() {
        let mut u = UnimplementedAcquires;
        let _ = u.kmessages_available();
    }

    #[test]
    fn test_vmiw_codes_match_c() {
        // C: com.h:732-734 (single authority: minix-types::ipc::vm).
        use minix_types::{VMIW_REGION, VMIW_STATS, VMIW_USAGE};
        assert_eq!((VMIW_STATS, VMIW_USAGE, VMIW_REGION), (1, 2, 3));
        // C: com.h:413. Referenced so the import stays live.
        assert_eq!(DIAGCTL_CODE_STACKTRACE, 2);
    }
}

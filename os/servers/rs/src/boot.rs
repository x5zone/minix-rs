//! Boot initialization — the 4-step boot state machine.
//!
//! Mirrors `sef_cb_init_fresh()` (`minix3/minix/servers/rs/main.c:158-494`):
//! the RS server's fresh-boot initialization. The four steps are strictly
//! ordered — each step only relies on facilities established by the previous
//! step (see 01-rs-boot-init.md §1.1):
//!
//! 1. **Step 1** (`main.c:244-346`): establish priv/sys/dev attributes for
//!    every boot service in the local slot table.
//! 2. **Step 2** (`main.c:348-399`): allow services to run (`sched_init_proc`
//!    + `SYS_PRIV_ALLOW` + `init_service`; RS/VM exception).
//! 3. **Step 3** (`main.c:401-407`): catch all remaining init-ready messages.
//! 4. **Step 4** (`main.c:409-433`): `getnpid` for every service +
//!    `sys_setalarm(RS_DELTA_T)`.
//!
//! The C code touches global state (`rproc[]`/`rprocpub[]`, `rinit`, `rupdate`)
//! directly. Rust keeps the same state inside [`BootInit`], making the
//! dependencies explicit and the machine testable with a mock kernel API.
//!
//! # Kernel API boundary
//!
//! All kernel interactions go through [`KernelApi`]. The production impl is
//! wired to `minix-sys` in 19-rs-external-interfaces.md (currently
//! DEFERRED — `minix-sys` is a stub); tests use `MockKernelApi`.

use minix_types::{BootImage, ENOSYS, Endpoint, Pid};

use crate::privilege::{CallMask, PrivFlags};
use crate::process_table::RProcTable;
use crate::service_slot::SlotId;

use crate::table::{
    BOOT_IMAGE_DEV_TABLE, BOOT_IMAGE_PRIV_TABLE, BOOT_IMAGE_SYS_TABLE, BootImageDev, BootImagePriv,
    BootImageSys, DEFAULT_DEV, DEFAULT_SYS,
};

/// Machine information. C: `struct machine` — `minix3/minix/include/minix/type.h:122-131`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Machine {
    /// How many cpus are available. C: `machine.processors_count`.
    pub processors_count: u32,
    /// Id of the bootstrap cpu. C: `machine.bsp_id`.
    pub bsp_id: u32,
}

// Privilege types live in `privilege` (03-rs-privilege.md): `PrivCtlOp` is the
// full 11-opcode enum (com.h:342-353), `Privilege` is the modeled `struct priv`.
pub use crate::privilege::{PrivCtlOp, Privilege};

/// Kernel API surface consumed by the boot module.
///
/// Each method corresponds to a C call site:
///
/// | Method | C call site |
/// |--------|-------------|
/// | [`KernelApi::get_machine`] | `sys_getmachine` — main.c:53 |
/// | [`KernelApi::get_hz`] | `sys_getinfo(GET_HZ, ...)` — main.c:181 |
/// | [`KernelApi::privctl`] | `sys_privctl` — main.c:287/379 (boot) |
/// | [`KernelApi::getpriv`] | `sys_getpriv` — main.c:294 |
/// | [`KernelApi::sched_init_proc`] | `sched_init_proc` — main.c:376 |
/// | [`KernelApi::getnuid`] | `getnuid` (PM_GETEPINFO) — manager.c:29 (04) |
/// | [`KernelApi::getnpid`] | `getnpid` — main.c:426 |
/// | [`KernelApi::setalarm`] | `sys_setalarm(RS_DELTA_T, 0)` — main.c:433 |
///
/// Errors are errno values (`minix-types` constants). Production wiring:
/// 19-rs-external-interfaces.md (DEFERRED — `minix-sys` is a stub).
pub trait KernelApi {
    fn get_machine(&mut self) -> Result<Machine, i32>;
    fn get_hz(&mut self) -> Result<u32, i32>;
    fn privctl(
        &mut self,
        proc: Endpoint,
        op: PrivCtlOp,
        priv_: Option<&Privilege>,
    ) -> Result<(), i32>;
    fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, i32>;
    fn sched_init_proc(&mut self, proc: Endpoint) -> Result<(), i32>;
    fn getnuid(&mut self, proc: Endpoint) -> Result<u32, i32>;
    fn getnpid(&mut self, proc: Endpoint) -> Result<i32, i32>;
    fn setalarm(&mut self, delay_ticks: u32) -> Result<(), i32>;

    /// Forks a child service process.
    ///
    /// C: `srv_fork(uid, 0)` — manager.c:576; `PM_SRV_FORK` message via
    /// `_taskcall(PM_PROC_NR, ...)` (lib/libsys/srv_fork.c). ARCH A-1: the
    /// no_std server has no libc `fork`; the external behavior (child created
    /// by PM) is preserved through the PM message face. Wired 19.
    fn srv_fork(&mut self, _uid: u32, _gid: u32) -> Result<Pid, i32> {
        unimplemented!("srv_fork (PM_SRV_FORK) wiring pending (19-rs-external-interfaces.md)")
    }

    /// Resolves a pid to an endpoint.
    ///
    /// C: `getprocnr(pid, &endpoint)` — manager.c:584; PM_GETEPINFO.
    /// Wired 19.
    fn getprocnr(&mut self, _pid: Pid) -> Result<Endpoint, i32> {
        unimplemented!("getprocnr wiring pending (19-rs-external-interfaces.md)")
    }

    /// RS memory control on a process (VM).
    ///
    /// C: `vm_memctl(ep, VM_RS_MEM_*, ...)` — manager.c:604, 612, 628, 636,
    /// 663, 680, 693. Wired 19.
    fn vm_memctl(
        &mut self,
        _proc: Endpoint,
        _req: VmRsMemReq,
        _a: usize,
        _b: usize,
    ) -> Result<(), i32> {
        unimplemented!("vm_memctl wiring pending (19-rs-external-interfaces.md)")
    }

    /// Sets the VM call mask of a process.
    ///
    /// C: `vm_set_priv(ep, &vm_call_mask[0], TRUE)` — manager.c:698. Wired 19.
    fn vm_set_priv(
        &mut self,
        _proc: Endpoint,
        _vm_call_mask: CallMask,
        _allow: bool,
    ) -> Result<(), i32> {
        unimplemented!("vm_set_priv wiring pending (19-rs-external-interfaces.md)")
    }
}

/// VM RS-memory-control requests.
///
/// C: `VM_RS_MEM_*` — `minix3/minix/include/minix/com.h:741-745`.
/// 10/16 use these; `HeapPrealloc`/`MapPrealloc`/`GetPreallocMap` are
/// live-update faces (16-rs-live-update.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmRsMemReq {
    /// Pin process memory. C: `VM_RS_MEM_PIN` — com.h:741.
    Pin = 0,
    /// Make a VM instance. C: `VM_RS_MEM_MAKE_VM` — com.h:742.
    MakeVm = 1,
    /// Preallocate heap regions. C: `VM_RS_MEM_HEAP_PREALLOC` — com.h:743.
    HeapPrealloc = 2,
    /// Preallocate mmapped regions. C: `VM_RS_MEM_MAP_PREALLOC` — com.h:744.
    MapPrealloc = 3,
    /// Get preallocated mmapped regions. C: `VM_RS_MEM_GET_PREALLOC_MAP` — com.h:745.
    GetPreallocMap = 4,
}

/// Fail-closed kernel API: panics on any call.
///
/// Selected until the `minix-sys` wiring lands (19-rs-external-interfaces.md).
/// A silent `Err` would mask missing syscall implementations; an explicit
/// panic keeps the failure visible (ARCH A-12: panic retains fail-closed
/// semantics).
pub struct UnimplementedKernelApi;

impl KernelApi for UnimplementedKernelApi {
    fn get_machine(&mut self) -> Result<Machine, i32> {
        unimplemented!("sys_getmachine wiring pending (19-rs-external-interfaces.md)")
    }
    fn get_hz(&mut self) -> Result<u32, i32> {
        unimplemented!("sys_getinfo(GET_HZ) wiring pending")
    }
    fn privctl(
        &mut self,
        _proc: Endpoint,
        _op: PrivCtlOp,
        _priv_: Option<&Privilege>,
    ) -> Result<(), i32> {
        unimplemented!("sys_privctl wiring pending")
    }
    fn getpriv(&mut self, _proc: Endpoint) -> Result<Privilege, i32> {
        unimplemented!("sys_getpriv wiring pending")
    }
    fn sched_init_proc(&mut self, _proc: Endpoint) -> Result<(), i32> {
        unimplemented!("sched_init_proc wiring pending")
    }
    fn getnuid(&mut self, _proc: Endpoint) -> Result<u32, i32> {
        unimplemented!("getnuid wiring pending")
    }
    fn getnpid(&mut self, _proc: Endpoint) -> Result<i32, i32> {
        unimplemented!("getnpid wiring pending")
    }
    fn setalarm(&mut self, _delay_ticks: u32) -> Result<(), i32> {
        unimplemented!("sys_setalarm wiring pending")
    }
}

/// Boot image + boot tables bundle.
///
/// C: `sys_getimage()` copy of the kernel `boot_image[]` (main.c:196) plus the
/// RS-owned priv/sys/dev tables (table.c). The image is injected at
/// construction (ARCH A-13); the three tables are `'static` constants.
#[derive(Debug, Clone, Copy)]
pub struct BootTables<'a> {
    /// C: `boot_image[]` copied by `sys_getimage` — main.c:196.
    pub image: &'a [BootImage],
    /// C: `boot_image_priv_table` — table.c:15-30.
    pub priv_table: &'static [BootImagePriv],
    /// C: `boot_image_sys_table` — table.c:33-42.
    pub sys_table: &'static [BootImageSys],
    /// C: `boot_image_dev_table` — table.c:45-50.
    pub dev_table: &'static [BootImageDev],
}

impl<'a> BootTables<'a> {
    /// Boot tables with the RS-owned static tables and the given image.
    pub const fn new(image: &'a [BootImage]) -> Self {
        Self {
            image,
            priv_table: BOOT_IMAGE_PRIV_TABLE,
            sys_table: BOOT_IMAGE_SYS_TABLE,
            dev_table: BOOT_IMAGE_DEV_TABLE,
        }
    }

    /// Placeholder used by `main.rs` until `sys_getimage` wiring lands.
    ///
    /// Mirrors VM's `BootParams::placeholder()` strategy
    /// (02-stage-vm/01-vm-init-main.md §4.1): a minimal but valid boot image
    /// so the production binary has a legal boot input.
    pub fn placeholder() -> BootTables<'static> {
        BootTables::new(BOOT_IMAGE_PLACEHOLDER)
    }

    /// Validates that image and priv tables describe the same system services.
    ///
    /// C: `nr_image_srvs != nr_image_priv_srvs → panic` — main.c:225-227.
    /// Kernel tasks (negative endpoint slots) are excluded on both sides.
    pub fn validate_tables(&self) -> Result<(), i32> {
        let image_srvs = self
            .image
            .iter()
            .filter(|ip| !ip.endpoint.is_kernel_task())
            .count();
        let priv_srvs = self
            .priv_table
            .iter()
            .filter(|e| !e.endpoint.is_kernel_task())
            .count();
        if image_srvs != priv_srvs {
            return Err(ENOSYS); // boot protocol violation; C panics (main.c:226)
        }
        Ok(())
    }
}

/// Minimal valid boot image for `placeholder()`.
///
/// Contains the boot-order services (RS/VM/PM/SCHED/VFS/DS) plus INIT; the
/// exact `start_addr`/`len` are placeholders (kernel ELF blobs are reported by
/// `sys_getimage` at runtime).
/// Builds a placeholder `BootImage` entry with a padded name.
const fn boot_img(proc_nr: i32, endpoint: Endpoint, name: &str) -> BootImage {
    let mut proc_name = [0u8; 16];
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        proc_name[i] = bytes[i];
        i += 1;
    }
    BootImage {
        proc_nr,
        proc_name,
        endpoint,
        start_addr: 0,
        len: 0,
    }
}

/// Minimal valid boot image for `placeholder()`.
///
/// Contains the full boot-order service set (RS → ... → INIT) so the image and
/// priv tables describe the same 12 services (main.c:225-227 count check).
/// The exact `start_addr`/`len` are placeholders (kernel ELF blobs are
/// reported by `sys_getimage` at runtime).
static BOOT_IMAGE_PLACEHOLDER: &[BootImage] = &[
    boot_img(2, Endpoint::RS, "rs"),
    boot_img(8, Endpoint::VM, "vm"),
    boot_img(0, Endpoint::PM, "pm"),
    boot_img(4, Endpoint::SCHED, "sched"),
    boot_img(1, Endpoint::VFS, "vfs"),
    boot_img(6, Endpoint::DS, "ds"),
    boot_img(5, Endpoint::TTY, "tty"),
    boot_img(3, Endpoint::MEM, "memory"),
    boot_img(7, Endpoint::MIB, "mib"),
    boot_img(9, Endpoint::PFS, "pfs"),
    boot_img(10, Endpoint::MFS, "mfs"),
    boot_img(11, Endpoint::INIT, "init"),
];

/// Failure modes of the boot table lookups.
///
/// C: `panic("boot image table lookup failed")` / `panic("boot image priv table
/// lookup failed")` — main.c:731, 746. Sys/dev lookups never fail: they fall
/// back to the default entry (main.c:753-777).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupError {
    /// Image table lookup failed (C panic, main.c:731).
    ImageTable,
    /// Priv table lookup failed (C panic, main.c:746).
    PrivTable,
}

/// Looks up an entry in the boot image table.
///
/// C: `boot_image_info_lookup(..., ip, ...)` image branch — main.c:723-733.
pub fn lookup_image(image: &[BootImage], endpoint: Endpoint) -> Result<&BootImage, LookupError> {
    image
        .iter()
        .find(|ip| ip.endpoint == endpoint)
        .ok_or(LookupError::ImageTable)
}

/// Looks up an entry in the boot image priv table.
///
/// C: `boot_image_info_lookup(..., pp, ...)` priv branch — main.c:735-748.
pub fn lookup_priv(
    table: &[BootImagePriv],
    endpoint: Endpoint,
) -> Result<&BootImagePriv, LookupError> {
    table
        .iter()
        .find(|e| e.endpoint == endpoint)
        .ok_or(LookupError::PrivTable)
}

/// Looks up an entry in the boot image sys table, falling back to the default.
///
/// C: `boot_image_info_lookup(..., sp, ...)` sys branch — main.c:753-762
/// ("accept the default entry").
pub fn lookup_sys(table: &[BootImageSys], endpoint: Endpoint) -> &BootImageSys {
    table
        .iter()
        .find(|e| e.endpoint == endpoint)
        .unwrap_or(&DEFAULT_SYS)
}

/// Looks up an entry in the boot image dev table, falling back to the default.
///
/// C: `boot_image_info_lookup(..., dp, ...)` dev branch — main.c:768-777.
pub fn lookup_dev(table: &[BootImageDev], endpoint: Endpoint) -> &BootImageDev {
    table
        .iter()
        .find(|e| e.endpoint == endpoint)
        .unwrap_or(&DEFAULT_DEV)
}

/// Global init descriptor carried from boot to the ready protocol.
///
/// C: `rinit` (`servers/rs/glo.h`) — full semantics in 12-rs-init-run.md.
/// This module only carries the grant creation point (main.c:185);
/// `init_service` (12) consumes `rproctab_gid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RinitState {
    /// C: `rinit.rproctab_gid = cpf_grant_direct(ANY, rprocpub, ...)` — main.c:185.
    ///
    /// `None` = grant not yet created (DEFERRED: `cpf_grant_direct` wiring is
    /// part of the syscall surface, 19). Consumption: 12-rs-init-run.md.
    pub rproctab_gid: Option<u32>,
}

/// The 4-step boot state machine.
///
/// C: `sef_cb_init_fresh` — main.c:158-494. [`BootInit::init_fresh`] is the
/// single entry; the four steps are private and strictly ordered.
pub struct BootInit<'a> {
    tables: BootTables<'a>,
    /// C: `rinit` — main.c:185 (grant creation point).
    rinit: RinitState,
    /// C: `rproc[]`/`rprocpub[]` + `rproc_ptr[]` — the service table with
    /// endpoint→slot index (02-rs-process-table.md §4.2, ARCH A-4).
    table: RProcTable,
    /// C: `shutting_down` — main.c:193.
    shutting_down: bool,
    /// C: `system_hz` — main.c:181.
    system_hz: u32,
    /// Number of init-ready messages still expected (Step 2 → Step 3).
    /// C: `nr_uncaught_init_srvs` — main.c:349-406.
    nr_uncaught_init_srvs: usize,
}

impl<'a> BootInit<'a> {
    /// Creates a fresh boot machine with the given boot tables.
    pub fn new(tables: BootTables<'a>) -> Self {
        Self {
            tables,
            rinit: RinitState::default(),
            table: RProcTable::new(),
            shutting_down: false,
            system_hz: 0,
            nr_uncaught_init_srvs: 0,
        }
    }

    /// Runs the 4-step boot. C: `sef_cb_init_fresh` — main.c:158-494.
    ///
    /// Step order is fixed and cannot be reordered from outside:
    /// `step1_set_attrs` → `step2_allow_run` → `step3_catch_init_ready` →
    /// `step4_finish`.
    pub fn init_fresh(&mut self, sys: &mut dyn KernelApi) -> Result<(), i32> {
        self.step0_prepare(sys)?;
        self.step1_set_attrs(sys)?;
        self.step2_allow_run(sys)?;
        self.step3_catch_init_ready(sys)?;
        self.step4_finish(sys)?;
        Ok(())
    }

    /// Step 0 — preparation: config, frequency, grant, resets, image copy.
    ///
    /// C: main.c:178-237.
    fn step0_prepare(&mut self, sys: &mut dyn KernelApi) -> Result<(), i32> {
        // C: env_parse("rs_verbose", ...) — main.c:179 (config injection; A-11).
        // C: sys_getinfo(GET_HZ, &system_hz, ...) — main.c:181-183.
        self.system_hz = sys.get_hz()?;
        // C: rinit.rproctab_gid = cpf_grant_direct(...) — main.c:185.
        //   Creation point. DEFERRED: grant syscall wiring is 19's scope; the
        //   field stays None until then (consumed by init_service, 12).
        self.rinit = RinitState::default();
        // C: RUPDATE_INIT() + shutting_down = FALSE — main.c:192-193.
        //   The update descriptor (16) is reset at construction
        //   (`RupdateState::default()` in 16's module); we reset the flag here.
        self.shutting_down = false;
        // C: sys_getimage(image) — main.c:196: the image is injected as
        //   `self.tables.image` (ARCH A-13).
        // C: count comparison — main.c:225-227.
        self.tables.validate_tables()?;
        // C: process table reset — main.c:230-237. Rebuilt from scratch
        //   (full field semantics: 02-rs-process-table.md §2.12).
        self.table = RProcTable::new();
        Ok(())
    }

    /// The global init descriptor prepared at boot.
    ///
    /// C: `rinit` — main.c:185. Consumed by the ready protocol
    /// (`init_service`, 12-rs-init-run.md).
    pub fn rinit(&self) -> &RinitState {
        &self.rinit
    }

    /// Step 1 — establish priv/sys/dev attributes for every boot service.
    ///
    /// C: main.c:244-346. RS/VM skip `SYS_PRIV_SET_SYS` (main.c:285-291) —
    /// they are already running. The priv-structure construction itself
    /// (send mask, call masks, sig mgr) belongs to 03/05.
    fn step1_set_attrs(&mut self, sys: &mut dyn KernelApi) -> Result<(), i32> {
        let tables = self.tables;
        for (slot_nr, priv_) in tables.priv_table.iter().enumerate() {
            if priv_.endpoint.is_kernel_task() {
                continue; // C: iskerneln skip — main.c:248-250
            }
            // C: boot_image_info_lookup(ep, image, &ip, NULL, &sys, &dev) — main.c:253-254.
            let ip = lookup_image(tables.image, priv_.endpoint).map_err(|_| ENOSYS)?;
            let sys_ = lookup_sys(tables.sys_table, priv_.endpoint);
            let dev = lookup_dev(tables.dev_table, priv_.endpoint);

            // C: boot Step 1 priv assembly — main.c:264-296 (03-rs-privilege.md §4.2).
            let mut privilege = Privilege::boot_priv(
                PrivFlags::from_bits_truncate(priv_.flags as u16),
                priv_.endpoint.slot(),
            );
            // C: sys_privctl(SYS_PRIV_SET_SYS) — RS/VM exception — main.c:285-291.
            if priv_.endpoint != Endpoint::RS && priv_.endpoint != Endpoint::VM {
                sys.privctl(priv_.endpoint, PrivCtlOp::SetSys, Some(&privilege))?;
            }
            // C: sys_getpriv — main.c:293-296. The kernel may have rewritten
            //   the structure (id/proc_nr/pending); take the synced version.
            privilege = sys.getpriv(priv_.endpoint)?;

            // C: slot population + activation — main.c:258-345. The boot slot
            //   index equals the priv-table index (main.c:255); mechanism
            //   ownership: 02-rs-process-table.md §4.2 `activate_boot_slot`.
            // C: strlcpy(rpub->proc_name, ip->proc_name) — main.c:313.
            let proc_name = crate::service_slot::Label::from_bytes(&ip.proc_name);
            self.table.activate_boot_slot(
                SlotId::new(slot_nr),
                priv_.endpoint,
                proc_name,
                priv_,
                sys_,
                dev,
                privilege,
            )?;
        }
        Ok(())
    }

    /// Step 2 — allow every boot service to run.
    ///
    /// C: main.c:348-399. RS/VM go through `init_service` (12) directly;
    /// other services get `sched_init_proc` + `SYS_PRIV_ALLOW` first.
    fn step2_allow_run(&mut self, sys: &mut dyn KernelApi) -> Result<(), i32> {
        let tables = self.tables;
        let mut nr_uncaught_init_srvs = 0usize;

        for priv_ in tables.priv_table {
            if priv_.endpoint.is_kernel_task() {
                continue; // C: iskerneln skip — main.c:354-356
            }
            // RS/VM are already running — C: main.c:362-373.
            if priv_.endpoint == Endpoint::RS || priv_.endpoint == Endpoint::VM {
                // C: init_service(rp, SEF_INIT_FRESH, ...) — main.c:365-367.
                //   Mechanism: 12-rs-init-run.md (self-simulated ready).
                //   DEFERRED: init_service wiring lands with 12.
                if priv_.endpoint != Endpoint::RS {
                    // VM still sends an RS_INIT message — main.c:369-370.
                    nr_uncaught_init_srvs += 1;
                }
                continue;
            }
            // C: sched_init_proc — main.c:376; sys_privctl(SYS_PRIV_ALLOW) — main.c:379.
            sys.sched_init_proc(priv_.endpoint)?;
            sys.privctl(priv_.endpoint, PrivCtlOp::Allow, None)?;

            if priv_.flags & crate::table::SYS_PROC != 0 {
                // C: init_service — main.c:387. Mechanism: 12.
                // C: SF_SYNCH_BOOT → catch_boot_init_ready — main.c:390-392.
                //   Mechanism: 12. Here we only count (Step 3 catches the rest).
                nr_uncaught_init_srvs += 1;
            }
        }
        self.nr_uncaught_init_srvs = nr_uncaught_init_srvs;
        Ok(())
    }

    /// Step 3 — catch all remaining init-ready messages.
    ///
    /// C: `while(nr_uncaught_init_srvs) { catch_boot_init_ready(ANY); ... }` —
    /// main.c:401-407. The receive mechanism is 12-rs-init-run.md; this module
    /// only tracks the counter (DEFERRED until 12 lands).
    fn step3_catch_init_ready(&mut self, _sys: &mut dyn KernelApi) -> Result<(), i32> {
        while self.nr_uncaught_init_srvs > 0 {
            self.nr_uncaught_init_srvs -= 1;
        }
        Ok(())
    }

    /// Step 4 — pid lookup + periodic alarm.
    ///
    /// C: main.c:409-433. `getnpid` signature: 19; alarm semantics: 07.
    fn step4_finish(&mut self, sys: &mut dyn KernelApi) -> Result<(), i32> {
        let tables = self.tables;
        for priv_ in tables.priv_table {
            if priv_.endpoint.is_kernel_task() {
                continue; // C: iskerneln skip — main.c:416-418
            }
            // C: rp = &rproc[boot_image_priv - boot_image_priv_table] — main.c:422;
            //   slot located through the A-4 endpoint index (02 §4.2).
            let id = self
                .table
                .endpoint_slot(priv_.endpoint)
                .expect("Step 1 activated every boot service");
            // C: rp->r_pid = getnpid(rpub->endpoint) — main.c:426.
            let pid = sys.getnpid(priv_.endpoint)?;
            if pid < 0 {
                // C: panic("unable to get pid") — main.c:427-429.
                return Err(ENOSYS);
            }
            self.table.get_mut(id).pid = Some(pid);
        }
        // C: sys_setalarm(RS_DELTA_T, 0) — main.c:433. RS_DELTA_T = system_hz
        // (const.h:49); period/heartbeat semantics: 07-rs-period-heartbeat.md.
        sys.setalarm(self.system_hz)?;
        Ok(())
    }

    /// RS self-upgrade after boot (USE_LIVEUPDATE).
    ///
    /// C: main.c:436-491. Gated by cargo feature `live-update` (ARCH A-11).
    /// The full mechanism belongs to 18-rs-self-lifecycle.md; this method
    /// only pins the call chain and its doc ownership.
    #[cfg(feature = "live-update")]
    pub fn self_update(&mut self, sys: &mut dyn KernelApi) -> Result<(), i32> {
        // C: clone_slot(rp, &replica_rp) — main.c:441 (10/18).
        // C: srv_fork(0, 0) — main.c:446 (10, ARCH A-1).
        // C: update_service(&rp, &replica_rp, RS_SWAP, 0) — main.c:460 (16).
        // C: cpf_reload() — main.c:464 (17).
        // C: cleanup_service(rp) — main.c:467 (15).
        // C: vm_memctl(VM_RS_MEM_PIN) — main.c:470-472 (10/19).
        // C: sys_privctl(SYS_PRIV_SET_SYS) + sched_init_proc + SYS_PRIV_YIELD — main.c:478-489 (03).
        let _ = sys.getnpid(Endpoint::RS)?; // placeholder: force the API boundary
        unimplemented!("RS self-upgrade lands with 18-rs-self-lifecycle.md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privilege::{CallMask, PrivFlags};
    use crate::table::*;
    use alloc::vec::Vec;
    use minix_types::BootImage;

    /// Records the kernel calls made during boot, in order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        GetMachine,
        GetHz,
        PrivCtl(Endpoint, PrivCtlOp),
        GetPriv(Endpoint),
        SchedInitProc(Endpoint),
        GetNpid(Endpoint),
        SetAlarm(u32),
    }

    #[derive(Default)]
    struct MockKernelApi {
        calls: Vec<Call>,
        hz: u32,
        pids: Vec<i32>,
    }

    impl MockKernelApi {
        fn new(hz: u32) -> Self {
            Self {
                calls: Vec::new(),
                hz,
                pids: Vec::new(),
            }
        }
    }

    impl KernelApi for MockKernelApi {
        fn get_machine(&mut self) -> Result<Machine, i32> {
            self.calls.push(Call::GetMachine);
            Ok(Machine::default())
        }
        fn get_hz(&mut self) -> Result<u32, i32> {
            self.calls.push(Call::GetHz);
            Ok(self.hz)
        }
        fn privctl(
            &mut self,
            proc: Endpoint,
            op: PrivCtlOp,
            _priv_: Option<&Privilege>,
        ) -> Result<(), i32> {
            self.calls.push(Call::PrivCtl(proc, op));
            Ok(())
        }
        fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, i32> {
            self.calls.push(Call::GetPriv(proc));
            Ok(Privilege::vacant())
        }
        fn sched_init_proc(&mut self, proc: Endpoint) -> Result<(), i32> {
            self.calls.push(Call::SchedInitProc(proc));
            Ok(())
        }
        fn getnuid(&mut self, _proc: Endpoint) -> Result<u32, i32> {
            Ok(0) // root for boot tests
        }
        fn getnpid(&mut self, proc: Endpoint) -> Result<i32, i32> {
            self.calls.push(Call::GetNpid(proc));
            Ok(self.pids.pop().unwrap_or(100))
        }
        fn setalarm(&mut self, delay_ticks: u32) -> Result<(), i32> {
            self.calls.push(Call::SetAlarm(delay_ticks));
            Ok(())
        }
    }

    fn boot_image(proc_nr: i32, endpoint: Endpoint) -> BootImage {
        BootImage {
            proc_nr,
            proc_name: *b"x\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            endpoint,
            start_addr: 0,
            len: 0,
        }
    }

    #[test]
    fn test_init_fresh_step_order() {
        // Boot tables must describe the same service set (image vs priv).
        let image: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
            boot_image(1, Endpoint::VFS),
            boot_image(6, Endpoint::DS),
            boot_image(5, Endpoint::TTY),
            boot_image(3, Endpoint::MEM),
            boot_image(7, Endpoint::MIB),
            boot_image(9, Endpoint::PFS),
            boot_image(10, Endpoint::MFS),
            boot_image(11, Endpoint::INIT),
        ];
        let tables = BootTables::new(image);
        tables
            .validate_tables()
            .expect("placeholder tables consistent");

        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.init_fresh(&mut sys).expect("boot must succeed");

        // Step 1: SYS_PRIV_SET_SYS for every service except RS/VM (10 services).
        let set_sys = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::PrivCtl(_, PrivCtlOp::SetSys)))
            .count();
        assert_eq!(set_sys, 10, "RS/VM skip SYS_PRIV_SET_SYS (main.c:285-291)");

        // Step 2: sched_init_proc + SYS_PRIV_ALLOW for every non-RS/VM SYS_PROC.
        let sched = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::SchedInitProc(_)))
            .count();
        let allow = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::PrivCtl(_, PrivCtlOp::Allow)))
            .count();
        assert_eq!(sched, 10);
        assert_eq!(allow, 10);

        // Step 1: getpriv for all 12 services (RS/VM included) — main.c:293-296.
        let getpriv = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::GetPriv(_)))
            .count();
        assert_eq!(
            getpriv, 12,
            "getpriv syncs all boot services (main.c:293-296)"
        );

        // Step 1: every slot's priv_ is the kernel-synced version returned by
        // sys_getpriv — 03-rs-privilege.md §1.3 (mock returns vacant).
        for ep in image.iter().map(|ip| ip.endpoint) {
            let id = boot.table.endpoint_slot(ep).expect("boot slot indexed");
            assert_eq!(boot.table.get(id).priv_, Privilege::vacant());
        }

        // Step 4: getnpid for all 12 services + setalarm(system_hz).
        let npid = sys
            .calls
            .iter()
            .filter(|c| matches!(c, Call::GetNpid(_)))
            .count();
        assert_eq!(npid, 12);
        assert!(sys.calls.iter().any(|c| matches!(c, Call::SetAlarm(100))));
    }

    #[test]
    fn test_init_fresh_populates_table() {
        let image: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
            boot_image(1, Endpoint::VFS),
            boot_image(6, Endpoint::DS),
            boot_image(5, Endpoint::TTY),
            boot_image(3, Endpoint::MEM),
            boot_image(7, Endpoint::MIB),
            boot_image(9, Endpoint::PFS),
            boot_image(10, Endpoint::MFS),
            boot_image(11, Endpoint::INIT),
        ];
        let tables = BootTables::new(image);
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.init_fresh(&mut sys).expect("boot must succeed");

        // Step 1: the 12 boot services occupy slots 0..11 (priv-table order,
        // main.c:255), marked IN_USE|ACTIVE, and the A-4 index hits.
        use crate::service_slot::RFlags;
        let boot_eps = [
            Endpoint::RS,
            Endpoint::VM,
            Endpoint::PM,
            Endpoint::SCHED,
            Endpoint::VFS,
            Endpoint::DS,
            Endpoint::TTY,
            Endpoint::MEM,
            Endpoint::MIB,
            Endpoint::PFS,
            Endpoint::MFS,
            Endpoint::INIT,
        ];
        for (i, ep) in boot_eps.iter().enumerate() {
            let id = boot.table.endpoint_slot(*ep).expect("boot slot indexed");
            assert_eq!(id, SlotId::new(i), "slot == priv-table index");
            let slot = boot.table.get(id);
            assert!(slot.flags.contains(RFlags::IN_USE | RFlags::ACTIVE));
            assert_eq!(slot.pub_.endpoint, *ep);
        }

        // Non-boot slots stay vacant.
        for i in 12..boot.table.len() {
            let slot = boot.table.get(SlotId::new(i));
            assert!(slot.flags.is_empty());
            assert!(!slot.pub_.in_use);
        }
    }

    #[test]
    fn test_step4_sets_pid() {
        let image: &[BootImage] = &[
            boot_image(2, Endpoint::RS),
            boot_image(8, Endpoint::VM),
            boot_image(0, Endpoint::PM),
            boot_image(4, Endpoint::SCHED),
            boot_image(1, Endpoint::VFS),
            boot_image(6, Endpoint::DS),
            boot_image(5, Endpoint::TTY),
            boot_image(3, Endpoint::MEM),
            boot_image(7, Endpoint::MIB),
            boot_image(9, Endpoint::PFS),
            boot_image(10, Endpoint::MFS),
            boot_image(11, Endpoint::INIT),
        ];
        let tables = BootTables::new(image);
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.init_fresh(&mut sys).expect("boot must succeed");

        // Step 4: every boot slot carries the pid returned by getnpid
        // (main.c:426; mock returns 100).
        let boot_eps = [
            Endpoint::RS,
            Endpoint::VM,
            Endpoint::PM,
            Endpoint::SCHED,
            Endpoint::VFS,
            Endpoint::DS,
            Endpoint::TTY,
            Endpoint::MEM,
            Endpoint::MIB,
            Endpoint::PFS,
            Endpoint::MFS,
            Endpoint::INIT,
        ];
        for ep in boot_eps {
            let id = boot.table.endpoint_slot(ep).expect("boot slot indexed");
            assert_eq!(boot.table.get(id).pid, Some(100));
        }
    }

    #[test]
    fn test_step1_skips_privctl_for_rs_vm() {
        // Custom tables: only RS + VM are boot services.
        let image: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(8, Endpoint::VM)];
        let priv_table: &[BootImagePriv] = &[
            BootImagePriv {
                endpoint: Endpoint::RS,
                label: "rs",
                flags: RSYS_F,
            },
            BootImagePriv {
                endpoint: Endpoint::VM,
                label: "vm",
                flags: VM_F,
            },
        ];
        let sys_table: &[BootImageSys] = &[
            BootImageSys {
                endpoint: Endpoint::RS,
                flags: SRVR_SF,
            },
            BootImageSys {
                endpoint: Endpoint::VM,
                flags: VM_SF,
            },
        ];
        let dev_table: &[BootImageDev] = &[];
        let tables = BootTables {
            image,
            priv_table,
            sys_table,
            dev_table,
        };
        let mut sys = MockKernelApi::new(100);
        let mut boot = BootInit::new(tables);
        boot.init_fresh(&mut sys).expect("boot must succeed");

        let set_sys: Vec<Endpoint> = sys
            .calls
            .iter()
            .filter_map(|c| match c {
                Call::PrivCtl(ep, PrivCtlOp::SetSys) => Some(*ep),
                _ => None,
            })
            .collect();
        assert!(set_sys.is_empty(), "RS and VM must skip SYS_PRIV_SET_SYS");
    }

    #[test]
    fn test_validate_tables_mismatch() {
        // Image describes 2 services; priv table describes 12 → mismatch.
        let image: &[BootImage] = &[boot_image(2, Endpoint::RS), boot_image(8, Endpoint::VM)];
        let tables = BootTables::new(image);
        assert!(
            tables.validate_tables().is_err(),
            "main.c:225-227 mismatch check"
        );
    }

    #[test]
    fn test_lookup_image_not_found() {
        let image: &[BootImage] = &[];
        assert!(matches!(
            lookup_image(image, Endpoint::RS),
            Err(LookupError::ImageTable)
        ));
    }

    #[test]
    fn test_lookup_priv_found() {
        let found = lookup_priv(BOOT_IMAGE_PRIV_TABLE, Endpoint::RS).expect("RS in priv table");
        assert_eq!(found.label, "rs");
    }

    #[test]
    fn test_lookup_priv_not_found() {
        assert_eq!(
            lookup_priv(BOOT_IMAGE_PRIV_TABLE, Endpoint::NONE),
            Err(LookupError::PrivTable)
        );
    }

    #[test]
    fn test_lookup_sys_default_fallback() {
        let sys = lookup_sys(BOOT_IMAGE_SYS_TABLE, Endpoint::INIT);
        assert_eq!(
            sys.flags, SRV_SF,
            "INIT is not in sys table → default (main.c:753-762)"
        );
    }

    #[test]
    fn test_lookup_dev_default_fallback() {
        let dev = lookup_dev(BOOT_IMAGE_DEV_TABLE, Endpoint::RS);
        assert_eq!(
            dev.dev_nr, 0,
            "RS is not in dev table → default (main.c:768-777)"
        );
    }

    #[test]
    fn test_placeholder_tables_valid() {
        let tables = BootTables::placeholder();
        assert!(
            tables.validate_tables().is_ok(),
            "placeholder must pass count check"
        );
    }
}

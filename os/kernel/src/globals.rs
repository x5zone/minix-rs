//! Global kernel statics — the crate's single audit point for
//! `static` state (A2, todo §1).
//!
//! Every process-global lives here: the `SyncUnsafeCell` wrapper, the
//! sealed `BklProtected` trait with its approved-type list (the friction
//! point for wrapping a new type), and the 24 statics themselves
//! (13 as of A2 + 11 collected in by D-62, 2026-09-09).
//! Accessor families (`*_with` / `*_boot_unchecked` / raw) stay in
//! `lib.rs` — they are the crate's API surface and consume these via
//! re-export, so the audit point for "what globals exist" and the
//! implementation of "how they may be touched" are one file apart by
//! design, not interleaved with the boot sequence.

use minix_boot::KernelInfo;
use bkl_protected::BklProtected;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64};

/// `UnsafeCell` with `Sync` gated on the [`BklProtected`] marker trait.
///
/// Mirrors the unstable stdlib `SyncUnsafeCell` (rust-lang issue #95439),
/// but with a **compile-time guard** against accidental misuse: only types
/// that explicitly opt into `BklProtected` can be wrapped. This prevents
/// soundness bugs where a `!Sync` type (e.g. `RefCell<T>`, `Rc<T>`,
/// `Cell<T>`) is silently promoted to `Sync` by being placed in a
/// `static SyncUnsafeCell<...>`.
///
/// All access requires external synchronization (BKL or single-threaded boot).
/// This type makes the `Sync` requirement explicit, replacing `static mut` +
/// `addr_of_mut!` for Rust 2024 compliance — no `static_mut_refs` involved.
///
/// # Soundness contract (FIX-07: R-01)
///
/// The `unsafe impl<T: BklProtected + ?Sized> Sync` below is sound because:
/// 1. `BklProtected` is a sealed trait — only types in this file's
///    `bkl_protected_impls!` macro call can implement it.
/// 2. Every approved type is either `Send + Sync` by itself (so wrapping it
///    in `SyncUnsafeCell` adds no new cross-thread access capability beyond
///    what `static` already provides) or is an internal kernel struct whose
///    mutation is serialized by the BKL at every callsite.
/// 3. The BKL provides mutual exclusion at runtime; `SyncUnsafeCell` only
///    silences the `!Sync`-ness of `UnsafeCell` so the wrapped type can be
///    placed in a `static`. Interior mutability through `get()` still
///    requires the caller to uphold the safety contract.
///
/// See `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/06-proc-init-boot-proc.md`
/// §4.1 (storage model) for the design rationale.
#[repr(transparent)]
pub(crate) struct SyncUnsafeCell<T: ?Sized> {
    value: core::cell::UnsafeCell<T>,
}

// SAFETY: See "Soundness contract" above. `T: BklProtected` restricts the
// impl to types whose mutation is serialized by the BKL (or which are
// write-once-read-only after boot). Without this bound, any `!Sync` type
// could be wrapped — that was the original soundness hole (R-01).
unsafe impl<T: BklProtected + ?Sized> Sync for SyncUnsafeCell<T> {}

impl<T> SyncUnsafeCell<T> {
    /// Creates a new `SyncUnsafeCell` wrapping the given value.
    ///
    /// `T` must implement [`BklProtected`]. This is enforced at construction
    /// time so that the `Sync` impl applies.
    pub(crate) const fn new(value: T) -> Self
    where
        T: BklProtected,
    {
        SyncUnsafeCell {
            value: core::cell::UnsafeCell::new(value),
        }
    }

    /// Gets a mutable pointer to the wrapped value.
    ///
    /// The caller must ensure that no concurrent access occurs (BKL or
    /// single-threaded context). Dereferencing the returned pointer is `unsafe`.
    pub(crate) fn get(&self) -> *mut T
    where
        T: BklProtected,
    {
        self.value.get()
    }
}

// ── BklProtected: sealed marker trait gating `SyncUnsafeCell` (FIX-07: R-01)
//
// Without this trait, the old blanket `unsafe impl<T: ?Sized> Sync` let any
// type (including `RefCell<T>` / `Rc<T>` / `Cell<T>`) be silently promoted
// to `Sync` by wrapping it in `SyncUnsafeCell`. The marker trait is sealed
// so external crates (and other modules in this crate) cannot add new impls
// without going through the audit process documented above.
//
// The trait is implemented only for the 9 types currently stored in
// `SyncUnsafeCell` statics (see `bkl_protected_impls!` below). Adding a new
// `SyncUnsafeCell<NewType>` static requires extending the macro call —
// this is intentional friction.
mod bkl_protected {
    /// Sealed marker trait — see module docs.
    ///
    /// # Safety
    ///
    /// Implementors must guarantee that all mutation of `Self` is serialized
    /// by the Big Kernel Lock (BKL) at every callsite, OR that `Self` is
    /// write-once-read-only after boot (e.g. `KernelInfo`). The BKL provides
    /// runtime mutual exclusion; this trait only silences the `!Sync`-ness
    /// of `UnsafeCell` so the wrapped type can live in a `static`.
    pub(crate) unsafe trait BklProtected: Sealed {}

    /// Sealed trait — no external impls possible.
    pub(crate) trait Sealed {}

    // ── Macro to reduce boilerplate for approved types ──
    //
    // Each invocation expands to `impl Sealed for T {}` + `unsafe impl
    // BklProtected for T {}`. The `unsafe` is on the trait, so each
    // macro call is a single auditable unit.
    macro_rules! bkl_protected_impls {
        ($($ty:ty),+ $(,)?) => {
            $(
                impl Sealed for $ty {}
                /// # Safety
                ///
                /// All mutation is serialized by the BKL (or write-once after
                /// boot). See `bkl_protected` module docs.
                unsafe impl BklProtected for $ty {}
            )+
        };
    }

    // ── Approved types ──
    //
    // Adding a new type here is the ONLY way to wrap it in `SyncUnsafeCell`.
    // Each addition must be audited for BKL-serialized mutation.
    bkl_protected_impls! {
        // Write-once-read-only after boot (no BKL needed post-init):
        minix_boot::KernelInfo,
        crate::memmap::MemMapEntry,

        // BKL-serialized mutation (kernel-internal types):
        crate::proc_table::ProcessTable,
        crate::kpriv::PrivTable,
        crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>,
        crate::smp::SmpState,
        crate::smp::CpuInfoTable,
        crate::ipc_filter::IpcFilterPool,
        crate::krandom::KRandomness,
        crate::proc::ProcNr,
        crate::clock::ClockState,
        crate::misc::SprofInfo,
        crate::kmess::KmessRing,

        // Primitive element type for buffer arrays (`[u8; N]` via the
        // composite impl below) — a plain byte has no mutation surface of
        // its own; buffer access is BKL-serialized by the owning static.
        u8,
    }

    // S-5 (2026-09-14): the live GDT/TSS and IDT images (protection.rs /
    // trap_entry.rs). Mutation surface: written once by init_protection
    // BEFORE the first lgdt/lidt; afterwards only set_kernel_stack
    // (sp0 + stamp), which runs under the BKL — same contract as the other
    // boot-phase tables. Keeping them in SyncUnsafeCell honors the arch
    // traits' load() lifetime contract (tables must outlive being loaded)
    // without leaking them into public API. x86-64 only: aarch64/riscv64
    // protection is register state with no table image.
    #[cfg(target_arch = "x86_64")]
    bkl_protected_impls! {
        minix_arch::x86_64::protection::X86_64Protection,
        minix_arch::x86_64::trap_entry::X86_64TrapEntry,
    }

    // Generic composite impls — derive BklProtected from the inner type.
    // These allow `SyncUnsafeCell<Option<T>>` and `SyncUnsafeCell<[T; N]>`
    // without listing every instantiation.
    impl<T: BklProtected> Sealed for Option<T> {}
    /// # Safety
    ///
    /// `Option<T>` is `BklProtected` iff `T` is. The `Option` layer adds no
    /// new mutation surface beyond what `T` already has.
    unsafe impl<T: BklProtected> BklProtected for Option<T> {}

    impl<T: BklProtected, const N: usize> Sealed for [T; N] {}
    /// # Safety
    ///
    /// `[T; N]` is `BklProtected` iff `T` is. Array indexing adds no new
    /// mutation surface beyond what `T` already has.
    unsafe impl<T: BklProtected, const N: usize> BklProtected for [T; N] {}
}



#[cfg(test)]
mod bkl_protected_tests {
    use super::*;
    use ::alloc::string::String;

    /// Verify all approved types implement `BklProtected`.
    ///
    /// If any of these fails to compile, a `SyncUnsafeCell<NewType>` static
    /// was added without extending the `bkl_protected_impls!` macro in
    /// `bkl_protected` module — that is the intended friction point (R-01).
    #[test]
    fn bkl_protected_approved_types_implement_trait() {
        fn assert_impl<T: BklProtected>() {}

        // Write-once-read-only after boot:
        assert_impl::<minix_boot::KernelInfo>();
        assert_impl::<crate::memmap::MemMapEntry>();

        // BKL-serialized mutation:
        assert_impl::<crate::proc_table::ProcessTable>();
        assert_impl::<crate::kpriv::PrivTable>();
        assert_impl::<crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>>();
        assert_impl::<crate::smp::SmpState>();
        assert_impl::<crate::smp::CpuInfoTable>();
        assert_impl::<crate::ipc_filter::IpcFilterPool>();
        assert_impl::<crate::krandom::KRandomness>();

        // Composite impls:
        assert_impl::<Option<minix_boot::KernelInfo>>();
        assert_impl::<[crate::memmap::MemMapEntry; 4]>();
    }

    /// Document the negative case: `RefCell<T>` is `!Sync` and must NOT
    /// implement `BklProtected`. If this test compiles, the soundness
    /// guard is working — `SyncUnsafeCell<RefCell<T>>` cannot be constructed
    /// because `RefCell<T>: BklProtected` does not hold.
    ///
    /// Note: This is a compile-pass test. To verify the negative case
    /// directly, attempt to uncomment the `SyncUnsafeCell::new(RefCell::new(0))`
    /// line — it will fail with "trait bound `RefCell<i32>: BklProtected`
    /// is not satisfied".
    #[test]
    fn bkl_protected_refcell_does_not_impl() {
        // Uncomment to verify the guard rejects `RefCell`:
        // let _ = SyncUnsafeCell::new(core::cell::RefCell::new(0i32));
        //                                                                        ^^^ expected error

        // The approved types still work:
        let _ = SyncUnsafeCell::new(crate::krandom::KRandomness::new());
        let _ = SyncUnsafeCell::new(Option::<KernelInfo>::None);

        // String is NOT in the approved list — would fail to compile:
        // let _ = SyncUnsafeCell::new(String::new());
        let _ = String::new(); // suppress unused import warning
    }
}


/// Global flag: kernel may allocate physical memory directly.
/// C: kernel_may_alloc in glo.h
/// Set to true at kmain start, cleared in bsp_finish_booting().
pub(crate) static KERNEL_MAY_ALLOC: AtomicBool = AtomicBool::new(false);

/// Free physical memory map — populated by add_memmap during boot.
/// C: kinfo.memmap[MAXMEMMAP] — param.h:18
///
/// SAFETY: Only written during boot (single-threaded, before BKL needed).
/// After boot, this is read-only. BKL protects any post-boot access.
#[allow(dead_code)] // SMP memmap infra (checklist D-17); not yet wired to all call sites
pub(crate) static FREE_MEMMAP: SyncUnsafeCell<[crate::memmap::MemMapEntry; crate::memmap::MAXMEMMAP]> =
    SyncUnsafeCell::new([crate::memmap::MEM_MAP_ENTRY_ZERO; crate::memmap::MAXMEMMAP]);

/// Global KernelInfo — stored once during kmain, read-only thereafter.
///
/// C: `kinfo` global in glo.h — populated by memcpy from boot params in main.c.
///
/// SAFETY: Only written once during boot (single-threaded, before BKL needed).
/// After boot, read-only under BKL protection.
pub(crate) static KERNEL_INFO: SyncUnsafeCell<Option<KernelInfo>> = SyncUnsafeCell::new(None);

/// Global process table — C's `EXTERN struct proc proc[NR_TASKS + NR_PROCS]`.
///
/// # Storage (06-proc-init-boot-proc.md §3.1)
///
/// `SyncUnsafeCell` is the Rust 2024 translation of C's BSS `EXTERN` array:
/// compile-time-fixed address, zero heap, zero runtime overhead, with explicit
/// `Sync` (BKL guards all access). The `#![no_std]` kernel has no allocator at
/// boot time, so `Box<[KProcess]>` is forbidden here.
///
/// # SAFETY
///
/// All access requires the Big Kernel Lock (BKL). The BKL serializes all
/// kernel code, so at most one CPU mutates `PROC_TABLE` at a time. Boot-time
/// init (single-threaded, before BKL exists) is also safe.
pub(crate) static PROC_TABLE: SyncUnsafeCell<crate::proc_table::ProcessTable> = SyncUnsafeCell::new(crate::proc_table::ProcessTable::new());

/// Global privilege table — C's `EXTERN struct priv priv[NR_SYS_PROCS]`.
///
/// Same storage / safety model as `PROC_TABLE`. See `06-proc-init-boot-proc.md` §3.1.
pub(crate) static PRIV_TABLE: SyncUnsafeCell<crate::kpriv::PrivTable> = SyncUnsafeCell::new(crate::kpriv::PrivTable::new());

/// Global IRQ manager — owns the architecture's interrupt controller and
/// the IRQ hook chain.
///
/// Initialized in `init_clock_and_interrupts` after the interrupt controller
/// is constructed from the platform descriptor. Stored as `Option` because
/// `InterruptController::new` is not `const fn` (it reads hardware base
/// addresses from a descriptor).
///
/// # Safety
///
/// All access requires the BKL (or single-threaded boot before BKL exists).
/// See `proc_table()` / `priv_table()` for the same pattern.
///
/// # Design (D10 / 14-exception-interrupt.md §4.4)
///
/// The IRQ manager is a global so that trap entry points (assembly stubs)
/// can reach it without holding a reference in a CPU-local. This mirrors
/// C's global `irq_hooks[]` + `irq_actids[]` + `intr_*` globals.
pub(crate) static IRQ_MANAGER: SyncUnsafeCell<Option<crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>>> = SyncUnsafeCell::new(None);

/// D-9 (C glo.h:44 + system.c:160) — the process whose kernel call is
/// currently in flight (`kbill_kcall`). Set unconditionally after
/// `kernel_call_dispatch_inner` returns (C sets it after dispatch, before
/// finish; a failed call's kernel work is still the caller's — and
/// `kernel_call_resume` does not re-set it). Consumed at the
/// context_stop equivalents (`finish_and_restore` step 2 / `idle` step 4):
/// the whole TSC delta since the last switch point is attributed to the
/// marker's process `p_cycles.kcall`, then the marker clears (C
/// arch_clock.c:279-281 — the whole-delta attribution is C's own coarse
/// estimate, faithfully kept). SMP: a single global mirrors

/// lands.
pub(crate) static KBILL_KCALL: SyncUnsafeCell<Option<crate::proc::ProcNr>> = SyncUnsafeCell::new(None);

/// D-46 (software half, 2026-09-06): the global software clock state —
/// uptime/realtime/alarm-timer ring live here. C: `kclockinfo` +
/// `clock_timers` (clock.c globals). Initialized in
/// `init_clock_and_interrupts` (Phase B); consumed by the timer IRQ
/// handler (`clock::clock_irq_handler`) at dispatch time.
pub(crate) static CLOCK_STATE: SyncUnsafeCell<Option<crate::clock::ClockState>> = SyncUnsafeCell::new(None);

/// Global SMP state — owns per-CPU `CpuLocal` (proc_ptr, bill_ptr,
/// cpu_last_tsc, cpu_last_idle, ...) and CPU readiness flags.
///
/// Initialized in `init_proc_and_boot` to a single-CPU (BSP-only)
/// configuration. SMP expansion (16-smp.md) will replace this with a real
/// SMP discovery that boots APs before `bsp_finish_booting`.
///
/// # Safety
///
/// All access requires the BKL (or single-threaded boot before BKL exists).
/// Same storage / safety model as `PROC_TABLE` / `PRIV_TABLE` / `IRQ_MANAGER`.
///
/// # Design (D-notify-scheduler / 11-scheduling-primitives.md §4.5)
///
/// Made global so that `cpu_load()` (clock.rs) and `notify_scheduler()`
/// (proc_table.rs) can reach per-CPU `cpu_last_tsc` / `cpu_last_idle`
/// without threading `&mut SmpState` through every call site. Mirrors C's
/// `get_cpu_var_ptr(cpu, ...)` access pattern.
pub(crate) static SMP_STATE: SyncUnsafeCell<Option<crate::smp::SmpState>> = SyncUnsafeCell::new(None);

/// IPC filter pool for per-process message filtering.
/// C: `ipc_filter_pool[IPCF_POOL_SIZE]` — ipc_filter.h:54
///
/// Populated by `kmain()` Phase C.5 (IPCF_POOL_INIT).
/// Used by `dispatch_statectl` AddIpcBlFilter/AddIpcWlFilter (implemented).
///
/// SAFETY: Only written once during boot (single-threaded, before BKL needed).
/// After boot, accessed under BKL protection.
pub(crate) static IPC_FILTER_POOL: SyncUnsafeCell<crate::ipc_filter::IpcFilterPool> = SyncUnsafeCell::new(crate::ipc_filter::IpcFilterPool::new());

/// Global atomic mirror of C's `vm_running` flag.
///
/// In C, `vm_running` is a plain `int` in `glo.h:74` — a global, never
/// per-CPU. The Rust mirror is a global `AtomicBool` with identical
/// semantics; readers span multiple consumers (do_umap_remote, acpi,
/// oxpcie in C). A future per-CPU split would diverge from C and needs
/// an [ARCH] marker if ever proposed.
///
/// Writers: `bsp_finish_booting` (step 1) sets it false;
///          `dispatch_vmctl(VMCTL_SETADDRSPACE)` sets it true when target is VM.
/// Readers: `do_vmctl` sub-commands (Doc 23) check it before touching VM state.
///
/// # C bug note
///
/// Minix3 C source never sets `vm_running = 1` — only `main.c:47` sets it to 0.
/// This is a C omission (the flag is read in `do_umap_remote.c:106`,
/// `acpi.c:61,70`, `oxpcie.c:52,73` but never set true). Rust corrects this
/// by setting `vm_running = true` in `VMCTL_SETADDRSPACE` when the target is
/// `VM_PROC_NR`, matching the design intent documented in
/// `09-vm-boot-protocol.md §3 decision4`.
pub(crate) static VM_RUNNING: AtomicBool = AtomicBool::new(false);

/// Global tracking of the current "page table process" (ptproc).
///
/// In Minix3 C, `ptproc` is a per-CPU `struct proc *` variable
/// (`get_cpulocal_var(ptproc)`) that records which process currently owns
/// the active page table on this CPU. The `setcr3()` helper inside
/// `arch_do_vmctl()` checks `if (p == get_cpulocal_var(ptproc))` to decide
/// whether a CR3 update should also reload the hardware CR3 register.
/// C: protect.c:370 (arch_post_init sets ptproc = VM) — see doc 09 §2.2.
///
/// In the Rust port, `CpuLocal::ptproc` (Doc 16 §2.2) is the eventual home
/// for per-CPU ptproc tracking under SMP. Until SMP lands, we keep a single
/// global `AtomicI32` mirror that records the proc-nr of the current ptproc.
/// This is safe because:
///
/// 1. **BKL protection**: All writers (`init_post_and_memory`,
///    `dispatch_vmctl(SetAddrSpace)` when target is ptproc) hold the BKL.
///    The reader (`dispatch_vmctl(SetAddrSpace)` comparison) also holds the
///    BKL — syscalls always acquire BKL before reaching the dispatcher.
/// 2. **Single-writer principle**: `ptproc` is set only once during boot
///    (to `VM_PROC_NR` in `init_post_and_memory`) and is not subsequently
///    changed in normal operation (matching C behavior — see
///    `arch_post_init()` which is the only writer in C).
///
/// The value stored is a `ProcNr.0` (i32). `i32::MIN` (sentinel) means
/// "no ptproc set yet" — distinct from any valid proc-nr (which are
/// non-negative for user processes and small negative for kernel tasks).


/// Sentinel for "no root page table installed yet" (A2: moved with the
/// static it sentinels — every reader/writer of CURRENT_ROOT_PHYS pairs
/// against this value).
pub(crate) const ROOT_PHYS_UNSET: u64 = u64::MAX;

pub(crate) static CURRENT_ROOT_PHYS: AtomicU64 = AtomicU64::new(ROOT_PHYS_UNSET);

/// Sentinel for "kerninfo page not published to user processes yet".
///
/// C leaves `minix_kerninfo_user` at 0 until arch paging init maps the
/// kernel info page into every user address space (glo.h:33 declares it;
/// `arch/i386/memory.c:913` is the only writer). `do_ipc`'s
/// `MINIX_KERNINFO` arm treats 0 as EBADCALL (proc.c:687-689, "It might
/// not be initialized yet"), so the sentinel is 0 itself — same value,
/// same observable behavior.
pub(crate) const KERNINFO_USER_UNSET: u64 = 0;

/// User-mapped virtual address of the `minix_kerninfo` page (E-KERNINFO).
///
/// Written once by arch/boot init when the page gets its user mapping
/// (lands with the E1 user-mode trap bridge); read on every
/// `MINIX_KERNINFO` IPC call. `AtomicU64` for the same reason as
/// `CURRENT_ROOT_PHYS`: plain write-once-read-many `u64` needs no
/// `SyncUnsafeCell` wrapper.
pub(crate) static MINIX_KERNINFO_USER: AtomicU64 = AtomicU64::new(KERNINFO_USER_UNSET);


// ── V13 D-62（2026-09-09）收编的漏网静态 ──
// A2 的"单一审计点"此前被 4 组静态打破（本体散在 krandom/smp/clock/misc）。
// 本体全部迁入本文件；声明即登记，包装类型必须已在 `bkl_protected_impls!`
// 审批清单或泛型复合 impl 覆盖范围内。

/// Randomness state. C: `krandom` — random.h/krandom.c.
/// Access is BKL-protected: the IRQ path (single-writer via
/// `get_randomness`) and the syscall path (single-reader via
/// `dispatch_getinfo`) never run concurrently under BKL.
pub(crate) static KRANDOM: SyncUnsafeCell<crate::krandom::KRandomness> =
    SyncUnsafeCell::new(crate::krandom::KRandomness::new());

/// Track whether `KRANDOM` has been initialized (fields set to non-zero).
/// In Rust, `KRandomness::new()` is a const fn so it is initialized at link time.
pub(crate) static KRANDOM_INIT: AtomicBool = AtomicBool::new(false);

/// Per-CPU CPUID identity table (filled at `bsp_finish_booting` Step 0).
/// C: `cpu_info[CONFIG_MAX_CPUS]` — archtypes.h:39. Written during boot
/// (BSP; the C AP path records under boot_lock + BKL, arch_smp.c:227-232,
/// and will do the same when SMP bring-up lands); post-boot readers
/// (GET_CPUINFO) hold the BKL.
pub(crate) static CPU_INFO: SyncUnsafeCell<crate::smp::CpuInfoTable> =
    SyncUnsafeCell::new(crate::smp::CpuInfoTable::new());

// Clock mirrors (D-62③ 裁决：显式双源契约，见 clock.rs 模块 doc)。
// 写侧：仅 `ClockState::tick_with`/`set_boottime`/`set_realtime`（持 BKL，
// Release）；读侧：`clock::get_monotonic` 等无锁 Acquire。两源允许相差
// ≤1 tick（写点相邻且在 BKL 内）。C ground truth 是单源 `kclockinfo`
// （clock.c:189 直写），Rust 双源是无 `&ClockState` 读取的 ergonomics
// 产物；收敛需穿透 IpcEngine 签名（build_notify_message 等），登记为
// 独立重构路径，未在本批实施。

/// Monotonic uptime in ticks (mirror of `ClockState::uptime`).
/// C: `kclockinfo.uptime` — read by `get_monotonic()` (clock.c:203).
pub(crate) static CLOCK_UPTIME: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Wall-clock ticks since boot (mirror of `ClockState::realtime`).
/// C: `kclockinfo.realtime` — read by `get_realtime()` (clock.c:178).
pub(crate) static CLOCK_REALTIME: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Boot time in seconds since UNIX epoch, set by `SYS_STIME`
/// (mirror of `ClockState::boottime`).
/// C: `kclockinfo.boottime` — read by `get_boottime()` (clock.c:220).
pub(crate) static CLOCK_BOOTTIME: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// TSC cycles per millisecond, calibrated during boot (standalone
/// calibration constant — NOT a mirror; single source).
/// C: `tsc_per_ms[cpuid]` — kernel/proc.h. Per-CPU values arrive with SMP.
pub(crate) static TSC_PER_MS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

// Statistical-profiling state (C: sprof_ep / sprof_info_addr_vir /
// sprof_data_addr_vir / sprof_mem_size / sprof_info — profile.h:15-18,
// do_sprofile.c:23; buffer — profile.c:16). All access serialized by the
// BKL (PROF_START/STOP syscall paths, sample-save on the timer IRQ path).

/// Profiler control endpoint.
pub(crate) static SPROF_EP: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
/// User-space address of the profiler info record.
pub(crate) static SPROF_INFO_ADDR: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
/// User-space address of the sample buffer copy-out area.
pub(crate) static SPROF_DATA_ADDR: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
/// Size of the user-space copy-out area.
pub(crate) static SPROF_MEM_SIZE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Global profiling record (PROF_START resets, PROF_STOP reads).
/// Upgraded from `static mut` (D-62④): the `SyncUnsafeCell` wrapper keeps
/// the same raw-pointer access shape (`get()` replaces `addr_of_mut!`)
/// while putting the static under the BklProtected audit list.
pub(crate) static SPROF_INFO: SyncUnsafeCell<crate::misc::SprofInfo> =
    SyncUnsafeCell::new(crate::misc::SprofInfo {
        mem_used: 0,
        total_samples: 0,
        idle_samples: 0,
        system_samples: 0,
        user_samples: 0,
    });

/// Static sample buffer (BSS-allocated, zero-initialized).
/// C: `char sprof_sample_buffer[SAMPLE_BUFFER_SIZE]` — profile.c:16.
/// Upgraded from `static mut` (D-62④), same wrapper rationale as SPROF_INFO.
pub(crate) static SPROF_SAMPLE_BUFFER: SyncUnsafeCell<[u8; crate::misc::SAMPLE_BUFFER_SIZE]> =
    SyncUnsafeCell::new([0; crate::misc::SAMPLE_BUFFER_SIZE]);

//! Minix-RS Kernel
//!
//! Microkernel implementation, including:
//! - Boot sequence (arch_boot → kmain)
//! - Process management (proc)
//! - IPC mechanism (ipc)
//! - Scheduler (sched)
//! - Virtual memory — kernel part (vm)
//! - Hardware abstraction (hal, arch)

#![no_std]
#![cfg_attr(not(test), no_main)]
// Test helpers across syscall_*.rs build `Message` via
// `default() + assign m_type` then fill union fields under `unsafe`.
// Struct-literal form would still need `unsafe` for union writes — marginal
// gain, large churn across 70+ tests. Allowed per clippy::field_reassign_with_default.
#![cfg_attr(test, allow(clippy::field_reassign_with_default))]
// Mechanical gate for the silent-error-swallowing family (the fourth crate,
// same procedure as minix-sys/minix-rt/minix-fs, and the largest one): every
// `let _ =` in the kernel belongs to the maintain+comment disposition of the
// registry or to type witnesses/test setup — IPC store_result (C
// deliberately does not consume copy errors), identity-mapped regions with
// no backing, infallible write! with fixed format strings, the BklSection
// RAII witness, hook teardown/privilege-inheritance "no actionable failure"
// cases, and two compile-time negative examples/type assertions. Introducing
// a new unjustified `let _ =` is reported on the spot.
#![warn(clippy::let_underscore_must_use)]

// Zero-heap contract: the C kernel has no `malloc` (the minix3 kernel
// allocates nothing at runtime — all tables are static arrays), and this
// crate preserves that. `alloc` is linked only so `#[cfg(test)]` code can
// use `alloc::` collections; the production build registers NO
// `global_allocator`, so any allocation attempt on a non-test path fails
// at link time (undefined `__rust_alloc`). Runtime data structures
// therefore use fixed-size arrays, index-based intrusive lists, and
// `FmtBuf` stack formatting (see clock.rs, ipc.rs, page_fault.rs).
extern crate alloc;
#[cfg(test)]
extern crate std;

use crate::boot_policy::{KernelLayout, KernelMapPolicy, plan_boot_pages};
use minix_arch::CurrentBootArch;
use minix_arch::arch::boot_arch::BootArch;
use minix_arch::arch::boot_dm::plan_dm_policy;
use minix_arch::arch::direct_map::DirectMapLayout;
use minix_arch::paging_ext::{BootPageSupport, HugePages, PageSize, detect_boot_page_support};
use minix_types::{PhysBytes, PhysRange, VirBytes};
// Handoff types are exercised only by the non-mock VM-loading path
// (same reason as the `PhysAccess` import below).
#[cfg(all(
    not(feature = "mock"),
    any(
        target_arch = "aarch64",
        target_arch = "x86_64",
        target_arch = "riscv64"
    )
))]
use minix_boot::{
    ARENA_CAP, BootModule, RuntimeFields, WIRE_MODULE_CAP, WireBootModule, decode_envelope,
    decode_memmap, decode_modules, decode_param_buf, decode_platform_sources,
    decode_reserved_regions, decode_sources, patch_runtime_fields_in,
};
#[cfg(not(feature = "mock"))]
use minix_types::{VM_BOOT_HANDOFF_VA, VmBootHandoff};

#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
use minix_arch::arch::boot_arch::AArch64BootArch as BootArchImpl;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
use minix_arch::arch::boot_arch::X86_64BootArch as BootArchImpl;

use minix_arch::paging::{PageFlags, PageTableError, Paging};
use minix_boot::{KernelInfo, MemoryRegion};
// `PhysAccess` is exercised only by the non-mock VM-loading path
// (`access.frame_virt`, cfg'd out under `feature = "mock"`); a plain
// import would be an unused-import error in mock builds.
#[cfg(not(feature = "mock"))]
use minix_arch::arch::frame::PhysAccess;
use minix_arch::{
    BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END, DirectMapArch, ReturnSequence, TrapStyle, pt_alloc,
};
// QEMU test kernels build a user context directly and must
// record the full-context return style (`test-user-trap`).
pub use kerninfo::init_kerninfo;
pub use minix_arch::TrapStyle as PublicTrapStyle;

pub mod kpriv;
pub mod proc;
pub mod proc_table;
pub mod vm;

/// Compile-time form reconciliation tripwire (a tripwire, not a fix — the
/// fix is `default = []` in the three packages).
///
/// Assign `Option::<T>::None` to an `Option<expected concrete type>`: it only
/// compiles when the two types are identical, with no unsafe involved. The
/// kernel, through its own `mock` feature, requires the actual forms of
/// minix-arch and minix-plat to agree **simultaneously**; if a future
/// dependency path ever re-crosses the two packages' forms through cargo
/// feature unification, this tripwire reports "expected form vs actual form"
/// first, instead of leaving someone to guess the type error that would
/// otherwise scatter across the `Current*` alias consumption sites.
/// The tripwire is not an error filter: a bare `cargo test -p minix-kernel`
/// (dev-deps force mock while this package's `mock` is off) additionally
/// reports the same form mismatch from `store_protection`,
/// `store_trap_entry` and `CurrentDirectMap::default` — four sites pointing
/// at one cause. The right answer is to state the host-test form explicitly
/// — `cargo test -p minix-kernel --features mock` (the CI job
/// `kernel-tests.yml` runs exactly this as its `host-default-form` leg) —
/// not to relax the tripwire.
#[doc(hidden)]
pub mod form_contract {
    #[cfg(feature = "mock")]
    const _: Option<minix_arch::form::RuntimeWindowForm> = Option::<minix_arch::SelectedForm>::None;
    #[cfg(feature = "mock")]
    const _: Option<minix_plat::form::MockForm> = Option::<minix_plat::SelectedForm>::None;
    #[cfg(not(feature = "mock"))]
    const _: Option<minix_arch::form::ProductionForm> = Option::<minix_arch::SelectedForm>::None;
    #[cfg(not(feature = "mock"))]
    const _: Option<minix_plat::form::RealForm> = Option::<minix_plat::SelectedForm>::None;
}
pub mod boot;
pub mod boot_alloc;
pub mod boot_policy;
pub mod capability;
pub mod dm_coverage;
pub mod errno;
pub mod sched;
pub mod vm_handoff;

pub mod irq_manager;
#[cfg(target_arch = "riscv64")]
pub mod uart_irq;
// Kernel-side trap/syscall dispatch bodies — the policy half of every
// arch's production trap legs (x86_64: exceptions/IRQs + SYSCALL over the
// IDT frame; aarch64/riscv64: the E-3ARCHTRAP production legs over their
// own frame shapes, registered per arch by init_protection).
pub mod clock;
pub mod cross_space;
pub mod debug;
pub mod globals;
pub mod grant;
pub mod ipc;
pub mod ipc_filter;
pub mod kerninfo;
pub(crate) mod kmess;
pub mod krandom;
pub mod misc;
pub mod page_fault;
pub mod pte_walk;
pub mod smp;
pub mod stacktrace;
pub mod syscall;
pub mod syscall_clock;
pub mod syscall_copy;
pub mod syscall_device;
pub mod syscall_process;
pub mod syscall_signal;
pub mod trap_dispatch;

use globals::{
    CLOCK_STATE, CURRENT_ROOT_PHYS, IPC_FILTER_POOL, IRQ_MANAGER, KBILL_IPC, KBILL_KCALL,
    KERNEL_INFO, PRIV_TABLE, PROC_TABLE, ROOT_PHYS_UNSET, SMP_STATE, SyncUnsafeCell, VM_RUNNING,
};

/// Test-only serialization for tests that touch process-global boot state.
///
/// Unit tests of this crate run in one process: `BOOT_ALLOC` (boot bump
/// region), the `MockBootDmInstaller` leaf registry, and the `pt_alloc`
/// registration are all process-global. Tests that run the boot flow
/// (`arch_boot_impl` → Step 4 DM establishment) or mutate the bump region
/// must hold this lock; the DM leaf registry is cleared on acquisition so
/// repeated boot simulations never trip `AlreadyMapped` on overlapping
/// bootstrap candidates (the self-root candidate VA set is identical across
/// boot tests by construction).
#[cfg(all(test, feature = "mock"))]
pub(crate) mod test_sync {
    extern crate std;
    use std::sync::{Mutex, MutexGuard};

    static BOOT_GLOBALS_LOCK: Mutex<()> = Mutex::new(());

    /// Acquire the boot-globals lock and clear the MockBootDmInstaller registry.
    pub(crate) fn lock_boot_globals() -> MutexGuard<'static, ()> {
        let guard = BOOT_GLOBALS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        minix_arch::arch::boot_dm::mock::mock_dm_clear();
        guard
    }
}

#[cfg(test)]
mod test_helpers;

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
#[path = "arch/x86_64/mod.rs"]
pub mod x86_64;

#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
#[path = "arch/aarch64/mod.rs"]
pub mod aarch64;

#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
#[path = "arch/riscv64/mod.rs"]
pub mod riscv64;

pub use core::panic::PanicInfo;

// ── Boot entry points ──

/// Common tail of every direct `arch_boot` entry: land `KernelInfo` into the
/// kernel `.bss` copy, register the panic diagnostic, build/enable the
/// bootstrap root, then perform the higher-half jump. Keeping this in one
/// generic helper avoids three drifting copies of the same sequence.
#[cfg(not(feature = "mock"))]
fn arch_boot_finish<A, H>(kernel_info: &KernelInfo, root_page: PhysBytes) -> !
where
    A: BootArch,
    H: crate::boot::HigherHalf,
{
    // Fail-fast before any page table is built: a malformed handoff must not
    // reach the mapping machinery and only surface later at kmain. The
    // pre-table gate skips the boot_modules non-empty check because QEMU
    // bootstrap test kernels legally carry no init-server image.
    kernel_info.validate_before_tables();
    kernel_info.check_boot_postconditions();
    let kinfo = store_kernel_info(kernel_info);
    register_panic_diagnostic();
    arch_boot_impl::<A>(kinfo, root_page);
    // SAFETY: arch_boot_impl just enabled paging with both identity and
    // kernel high mappings. kinfo is the kernel-.bss copy — valid at the
    // high address. kern_stack_top() derives the image-tail stack top from
    // the two image-span fields.
    unsafe { H::jump_to_kmain(kinfo) }
}

/// Architecture-specific boot entry (called by boot-shim after ExitBootServices).
/// Selects the correct `Paging` implementation at compile time, then performs
/// the higher-half transition via the `HigherHalf` trait.
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use crate::x86_64::higher_half::X86_64HigherHalf;
    // Boot-visibility fix (root cause): KernelInfo is taken by value into the
    // kernel .bss global at fn entry, and everything afterwards (building page
    // tables, jumping to kmain) uses a reference to the global copy. The
    // original implementation handed the shim-stack `&KernelInfo` straight to
    // kmain through a `noreturn` asm `in("rdi")` — in x86-64 System V RDI is
    // caller-saved, and BOOTX64.EFI's codegen reused the entry RDI across the
    // intervening call, so after the callee overwrote the register kmain
    // received a .rdata string address (observed: kinfo=0x1ddda2ee, exactly
    // the "…jumping to kmain\n" string start plus its length); validate read
    // ASCII garbage and panicked. The global copy's address is a link-time
    // RIP-relative constant, handed over through no register at all.
    // Kernel-side first marker (EarlyConsole=COM1, the same port as
    // boot-shim's raw_serial; EBS has passed, no concurrent writers).
    {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        Console::write_str("kernel: arch_boot entered\n");
    }
    arch_boot_finish::<minix_arch::arch::boot_arch::X86_64BootArch, X86_64HigherHalf>(
        kernel_info,
        root_page,
    )
}

#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use crate::aarch64::higher_half::AArch64HigherHalf;
    // Same as x86_64: take KernelInfo by value into the kernel .bss global at
    // entry; jumps and page-table builds all use the global copy, eliminating
    // any reference handed across a `noreturn` asm call in a caller-saved
    // register (x0 on aarch64).
    arch_boot_finish::<minix_arch::arch::boot_arch::AArch64BootArch, AArch64HigherHalf>(
        kernel_info,
        root_page,
    )
}

/// Kernel-image-side high-half boot resume (all three cross-image
/// architectures; the aarch64 delegate shell carries the milestone prints).
///
/// The boot-shim already built the bootstrap page-table root and enabled
/// paging; that root is *currently loaded* in TTBR0/TTBR1 and maps *this* code.
/// This entry runs only the boot phases that are purely *additive* on the live
/// root — validate/register allocators, land the KernelInfo into this image's
/// `.bss`, establish the direct map, then enter `kmain`. It must NOT rebuild the
/// tables: calling [`build_bootstrap_root_and_enable`] here would
/// `new_from_page(root_page)` and zero-fill the *active* root (including the L0
/// slot holding this very function and the MMIO window), so the next
/// instruction fetch would fault. Because this image links in the TTBR1 high
/// half, `jump_to_kmain`'s `{kmain}` resolves to the real high virtual address
/// and execution proceeds immune to the first `switch_address_space` — the
/// original root cause, fixed at its source.
/// The additive-resume sequence shared by all cross-image entry points
/// (D01-R1 D2-07): validate → DM plan → record root → store → register →
/// establish DM → jump. Extracted for the same reason `arch_boot_finish`
/// exists for the inline face — one sequence, three callers, zero drift.
/// Architecture differences live only in the two type parameters; leg-specific
/// console milestones stay in the delegate shells.
#[cfg(not(feature = "mock"))]
fn resume_additive<A: BootArch, H: crate::boot::HigherHalf>(
    kernel_info: &KernelInfo,
    root_page: PhysBytes,
    stage_label: &str,
    done_label: &str,
) -> ! {
    // Validate KernelInfo + (idempotently) register the resumed bump allocator
    // and pt_alloc. This helper touches no page table — safe on a live root.
    let support = boot_validate_and_prepare::<A>(
        kernel_info.kern_virt_base,
        kernel_info.kern_phys_base,
        kernel_info.kern_size,
    );
    let layout = DirectMapLayout::<A>::from_arch();
    let dm = plan_dm_policy::<A>(&support, &layout)
        .unwrap_or_else(|e| panic!("resume: direct-map policy: {e}"));
    // Record the already-active bootstrap root in *this image's* global. The
    // inline boot records it inside `build_bootstrap_root_and_enable`, which we
    // deliberately skip; without this, downstream `init_kerninfo` (and every
    // other `current_root_phys()` consumer) sees `None` and panics, because the
    // boot-shim set the value only in its own (separate) `.bss` copy.
    set_current_root_phys(root_page);
    // Land the KernelInfo slices into this image's `.bss` before anything reads
    // them: the handoff copy's fat pointers still reference the boot-shim's
    // boot-services pool, which we re-home here so they survive past the PE.
    let kinfo = store_kernel_info(kernel_info);
    register_panic_diagnostic();
    // Step 4: add direct-map coverage to the *active* bootstrap root (additive;
    // `establish_boot_dm` wraps `root_page` and only writes new PTEs).
    // (`boot_stage!` is defined later in this file; use the same Console
    //   print path here. An empty label means the leg has no pre-establish
    //   landmark, so emit nothing.)
    if !stage_label.is_empty() {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        Console::write_str(stage_label);
        Console::write_str("\n");
    }
    // SAFETY: the boot-shim's bootstrap root is active, its identity window
    // covers the table pages, and this is the boot thread.
    unsafe { crate::dm_coverage::establish_boot_dm::<A>(kinfo, root_page, &dm) }
        .unwrap_or_else(|e| panic!("resume: DM coverage: {e}"));
    if !done_label.is_empty() {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        Console::write_str(done_label);
        Console::write_str("\n");
    }
    // SAFETY: paging is live via the shim's bootstrap root; kinfo is this
    // image's high-half-resident `.bss` copy; `kern_stack_top` is the high
    // virtual stack the inline boot used successfully.
    unsafe { H::jump_to_kmain(kinfo) }
}

#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub fn arch_boot_resume_high_half(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use crate::aarch64::higher_half::AArch64HigherHalf;
    use crate::boot::HigherHalf;
    // The aarch64 pre-establish landmark (grep'd by the U-Boot leg's gate
    // script) is preserved via stage_label. After "DM ok, jumping" was
    // merged into the common body, only the common print point remains.
    // The landmark sits after establish; to keep the gate script's anchor,
    // this shell keeps the first of the two lines, and the second is
    // carried by resume_additive's stage print (for the aarch64 leg the
    // label is the one before establish; removing the second line requires
    // updating the gate script — see this round's L4 check of
    // test-riscv64-uboot.sh's landmark list).
    resume_additive::<CurrentBootArch, AArch64HigherHalf>(
        kernel_info,
        root_page,
        "kernel-image: step4 DM coverage (additive on active root)",
        "kernel-image: DM ok, jumping to kmain (high half)",
    )
}

macro_rules! boot_stage {
    ($msg:expr) => {{
        #[cfg(not(feature = "mock"))]
        {
            use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
            Console::write_str($msg);
        }
    }};
}

/// The two values that cross the shim→image jump, each in its own coordinate
/// space.
///
/// The two values travel as one struct so a positional swap cannot compile.
/// Two adjacent bare `u64`s (`-> (u64, u64)` and
/// `jump_kernel_image_v1(entry: u64, arena_pa: u64)`) would allow the swap,
/// and that swap is close to the worst possible boot failure: the branch
/// target becomes the arena's *physical* address, so the first instruction
/// fetch after paging comes up faults, and `scause`/`CR2` report an address
/// unrelated to the real cause. Same pattern as `PlacementPool`
/// (`kernel-image/src/bootface_common.rs`) and `MapCounts`.
///
/// Visibility follows the single public entry: only `enter_kernel_image_v1`
/// is `pub`; `PreparedJump`, `prepare_kernel_image_v1` and
/// `jump_kernel_image_v1` stay private to this crate. This is not a wire type
/// — it never crosses an image boundary, it only carries the two locals
/// between two adjacent calls.
#[cfg(all(
    not(feature = "mock"),
    any(
        target_arch = "aarch64",
        target_arch = "x86_64",
        target_arch = "riscv64"
    )
))]
#[derive(Debug, Clone, Copy)]
struct PreparedJump {
    /// Virtual entry of the standalone image's `_start`.
    pub entry: VirBytes,
    /// Physical base of the handoff arena (the `x0`/`rbx`/`a0` handoff value).
    pub arena_pa: PhysBytes,
}

/// T1 2c/3b sender-side entry: build the bootstrap root, patch the runtime
/// fields into the already-serialized arena, then absolute-jump into the
/// standalone image.
///
/// The arena is an `ARENA_CAP` (eight-page) allocation below the identity
/// bound — `LOADER_DATA` on the two UEFI legs, bump-pool pages on the U-Boot
/// shim leg; the bootstrap root maps it through the identity window, so the in-place
/// patch is a plain memory write after paging is live. The architecture-specific
/// part is only the jump register convention: aarch64 passes the arena in
/// `x0`, x86-64 in `rbx` (the image trampoline moves it to `rdi` after the
/// `.bss` clear).

#[cfg(all(
    not(feature = "mock"),
    any(
        target_arch = "aarch64",
        target_arch = "x86_64",
        target_arch = "riscv64"
    )
))]

fn prepare_kernel_image_v1(
    arena_pa: PhysBytes,
    report: &minix_boot::BootReport<'_>,
    root_page: PhysBytes,
    shim_windows: &[PhysRange],
) -> PreparedJump {
    let arena_pa = arena_pa.0;
    // riscv64 has no `BootArchImpl` alias at module scope (its direct boot
    // face names the type explicitly); the v1 sender names it here instead.
    #[cfg(target_arch = "riscv64")]
    use minix_arch::arch::boot_arch::Riscv64BootArch as BootArchImpl;
    // Entry-state contract (T2): no maskable interrupt may run while the
    // bootstrap root is being built and activated. The firmware handed over
    // with IF=1; `paging.enable` switches CR3 before the image trampoline's
    // own `cli` would run, so close the window here, on the shim side.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }
    let support = boot_validate_and_prepare::<BootArchImpl>(
        report.kern_virt_base,
        report.kern_phys_base,
        report.kern_size,
    );
    let layout = DirectMapLayout::<BootArchImpl>::from_arch();
    let kernel_layout = KernelLayout {
        virtual_base: report.kern_virt_base.0,
        physical_base: report.kern_phys_base.0,
        size: report.kern_size,
    };
    let policy = plan_boot_pages::<BootArchImpl>(&support, kernel_layout, &layout)
        .unwrap_or_else(|e| panic!("enter_kernel_image_v1: page-size policy: {e}"));
    let (bump_base, bump_end) =
        boot_alloc::boot_alloc_region().expect("enter_kernel_image_v1: bump region not registered");
    // Minimal boot identity windows (BOOT-SCOPE 2a): shim image/stack/temp
    // buffers + kernel LMA + bootstrap pool + firmware tables + device windows.
    // The Direct Map is established later by the kernel and covers everything
    // else; nothing outside these windows is disturbed before then.
    let mut windows = BootWindows::new();
    for range in shim_windows {
        windows.push(range.start().0, range.end().0);
    }
    push_common_boot_windows(
        &mut windows,
        report.kern_phys_base.0,
        report.kern_size,
        root_page.0,
        Some((bump_base, bump_end)),
        report.boot_modules,
        report.platform_sources,
    );
    windows.push(
        arena_pa,
        arena_pa
            .checked_add(ARENA_CAP as u64)
            .expect("enter_kernel_image_v1: arena overflows u64"),
    );
    // x86 boots APs through a real-mode trampoline allocated from the lowest
    // free page (SMP setup happens after kmain starts). Keep the low 2 MiB
    // identity-mapped; it is boot work, not a general low-memory window.
    // aarch64/riscv64 wake secondary CPUs through PSCI/SBI and never touch
    // low physical memory, so the window is x86-only.
    #[cfg(target_arch = "x86_64")]
    windows.push(0, DEV_PIECE);
    let windows = windows.check(support.phys_addr_limit());
    build_bootstrap_root_and_enable::<BootArchImpl>(root_page, &policy.kernel(), windows);
    let bump_next = boot_alloc::boot_alloc_next();
    // SAFETY: `arena_pa` is the `ARENA_CAP` arena the boot-shim allocated
    // below `boot_dm_admissible_end` (LOADER_DATA on UEFI legs, bump pages on
    // the U-Boot shim leg); the root just enabled maps
    // that physical range identity and writable, and this function owns the
    // patch phase (the shim is the only other party and never writes again).
    let arena = unsafe { core::slice::from_raw_parts_mut(arena_pa as *mut u8, ARENA_CAP) };
    patch_runtime_fields_in(
        arena,
        RuntimeFields {
            root_page: root_page.0,
            bump_base,
            bump_next,
            bump_end,
        },
    );
    // The `black_box` is a compiler barrier, not a value sanitizer: it stops
    // LLVM from folding the branch target into a compile-time literal, which
    // would break the inline-asm `in(reg)` constraint below. Keep it on the
    // inner `u64` and wrap after.
    let entry = VirBytes(core::hint::black_box(report.kern_virt_base.0));
    PreparedJump {
        entry,
        arena_pa: PhysBytes(arena_pa),
    }
}

/// Build the root, then jump into the standalone image (stable public entry).
#[cfg(all(
    not(feature = "mock"),
    any(
        target_arch = "aarch64",
        target_arch = "x86_64",
        target_arch = "riscv64"
    )
))]
pub fn enter_kernel_image_v1(
    arena_pa: PhysBytes,
    report: &minix_boot::BootReport<'_>,
    root_page: PhysBytes,
    shim_windows: &[PhysRange],
) -> ! {
    let jump = prepare_kernel_image_v1(arena_pa, report, root_page, shim_windows);
    jump_kernel_image_v1(jump)
}

/// Absolute jump into the standalone image's high-half entry.
///
/// Split from [`prepare_kernel_image_v1`] so `enter_kernel_image_v1` can
/// keep preparation and the absolute jump as two internal steps.
#[cfg(all(
    not(feature = "mock"),
    any(
        target_arch = "aarch64",
        target_arch = "x86_64",
        target_arch = "riscv64"
    )
))]
fn jump_kernel_image_v1(jump: PreparedJump) -> ! {
    // Destructure into the two bare `u64`s the asm arms need. The names stay
    // in one place so a future reader cannot reorder them by accident.
    let PreparedJump {
        entry: VirBytes(entry),
        arena_pa: PhysBytes(arena_pa),
    } = jump;
    // SAFETY: paging is enabled and `entry` maps the executable kernel-image
    // `_start`; the arena is a valid physical address below the identity end.
    // The handoff register is bound to a fixed operand so LLVM cannot
    // overwrite the branch target with an implicit move.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!(
            "isb",
            "br {entry}",
            in("x0") arena_pa,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        __minix_x86_enter_image(entry, arena_pa)
    }

    // riscv64: the receiver entry takes the arena in `a0` (U-Boot bootelf's
    // argv ABI is consumed one layer below, by the boot-shim; this jump is
    // shim→image, and the shim defines the convention). `fence.i` first: the
    // kernel image bytes were just copied by the shim, and `jr` must fetch
    // them with a coherent I-cache.
    #[cfg(target_arch = "riscv64")]
    unsafe {
        core::arch::asm!(
            "fence.i",
            "jr {entry}",
            in("a0") arena_pa,
            entry = in(reg) entry,
            options(noreturn)
        );
    }

}




/// x86-64 machine entry into the standalone image.
///
/// The handoff contract carries the arena in `rbx` across the image
/// trampoline's `.bss` clear. Inline asm cannot use `rbx` (LLVM reserves it),
/// so the move lives in a small global-asm function: SysV passes `entry` in
/// `rdi` and `arena_pa` in `rsi`; the stub moves them to `rax`/`rbx`, clears
/// the interrupt flag and direction flag, and jumps.
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
core::arch::global_asm!(
    r#"
    .section .text
    .globl __minix_x86_enter_image
__minix_x86_enter_image:
    mov rbx, rsi
    mov rax, rdi
    cli
    cld
    jmp rax
    "#
);

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
unsafe extern "sysv64" {
    fn __minix_x86_enter_image(entry: u64, arena_pa: u64) -> !;
}

/// T1 2c/3b receiver-side entry: read the v1 arena through the identity
/// window, land all seven payloads into this image's `.bss` stores, rebuild
/// `KernelInfo`, then resume the additive high-half boot.
///
/// The bootstrap root is already live (the shim built it); this entry must
/// never rebuild it — `new_from_page` would zero the active root that maps
/// this very function.
///
/// # Arena lifecycle contract
///
/// The decode below is the **last read of the arena**. Every payload is
/// copied into a `.bss` store, and `KernelInfo`'s slices point at those
/// stores, not at the arena; `arena_pa` is not retained. The VM free-page
/// list is published much later, in `kmain` → `build_vm_handoff`, which is
/// strictly after this function. Therefore:
///
/// - `LastRead(arena) ≺ Publish(VMFreePages)`, so arena frames that the
///   classification leaves in the conventional map may be reissued to VM;
/// - the arena is still named by a startup-object record (identity, not
///   liveness) and stays in the boot handoff's deduction inputs if a leg
///   classifies it as occupied;
/// - if a future change needs to read the arena after the jump, it must move
///   this function's decode boundary or add the arena to the deduction —
///   otherwise the VM can hand out a live wire image.
#[cfg(all(
    not(feature = "mock"),
    any(
        target_arch = "aarch64",
        target_arch = "x86_64",
        target_arch = "riscv64"
    )
))]
pub fn arch_boot_resume_v1(arena_pa: u64) -> ! {
    // SAFETY: the sender allocated an `ARENA_CAP` arena below the identity
    // bound (LOADER_DATA on UEFI legs, bump pages on the U-Boot shim leg) and
    // the live bootstrap root maps it; nothing writes it
    // after the jump.
    let arena = unsafe { core::slice::from_raw_parts(arena_pa as *const u8, ARENA_CAP) };
    let scalars = decode_envelope(arena)
        .unwrap_or_else(|e| panic!("kernel-image: handoff v1 envelope invalid: {e:?}"));

    // Land the memmap and reserved lists directly into the T5 stores; the
    // decoded values must survive the first process-root switch.
    let memmap_store = unsafe { &mut *crate::globals::MEMMAP_REGION_STORE.get() };
    let memmap_len = decode_memmap(arena, &scalars, memmap_store)
        .unwrap_or_else(|e| panic!("kernel-image: handoff v1 memmap invalid: {e:?}"));
    let memmap_ptr = memmap_store.as_ptr();

    let reserved_store = unsafe { &mut *crate::globals::RESERVED_REGION_STORE.get() };
    let reserved_len = decode_reserved_regions(arena, &scalars, reserved_store)
        .unwrap_or_else(|e| panic!("kernel-image: handoff v1 reserved invalid: {e:?}"));
    let reserved_ptr = reserved_store.as_ptr();

    let mut wire_modules = [WireBootModule {
        name: [0; 16],
        start: 0,
        len: 0,
    }; WIRE_MODULE_CAP];
    let module_len = decode_modules(arena, &scalars, &mut wire_modules)
        .unwrap_or_else(|e| panic!("kernel-image: handoff v1 modules invalid: {e:?}"));
    let name_store = unsafe { &mut *crate::globals::BOOT_MODULE_NAME_STORE.get() };
    let module_store = unsafe { &mut *crate::globals::BOOT_MODULE_STORE.get() };
    for (i, wire) in wire_modules.iter().enumerate().take(module_len) {
        let name_len = wire
            .name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(wire.name.len());
        name_store[i].fill(0);
        name_store[i][..name_len].copy_from_slice(&wire.name[..name_len]);
        // SAFETY: the name pool is a kernel `.bss` static, so the slice lives
        // for the whole run; `from_utf8` validates the bytes before constructing
        // the `str` (the handoff protocol says ASCII, but do not rely on it).
        let bytes: &'static [u8] = unsafe {
            core::slice::from_raw_parts(name_store.as_ptr().add(i) as *const u8, name_len)
        };
        let name: &'static str = core::str::from_utf8(bytes)
            .expect("kernel-image: boot module name must be UTF-8/ASCII per handoff protocol");
        module_store[i] = BootModule {
            name,
            start: PhysBytes(wire.start),
            len: usize::try_from(wire.len)
                .unwrap_or_else(|_| panic!("kernel-image: module length out of range")),
        };
    }
    let module_ptr = module_store.as_ptr();

    let platform_store = unsafe { &mut *crate::globals::PLATFORM_SOURCE_STORE.get() };
    let platform_len = decode_platform_sources(arena, &scalars, platform_store)
        .unwrap_or_else(|e| panic!("kernel-image: handoff v1 platform invalid: {e:?}"));
    let platform_ptr = platform_store.as_ptr();

    let param_store = unsafe { &mut *crate::globals::PARAM_BUF_STORE.get() };
    let param_len = decode_param_buf(arena, &scalars, param_store)
        .unwrap_or_else(|e| panic!("kernel-image: handoff v1 param invalid: {e:?}"));
    let param_ptr = param_store.as_ptr();

    let source_store = unsafe { &mut *crate::globals::SOURCE_RECORD_STORE.get() };
    let source_len = decode_sources(arena, &scalars, source_store)
        .unwrap_or_else(|e| panic!("kernel-image: handoff v1 sources invalid: {e:?}"));
    let source_ptr = source_store.as_ptr();

    let kinfo = KernelInfo {
        // SAFETY: all four pointers reference the `.bss` stores filled above;
        // their lengths are the decoder's validated element counts.
        memmap: unsafe { core::slice::from_raw_parts(memmap_ptr, memmap_len) },
        reserved_regions: unsafe { core::slice::from_raw_parts(reserved_ptr, reserved_len) },
        source_records: unsafe { core::slice::from_raw_parts(source_ptr, source_len) },
        kern_virt_base: VirBytes(scalars.kern_virt_base),
        kern_phys_base: PhysBytes(scalars.kern_phys_base),
        kern_size: scalars.kern_size,
        boot_modules: unsafe { core::slice::from_raw_parts(module_ptr, module_len) },
        platform_sources: unsafe { core::slice::from_raw_parts(platform_ptr, platform_len) },
        param_buf: unsafe { core::slice::from_raw_parts(param_ptr, param_len) },
        authority: scalars.ram_authority,
    };
    kinfo.validate_requiring_sources();
    kinfo.check_boot_postconditions();
    boot_alloc::resume_boot_pt_alloc(scalars.bump_base, scalars.bump_next, scalars.bump_end);
    resume_after_v1_decode(&kinfo, scalars.root_page)
}

/// Finish the v1 handoff on aarch64: the bootstrap root is already live, so
/// only the additive phases may run.
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
fn resume_after_v1_decode(kernel_info: &KernelInfo, root_page: u64) -> ! {
    arch_boot_resume_high_half(kernel_info, PhysBytes(root_page))
}

/// Finish the v1 handoff on x86-64: validate/register the allocators, record
/// the already-live root, land the KernelInfo, add direct-map coverage, then
/// enter `kmain` at its high virtual address. The root must not be rebuilt.
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
fn resume_after_v1_decode(kernel_info: &KernelInfo, root_page: u64) -> ! {
    use crate::boot::HigherHalf;
    use crate::x86_64::higher_half::X86_64HigherHalf;
    resume_additive::<CurrentBootArch, X86_64HigherHalf>(
        kernel_info,
        PhysBytes(root_page),
        // The x86 leg has no establish pre/post landmark — keep no output;
        // an empty string skips the print.
        "",
        "",
    )
}

/// Finish the v1 handoff on riscv64: same additive-only shape as x86-64 —
/// the shim's bootstrap root is already live (`satp` enabled before the
/// jump), so this records the root, lands the KernelInfo, adds direct-map
/// coverage and jumps to `kmain` at its high virtual address. The root must
/// never be rebuilt: `new_from_page` would zero the active root that maps
/// this very function.
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
fn resume_after_v1_decode(kernel_info: &KernelInfo, root_page: u64) -> ! {
    use crate::boot::HigherHalf;
    use crate::riscv64::higher_half::Riscv64HigherHalf;
    resume_additive::<minix_arch::arch::boot_arch::Riscv64BootArch, Riscv64HigherHalf>(
        kernel_info,
        PhysBytes(root_page),
        "",
        "",
    )
}

#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use crate::riscv64::higher_half::Riscv64HigherHalf;
    // Same as x86_64: take KernelInfo by value into the kernel .bss global at
    // entry; jumps and page-table builds all use the global copy, eliminating
    // any reference handed across a `noreturn` asm call in a caller-saved
    // register (a0 on riscv64).
    arch_boot_finish::<minix_arch::arch::boot_arch::Riscv64BootArch, Riscv64HigherHalf>(
        kernel_info,
        root_page,
    )
}

#[cfg(all(test, feature = "mock"))]
pub fn arch_boot_test(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::paging::mock::MockPaging;
    let _info = arch_boot_impl::<minix_arch::arch::boot_arch::MockBootArch>(kernel_info, root_page);
    // In mock context, just verify we reach here without panic — that
    // confirms identity + kernel mapping + paging enable all succeeded.
    mock_kmain_ok()
}

/// Minimal kmain for mock/test context: verifies basic higher-half invariants.
#[cfg(all(test, feature = "mock"))]
fn mock_kmain_ok() -> ! {
    // `arch_boot_impl` already verified the boot flow. We just need to
    // return `!` to satisfy the caller contract. In tests, the caller
    // expects this function to diverge.
    loop {
        core::hint::spin_loop();
    }
}

/// Fail closed when the boot-stage page-table allocator was not registered.
///
/// Takes the observed registration instead of reading the process-wide
/// global, so the missing-registration path is testable without racing other
/// tests that initialize the global allocator. The DM-admissible bound stays
/// in the message because a caller that does register must keep the bump
/// region below it (`establish_boot_dm` fails otherwise).
fn require_boot_region<A: BootArch>(region: Option<(u64, u64)>) -> (u64, u64) {
    region.unwrap_or_else(|| {
        let dm_admissible_end = core::cmp::min(
            IDENTITY_MAP_END,
            <A::DirectMap as minix_arch::DirectMapArch>::VM_DIRECT_MAP_SIZE,
        );
        panic!(
            "arch_boot: boot allocator not registered — the boot-shim must call \
             init_boot_pt_alloc with a DM-admissible region below {dm_admissible_end:#x} \
             before entering the kernel"
        )
    })
}

/// The three kernel-layout scalars every boot entry has to validate.
///
/// They used to be four **positional** parameters
/// (`boot_validate_and_prepare::<A>(kern_virt_base, kern_phys_base,
/// kern_size, kern_stack_top)`), with two of them (`kern_virt_base` and
/// `kern_stack_top`) sharing the `VirBytes` newtype — so swapping those two
/// still compiled, and the two alignment assertions inside the function
/// (`vaddr % 0x1000`, `stack_top % 16`) both still passed. A function call's
/// parameter names never appear at the call site, which makes this a
/// passive-error surface; a struct literal must write the name and the value
/// on the same line, which makes it an active one.
///
fn boot_validate_and_prepare<A: BootArch>(
    kern_virt_base: VirBytes,
    kern_phys_base: PhysBytes,
    kern_size: u64,
) -> BootPageSupport<A> {
    // Per-assertion markers (boot-visibility diagnostics; same approach as
    // the arch_boot entry).
    macro_rules! vmark {
        ($msg:expr) => {{
            #[cfg(not(feature = "mock"))]
            {
                use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
                Console::write_str($msg);
            }
        }};
    }
    vmark!("kernel: v0 enter\n");
    // `validate()` is called later in `kmain`; the assertions here are
    // defense-in-depth — they fail-fast before paging setup begins.
    let kv = kern_virt_base.0;
    let kp = kern_phys_base.0;
    let ks = kern_size;
    // Derived, not received. `checked_add` because a
    // wrapping `kern_virt_base + kern_size` means the image span itself is
    // corrupt, and the alignment check below would then be applied to a
    // bogus stack pointer.
    let kern_stack_top =
        VirBytes(kv.checked_add(ks).expect("arch_boot: kern_virt_base + kern_size overflows u64"));

    // kern_size must be positive
    assert!(ks > 0, "arch_boot: kern_size must be > 0");

    // kern_phys_base must be page-aligned (at least 4KB)
    assert!(
        kp.is_multiple_of(0x1000),
        "arch_boot: kern_phys_base must be page-aligned"
    );

    // kern_virt_base must be page-aligned (at least 4KB)
    assert!(
        kv.is_multiple_of(0x1000),
        "arch_boot: kern_virt_base must be page-aligned"
    );

    // kern_stack_top must be 16-byte aligned (ABI requirement)
    assert!(
        kern_stack_top.0.is_multiple_of(16),
        "arch_boot: kern_stack_top must be 16-byte aligned"
    );

    // The boot-stage page-table allocator must have been registered by the
    // caller before entering the kernel. Every production entry registers it
    // (UEFI x86_64/aarch64, U-Boot, both direct-core `bootface` legs), and so
    // does every test kernel. There used to be a fallback that picked the
    // first DM-admissible conventional-memory range out of the memmap when
    // this check failed; that range can overlap the kernel image, the loaded
    // modules or the arena, so it could hand out live pages. A missing
    // registration means the caller skipped a required step — fail closed.
    vmark!("kernel: v1 asserts ok\n");
    require_boot_region::<A>(boot_alloc::boot_alloc_region());
    vmark!("kernel: v2 boot allocator registered\n");
    if !pt_alloc::is_registered() {
        pt_alloc::register(boot_alloc::boot_pt_alloc);
    }
    vmark!("kernel: v3 pt_alloc ok\n");

    // Capability facts only: no page-size choice happens here. The window
    // table is assembled by the caller after this function returns, so the
    // physical-address bound is checked per window in
    // `assert_boot_windows_addressable`; requiring the platform to cover the
    // whole VA ceiling would re-introduce the retired 4 GiB blanket premise.
    let support = detect_boot_page_support::<A>()
        .unwrap_or_else(|e| panic!("arch_boot: page-size capability facts: {e}"));
    vmark!("kernel: v4 capability facts ok\n");
    support
}

/// Generic boot implementation — works for any `HugePages` impl.
///
/// `P: HugePages` implies `P: Paging`, so we get `new_from_page`, `enable`, `map`
/// from `Paging` plus `HUGE_PAGE_SIZE` and `map_huge` from `HugePages`.
///
/// Returns `&KernelInfo` after paging is enabled, so the caller can decide
/// what to do next (e.g., call `kmain` for the real kernel, or print PASS
/// for a test kernel).
///
/// C: pg_clear() + pg_identity() + pg_mapkernel() + pg_load() + vm_enable_paging()
///    pre_init.c:230-236, pg_utils.c:162/186/204/247
/// Boot per-stage markers (EarlyConsole=COM1, same approach as debug.rs's
/// CurrentEarlyConsole usage; compiled out entirely under mock builds).

pub fn arch_boot_impl<A: BootArch>(kernel_info: &KernelInfo, root_page: PhysBytes) -> &KernelInfo {
    // Step 0: validate KernelInfo, register the allocator, and collect the
    // platform's page-size facts (no choice yet).
    boot_stage!("kernel: entering validate\n");
    let support = boot_validate_and_prepare::<A>(
        kernel_info.kern_virt_base,
        kernel_info.kern_phys_base,
        kernel_info.kern_size,
    );
    boot_stage!("kernel: step0 validate ok\n");

    // One policy for both consumers: the kernel image mapping and the Direct
    // Map. The policy carries the validated layout, so the execution path
    // cannot re-assemble a different one.
    let layout = DirectMapLayout::<A>::from_arch();
    let kernel_layout = KernelLayout {
        virtual_base: kernel_info.kern_virt_base.0,
        physical_base: kernel_info.kern_phys_base.0,
        size: kernel_info.kern_size,
    };
    let policy = plan_boot_pages::<A>(&support, kernel_layout, &layout)
        .unwrap_or_else(|e| panic!("arch_boot: page-size policy: {e}"));

    // Minimal boot identity windows: kernel LMA (contains the direct-boot
    // face and its stack), bootstrap pool, root page, modules, firmware
    // tables and device windows. Direct Map is Step 4.
    let mut windows = BootWindows::new();
    push_common_boot_windows(
        &mut windows,
        kernel_info.kern_phys_base.0,
        kernel_info.kern_size,
        root_page.0,
        boot_alloc::boot_alloc_region(),
        kernel_info.boot_modules,
        kernel_info.platform_sources,
    );
    // Same low 2 MiB requirement as the handoff path: x86 AP startup writes
    // its real-mode trampoline from the lowest free page after kmain starts.
    // aarch64/riscv64 have no real-mode trampoline and do not get the window.
    #[cfg(target_arch = "x86_64")]
    windows.push(0, DEV_PIECE);
    // Direct-core leg: the caller keeps executing on its pre-entry stack —
    // the UEFI firmware stack in the bootstrap carriers, the firmware-given
    // stack in the direct-boot face. The v1 sender covers its own stack via
    // `shim_stack_window` (boot-shim/src/uefi_helpers.rs) before the handoff;
    // the direct-core entry has no sender, and the caller's stack IS the boot
    // path's own stack, so it belongs in the window list under the same
    // category ("shim image/stack", cf. the list documented above). Without
    // it the first stack access after enable() — the `ret` that follows the
    // CR3/TTBR/satp load — faults into an unmapped page with no IDT/VBAR/
    // stvec installed: a silent stop with no diagnostic channel (hello-boot
    // fresh binary: rsp=0x7E6_6470 while the image window was [0x5E0_0000,0x600_0000)).
    // Window shape matches `shim_stack_window`: 4 MiB below the page-aligned
    // SP, rounded down to a 2 MiB boundary. Overlapping windows are harmless
    // (fixed 2 MiB leaves with identical flags).
    //
    // `cfg(not(test))`: on the host the mock tests call this entry in-process,
    // where the real SP is a Linux user-stack address far above the boot VA
    // ceiling — the assert below would fire on a false positive (the host
    // never actually enables paging). On firmware targets the SP always sits
    // inside the boot-mappable range, and if a future port ever violates
    // that, the assert stays fail-closed: omitting the window silently would
    // reintroduce exactly the post-enable silent stop this window prevents.
    #[cfg(not(test))]
    {
        let sp: u64;
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("mov {}, rsp", out(reg) sp, options(nomem, nostack));
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            core::arch::asm!("mov {}, sp", out(reg) sp, options(nomem, nostack));
        }
        #[cfg(target_arch = "riscv64")]
        unsafe {
            core::arch::asm!("mv {}, sp", out(reg) sp, options(nomem, nostack));
        }
        let top = sp.saturating_add(0xFFF) & !0xFFF;
        windows.push(
            top.saturating_sub(4 * 1024 * 1024) & !((1 << 21) - 1),
            top,
        );
    }
    let windows = windows.check(support.phys_addr_limit());

    // Steps 1-3: build the bootstrap root (minimal identity windows + kernel
    // high half + the aarch64 MMIO window), enable paging, and record the
    // active root.
    build_bootstrap_root_and_enable::<A>(root_page, &policy.kernel(), windows);

    // Step 4: Establish Direct Map coverage on the bootstrap root.
    // 07-paging_init_design §6.1: every candidate is preflighted
    // before the first leaf write; both windows share the same policy.
    boot_stage!("kernel: step4 DM coverage (preflight + install)\n");
    // SAFETY: paging is enabled with the bootstrap root active, all table
    // pages are inside the identity-mapped writable range, and boot is
    // single-threaded.
    unsafe { crate::dm_coverage::establish_boot_dm::<A>(kernel_info, root_page, &policy.dm()) }
        .unwrap_or_else(|e| panic!("arch_boot: DM coverage: {e}"));
    boot_stage!("kernel: step4 DM coverage ok\n");

    // Return kernel_info so the caller can decide what to do next.
    kernel_info
}

/// The low-VA device windows the Step-1 identity map must emit as
/// Device-nGnRnE ([ARCH: mmio-device], SD-42).
///
/// - **aarch64**: the platform MMIO window (PL011 GICv3 registers). Both the
///   identity alias here and the high-half alias of Step 2b carry the same
///   attributes on purpose — mapping one physical page with two attribute
///   sets is UNPREDICTABLE per the ARM ARM.
/// - **riscv64**: `kernel_identity_mmio_regions()` — the PLIC window and the
///   NS16550A UART page (`kernel/src/uart_irq.rs` dereferences the UART at
///   VA == PA; a Normal WBWA mapping satisfies QEMU and not real hardware).
/// - **x86_64**: none here — LAPIC/IO-APIC are installed by Step 2c at their
///   high-half device-window slots. `flags_to_pte` encodes `DEVICE` as
///   `PWT|PCD`, so those mappings do not depend on firmware MTRRs.
#[cfg(target_arch = "aarch64")]
fn identity_device_windows() -> &'static [(u64, u64)] {
    use minix_platform::arch::aarch64::{MMIO_PA_BASE, MMIO_WINDOW_LEN};
    const WINDOWS: &[(u64, u64)] = &[(MMIO_PA_BASE, MMIO_WINDOW_LEN)];
    WINDOWS
}

#[cfg(target_arch = "riscv64")]
fn identity_device_windows() -> &'static [(u64, u64)] {
    minix_arch::kernel_identity_mmio_regions()
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "riscv64")))]
fn identity_device_windows() -> &'static [(u64, u64)] {
    &[]
}

/// Device-window piece size used by Step 1 (2 MiB).
const DEV_PIECE: u64 = 1 << 21;

/// Whether one identity piece intersects a device window — those pieces are
/// emitted with `PageFlags::DEVICE`. A window smaller than the piece widens
/// to the whole piece: on riscv the UART's 4 KiB page sits inside the QEMU
/// virt device range, where the surrounding 2 MiB block is not RAM either.
fn identity_piece_is_device(piece: u64, piece_len: u64, windows: &[(u64, u64)]) -> bool {
    windows.iter().any(|&(base, len)| {
        base < piece.saturating_add(piece_len) && piece < base.saturating_add(len)
    })
}

/// Build the bootstrap page-table root and enable paging (Steps 1-3 of the
/// generic boot). Extracted from [`arch_boot_impl`] so the
/// boot-shim→high-half-kernel-image handoff can run the exact same table
/// construction without also running Step 4 (DM) or entering `kmain` — both of
/// which the standalone kernel image performs itself, in the high half.
///
/// On return, paging is enabled with TTBR0/TTBR1 (x86: CR3) both pointing at
/// `root_page`, and the active root is recorded in the kernel global.
/// Capacity of the boot identity-window list. The windows are the pieces of
/// physical memory the pre-DM boot path actually touches: shim image/stack,
/// kernel LMA, root/bump/arena, modules, platform tables and device windows.
const MAX_BOOT_WINDOWS: usize = 180;

/// Lower bound derivable from the fixed producers: at most 3 shim windows,
/// kernel + root + bump + arena (4), 12 modules, 4 platform sources,
/// 2 identity device windows, the direct-core caller's stack window, the
/// x86 low-2 MiB trampoline window, and a full ACPI chain (root + up to 35
/// tables) per platform source. The extra headroom covers future one-off
/// windows; the const assertion turns a capacity regression into a compile
/// error instead of a boot panic.
const BOOT_WINDOW_FLOOR: usize = 3
    + 4
    + minix_boot::NR_BOOT_MODULES
    + minix_boot::WIRE_PLATFORM_CAP
    + 2
    + 1
    + 1
    + ACPI_CHAIN_WINDOWS;

/// ACPI table-chain windows reserved per platform source. riscv64 has no
/// ACPI discovery path, so the module is not compiled there.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const ACPI_CHAIN_WINDOWS: usize =
    minix_platform::acpi::TABLE_CHAIN_MAX * minix_boot::WIRE_PLATFORM_CAP;
#[cfg(target_arch = "riscv64")]
const ACPI_CHAIN_WINDOWS: usize = 0;
const _: () = assert!(MAX_BOOT_WINDOWS >= BOOT_WINDOW_FLOOR);

const EMPTY_PHYS_RANGE: PhysRange = match PhysRange::new(PhysBytes(0), PhysBytes(0)) {
    Some(range) => range,
    None => panic!("empty phys range"),
};

/// A tiny fixed-capacity list of physical windows that must be identity-mapped
/// while the boot path is still running below the Direct Map.
///
/// The element is [`PhysRange`], not a bare `(start, end)` pair: the
/// constructor rejects a reversed range at the type boundary, so a caller
/// cannot swap the endpoints into an inverted window. `PhysRange` still admits
/// an empty range (`start == end`), so `push` rejects it explicitly; the
/// `EMPTY_PHYS_RANGE` fill value below is a sentinel for uninitialized slots
/// and is never exposed by `as_slice`.
struct BootWindows {
    ranges: [PhysRange; MAX_BOOT_WINDOWS],
    len: usize,
}

impl BootWindows {
    const fn new() -> Self {
        Self {
            ranges: [EMPTY_PHYS_RANGE; MAX_BOOT_WINDOWS],
            len: 0,
        }
    }

    fn push(&mut self, start: u64, end: u64) {
        assert!(
            start < end,
            "boot identity window is empty or reversed: [0x{start:x},0x{end:x})"
        );
        assert!(
            self.len < MAX_BOOT_WINDOWS,
            "boot identity windows exceed capacity: len={}, cap={MAX_BOOT_WINDOWS}",
            self.len
        );
        self.ranges[self.len] = PhysRange::new(PhysBytes(start), PhysBytes(end))
            .expect("boot identity window: reversed range rejected by the assertion above");
        self.len += 1;
    }

    /// Add a 2 MiB-aligned window covering `addr`.
    fn push_2m(&mut self, addr: u64) {
        let base = addr & !(DEV_PIECE - 1);
        let end = base
            .checked_add(DEV_PIECE)
            .expect("boot identity window: 2 MiB piece overflows u64");
        self.push(base, end);
    }

    fn as_slice(&self) -> &[PhysRange] {
        &self.ranges[..self.len]
    }
}

fn push_module_windows(windows: &mut BootWindows, modules: &[minix_boot::BootModule]) {
    for module in modules {
        windows.push(
            module.start.0,
            module
                .start
                .0
                .checked_add(module.len as u64)
                .expect("boot window: module span overflows u64"),
        );
    }
}

fn push_platform_windows(
    windows: &mut BootWindows,
    sources: &[minix_boot::PlatformDescSource],
) {
    for source in sources {
        let start = source.phys_addr().0;
        if source.has_len() {
            windows.push(
                start,
                start
                    .checked_add(source.len() as u64)
                    .expect("boot window: platform blob overflows u64"),
            );
        } else {
            // Legacy producer: no length recorded, cover the address's piece.
            windows.push_2m(start);
        }
        // The RSDP blob is only the entry point: XSDT/RSDT and the tables it
        // points at are separate physical allocations. Consume those here,
        // while firmware memory is still identity-accessible, instead of
        // relying on the address's 2 MiB piece to happen to cover them.
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        if source.kind() == minix_boot::RSDP {
            let mut chain = [(0u64, 0u64); minix_platform::acpi::TABLE_CHAIN_MAX];
            // SAFETY: the producer recorded this as an ACPI RSDP; boot is
            // pre-paging, so firmware memory is identity-accessible.
            let n = unsafe {
                minix_platform::acpi::table_chain_windows(start as usize, &mut chain)
            };
            for &(span_start, span_end) in &chain[..n] {
                windows.push(span_start, span_end);
            }
        }
    }
}

fn push_device_windows(windows: &mut BootWindows) {
    for &(base, len) in identity_device_windows() {
        windows.push(
            base,
            base.checked_add(len)
                .expect("boot window: device window overflows u64"),
        );
    }
}

/// Push the window categories shared by the v1 sender and the direct-core
/// entry: kernel LMA, root page, optional bootstrap pool, modules, platform
/// tables and identity device windows. The two callers differ only in their
/// extra windows (shim/arena vs. none), so keeping the common list in one
/// place means a new category cannot be added to one leg and forgotten in the
/// other.
fn push_common_boot_windows(
    windows: &mut BootWindows,
    kern_phys_base: u64,
    kern_size: u64,
    root_page: u64,
    bump: Option<(u64, u64)>,
    modules: &[minix_boot::BootModule],
    platform_sources: &[minix_boot::PlatformDescSource],
) {
    windows.push(
        kern_phys_base,
        kern_phys_base
            .checked_add(kern_size)
            .expect("boot window: kernel span overflows u64"),
    );
    windows.push(
        root_page,
        root_page
            .checked_add(0x1000)
            .expect("boot window: root page overflows u64"),
    );
    if let Some((bump_base, bump_end)) = bump {
        windows.push(bump_base, bump_end);
    }
    push_module_windows(windows, modules);
    push_platform_windows(windows, platform_sources);
    push_device_windows(windows);
}

/// A window list that has passed the platform physical-address bound check.
///
/// The only constructor is [`BootWindows::check`]. Passing the checked type
/// to [`build_bootstrap_root_and_enable`] makes "validate then build" the only
/// expressible order; a caller cannot hand over an unchecked list.
#[derive(Clone, Copy)]
struct CheckedWindows<'a>(&'a [PhysRange]);

impl BootWindows {
    /// Validate every window against the platform physical-address limit and
    /// return the token consumed by the page-table builder.
    fn check(&self, phys_addr_limit: u64) -> CheckedWindows<'_> {
        for range in self.as_slice() {
            let start = range.start().0;
            let end = range.end().0;
            assert!(
                end <= phys_addr_limit,
                "boot identity window [0x{start:x},0x{end:x}) exceeds the platform physical-address bound 0x{phys_addr_limit:x}"
            );
        }
        CheckedWindows(self.as_slice())
    }
}

fn build_bootstrap_root_and_enable<A: BootArch>(
    root_page: PhysBytes,
    policy: &KernelMapPolicy<A>,
    windows: CheckedWindows<'_>,
) {
    let kern_huge = policy.page_size();
    let layout = policy.layout();
    // The bootstrap root is constructed BEFORE the Direct Map exists
    // (Step 4 comes after Steps 1–3), so its zeroing must go through the
    // identity view — the root frame sits inside the declared UEFI windows.
        let mut paging = A::Paging::new_from_page(root_page, minix_arch::paging::PteChannel::Identity);

    // Step 1: Minimal boot identity windows — VA = PA only for the pieces
    // the pre-Direct-Map boot path actually touches (DESIGN: BOOT-SCOPE 2a).
    //
    // C maps 4 GiB with `pg_identity` because the 32-bit kernel runs inside
    // that window; the Rust cross-image leg only needs its shim image/stack,
    // the kernel LMA span, the bootstrap pool (root/bump/modules/arena), the
    // firmware tables it reads, and the device windows it drives. Mapping the
    // whole low 4 GiB kept every hole PRESENT and hid wild accesses; the
    // minimal windows turn those into page faults.
    //
    // The caller supplies the windows; the builder rounds each to 2 MiB and
    // emits Device flags for any piece inside a device window. Overlapping
    // windows are harmless for a different reason than "the error is
    // ignored": every write is a fixed 2 MiB leaf, the flags depend only on
    // the piece address (not on which window supplied it), and a second
    // `map_huge` overwrites the same leaf with identical flags. `map_huge`
    // returns `AlreadyMapped` only when a 1 GiB leaf already occupies the
    // slot, which cannot happen here because Step 1 never emits 1 GiB leaves.
    let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let device_windows = identity_device_windows();
    for range in windows.0 {
        let raw_start = range.start().0;
        let raw_end = range.end().0;
        let start = raw_start & !(DEV_PIECE - 1);
        let end = raw_end
            .checked_add(DEV_PIECE - 1)
            .expect("boot identity window end overflows u64")
            & !(DEV_PIECE - 1);
        assert!(
            end <= IDENTITY_MAP_END,
            "boot identity window [0x{raw_start:x},0x{raw_end:x}) exceeds the boot VA ceiling"
        );
        let mut addr = start;
        while addr < end {
            let piece_flags = if identity_piece_is_device(addr, DEV_PIECE, device_windows) {
                PageFlags::kernel_read_write() | PageFlags::DEVICE
            } else {
                id_flags
            };
            // `map_huge` never probes physical memory, so an unbacked hole
            // cannot produce an error. An overlapping 2 MiB window that lands
            // on an already-present 2 MiB leaf returns `AlreadyMapped`; the
            // arm below accepts it because rewriting the same window is
            // harmless. A present 1 GiB leaf is the other possible conflict.
            // Step 1's fresh root never creates one, so the arm is defensive;
            // every other error, including `AllocationFailed`, fails closed
            // here instead of surfacing at the first instruction fetch after
            // enable().
            match paging.map_huge(
                VirBytes(addr),
                PhysBytes(addr),
                PageSize::MIB_2,
                piece_flags,
            ) {
                Ok(()) | Err(PageTableError::AlreadyMapped) => {}
                Err(error) => panic!("boot identity map: map_huge failed: {error:?}"),
            }
            addr += DEV_PIECE;
        }
    }

    // Step 2: Kernel high-address mapping.
    // C: pg_mapkernel() — pg_utils.c:186
    //
    // When kern_virt_base == kern_phys_base, the kernel mapping is
    // identical to the identity mapping (Step 1). Skip it to avoid
    // overwriting L2 entries (especially on riscv64 where the same
    // VPN[2] slot would be written twice with potentially different
    // page sizes, corrupting the identity mapping).
    // See 02-higher-half-kernel.md for the full analysis.
    // Use getter methods (preferred API).
    let kern_virt = layout.virtual_base;
    let kern_phys = layout.physical_base;
    if kern_virt != kern_phys {
        let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let mut offset = 0u64;
        while offset < layout.size {
            paging
                .map_huge(
                    VirBytes(kern_virt + offset),
                    PhysBytes(kern_phys + offset),
                    kern_huge,
                    kern_flags,
                )
                .expect("kernel map: map_huge failed — kernel image mismatch");
            offset += kern_huge.bytes();
        }
    }

    // Step 2b (aarch64 only): map the device MMIO window into the
    // kernel high half (TTBR1) of the bootstrap root.
    //
    // aarch64 has no PIO — the PL011 console and GICv3 are MMIO-only. During
    // boot they are reached through the low-half identity mapping (VA==PA),
    // which lives in the TTBR0 half. `switch_address_space` swaps TTBR0 to a
    // per-process root and issues `tlbi alle1is`, so the identity entry is
    // gone after the first switch and any kernel MMIO access faults. TTBR1 is
    // pinned to the bootstrap root and never switched, so a high-half alias of
    // the window survives every context switch. Flags: supervisor RW; the
    // window carries EXECUTABLE=off so EL0 can neither reach nor fetch it
    // (it is already supervisor-only). NOTE: `kernel_read_write` maps to
    // XN(bit54), which only gates EL0 fetch; forbidding EL1 fetch would need
    // PXN(bit53), which `flags_to_pte` does not currently emit. The alias is
    // mapped with `PageFlags::DEVICE`: these are MMIO registers, and
    // Step 1's identity alias of the same window carries the same Device
    // attributes so the two mappings of one physical page agree (mismatched
    // attributes are UNPREDICTABLE per the ARM ARM).
    #[cfg(target_arch = "aarch64")]
    {
        use minix_platform::arch::aarch64::{MMIO_PA_BASE, MMIO_VA_BASE, MMIO_WINDOW_LEN};
        const MMIO_BLOCK: u64 = 1 << 21; // 2 MiB — map_huge() block for size < 1 GiB
        // The while loop below steps `off` by MMIO_BLOCK over MMIO_WINDOW_LEN
        // and calls map_huge with 2 MiB-aligned VA/PA. Pin the preconditions so
        // a future window-size / base change that would silently leave a tail
        // unmapped or break block alignment fails at compile time.
        const _: () = assert!(
            MMIO_WINDOW_LEN % MMIO_BLOCK == 0
                && MMIO_PA_BASE % MMIO_BLOCK == 0
                && MMIO_VA_BASE % MMIO_BLOCK == 0
        );
        let mmio_flags = PageFlags::kernel_read_write() | PageFlags::DEVICE;
        let mut off = 0u64;
        while off < MMIO_WINDOW_LEN {
            paging
                .map_huge(
                    VirBytes(MMIO_VA_BASE + off),
                    PhysBytes(MMIO_PA_BASE + off),
                    PageSize::MIB_2,
                    mmio_flags,
                )
                .expect("kernel map: MMIO high-half window map_huge failed");
            off += MMIO_BLOCK;
        }
    }

    // Step 2c (PD-22 step 2): install the kernel device window into the
    // bootstrap root — the same VAs `map_kernel` installs into process
    // roots (segment 5). Boot-phase MMIO consumers (PLIC init, LAPIC init)
    // run under THIS root and address devices through the same high-half
    // VAs the runtime uses (ruling §2.1 option A: one device VA for boot
    // root and process roots; the low identity map stays for its own
    // scope, so this is additive duplication of the device regions only).
    {
        // ALL regions: boot-phase MMIO consumers run under THIS root
        // (controller init on both arches; x86 AP bring-up ICR writes), and
        // the slots are 2 MiB-aligned by construction, so each region
        // installs as huge leaves along the proven step-1/2 path. The 4 KiB
        // map() path hangs here (pre-DM-establishment channel writes) and
        // must not be reintroduced for bootstrap-root installs.
        use minix_arch::paging::Paging as _;
        let regions = minix_arch::arch::device_window::device_window_regions();
        // Architectures with an empty device table (aarch64: the interrupt
        // controller is reached via system registers and devices via the
        // TTBR1-pinned high window) have nothing to install here — an
        // empty table is a legitimate form, not an error.
        if regions.is_empty() {
            boot_stage!("kernel: step2c device window empty (skip)\n");
        } else {
            for region in regions {
                if !region.bootstrap {
                    continue; // runtime-only consumer: process roots carry it
                }
                let va_base = minix_arch::arch::device_window::slot_va_base(regions, region.slot);
                let mut off = 0u64;
                while off < region.size {
                    paging
                        .map_huge(
                            VirBytes(va_base + off),
                            PhysBytes(region.phys_base + off),
                            minix_arch::paging_ext::PageSize::MIB_2,
                            minix_arch::arch::device_window::device_window_flags(),
                        )
                        .expect("device window map into bootstrap root failed");
                    off += 2 << 20;
                }
            }
        }
    }

    boot_stage!("kernel: step1+2 mappings ok\n");
    // Step 3: Enable paging.
    // SAFETY: Steps 1+2 set up identity mapping covering current RIP.
    let _root_phys = unsafe { paging.enable() };

    // Record the bootstrap root physical address in the kernel global so
    // later phases (e.g., `init_proc_and_boot` loading the VM ELF) can
    // wrap it via `Paging::from_active_root` and add more mappings to the
    // *same* page table. The `paging` instance is about to be dropped,
    // but the page table it created remains active in the MMU.
    //
    // We use the `root_page` parameter (not `_root_phys`'s return value)
    // because some architectures' `enable()` returns a different value
    // (e.g., the satp-encoded value on riscv64, not the raw physical
    // address). The parameter is always the raw physical address.
    set_current_root_phys(root_page);

    // The high-half MMIO window (mapped in Step 2b) is now live in
    // TTBR1 (both TTBR0 and TTBR1 point at the bootstrap root until the first
    // switch, and TTBR1 keeps it forever). Route the early console to the
    // TTBR1-resident alias so serial output survives `switch_address_space`.
    #[cfg(target_arch = "aarch64")]
    minix_plat::arm64::early_console::use_high_mmio_window();
}

/// Land a boot-shim slice into a kernel `.bss` store (DESIGN-12).
///
/// The capacity check is fail-closed; the same-address case is skipped so
/// `resume_additive` can re-store the slices it decoded into the store
/// without `ptr::copy` aliasing UB.
fn land_slice<T: Copy>(src: &[T], dst: &mut [T], what: &str) {
    assert!(
        src.len() <= dst.len(),
        "store_kernel_info: {what} exceeds the .bss landing pad"
    );
    if src.as_ptr() != dst.as_ptr() {
        // SAFETY: `src.len() <= dst.len()`, the ranges are distinct unless the
        // pointer equality guard above skipped, and `ptr::copy` tolerates
        // partial overlap.
        unsafe {
            core::ptr::copy(src.as_ptr(), dst.as_mut_ptr(), src.len());
        }
    }
}

/// Stores the boot `KernelInfo` into the process-global slot and returns a
/// reference into that slot.
///
/// Production has two call sites: `arch_boot_finish` (a real copy) and
/// `resume_additive` (same-address skip). `kmain` reads the slot through
/// `kernel_info()` and does not re-store; bootstrap test kernels that bypass
/// kmain call this explicitly before any consumer of [`KERNEL_INFO`].
///
/// The returned reference points at the `.bss` global, not at the caller's
/// input. The slot is written exactly once per leg: a second
/// store panics instead of updating the slot in place.
pub fn store_kernel_info(kernel_info: &KernelInfo) -> &'static KernelInfo {
    // The slot is written exactly once per leg, before kmain. A second
    // store would overwrite a live `&'static KernelInfo` handed out to
    // earlier readers — fail closed instead of silently aliasing.
    // SAFETY: the one-shot slot; read before the single write below.
    assert!(
        unsafe { (*crate::globals::KERNEL_INFO.get()).is_none() },
        "store_kernel_info: a second store would rewrite a live &'static slot"
    );
    // SAFETY: boot is single-threaded (before BKL exists); one write, then
    // read-only for the rest of the run.
    unsafe {
        // The `reserved_regions` payload lives in the boot-shim's
        // UEFI-pool heap — a `&'static` slice whose backing pages are only
        // mapped by the firmware's 1:1 table. After the higher-half jump
        // the kernel's own tables do not replay that mapping and the same
        // VA reads zeros (real machine: res=127 zeroed=127 when the
        // classification point read the reserved entries). Land the bytes
        // in kernel .bss — mapped
        // for the whole run — and hand the global copy that pointer.
        // `ptr::copy` (not `copy_nonoverlapping`) because a partially
        // overlapping source would be UB under the latter; the exact
        // same-address case (resume_additive re-stores the decoded slices) is skipped by
        // the equality guard below.
        let store = &mut *crate::globals::RESERVED_REGION_STORE.get();
        let src = kernel_info.reserved_regions;
        land_slice(src, store, "reserved_regions");
        // The same disease takes the `memmap` and
        // `boot_modules` payloads — both live in the boot-shim's UEFI-pool
        // heap and their pages do not survive the rest of boot (real
        // machine vh1: all conventional memmap entries read back length 0
        // at the `vm_handoff::classify` point while still intact at Step 4
        // DM-coverage establishment → VM free list empty → VM panic at
        // `servers/vm/src/boot.rs:159`). Land both here before publishing
        // the global copy; module names go into a fixed byte pool, copied
        // strlcpy-style (15-byte bound, same as `vm_handoff::copy_name`).
        let mstore = &mut *crate::globals::MEMMAP_REGION_STORE.get();
        let msrc = kernel_info.memmap;
        land_slice(msrc, mstore, "memmap");
        let bstore = &mut *crate::globals::BOOT_MODULE_STORE.get();
        let bnames = crate::globals::BOOT_MODULE_NAME_STORE.get();
        let bsrc = kernel_info.boot_modules;
        assert!(
            bsrc.len() <= bstore.len(),
            "store_kernel_info: boot_modules exceeds the .bss landing pad"
        );
        if bsrc.as_ptr() != bstore.as_ptr() {
            for (i, module) in bsrc.iter().enumerate() {
                let bytes = module.name.as_bytes();
                let n = bytes.len().min(crate::globals::BOOT_MODULE_NAME_LEN - 1);
                // The pool reference goes through the raw pointer so the
                // landed `&'static str` is derived from the `static` itself
                // (store-lifetime backing), not from a local borrow.
                let slot = &mut (*bnames)[i];
                // `copy_from_slice` requires non-overlapping source and
                // destination. A public caller may pass a `name` that already
                // points into this landing pool while the module slice itself
                // lives elsewhere, so use the overlap-safe `ptr::copy` here.
                // SAFETY: `slot` is a live 16-byte name slot, `bytes` is a
                // live `&str` slice, and `n <= BOOT_MODULE_NAME_LEN - 1`.
                unsafe {
                    core::ptr::copy(bytes.as_ptr(), slot.as_mut_ptr(), n);
                }
                // Boot-module names are expected to be ASCII literals
                // (boot-shim `MODULE_NAMES`), but this function is public and
                // an arbitrary caller could pass a non-ASCII name whose
                // 15-byte truncation splits a multi-byte sequence. Validate
                // instead of relying on the caller: on an invalid tail, keep
                // the longest valid UTF-8 prefix.
                let name: &'static str = match core::str::from_utf8(&slot[..n]) {
                    Ok(s) => s,
                    Err(e) => core::str::from_utf8(&slot[..e.valid_up_to()]).unwrap_or("?"),
                };
                bstore[i] = minix_boot::BootModule {
                    name,
                    start: module.start,
                    len: module.len,
                };
            }
        }
        // The platform-source list and the boot parameter bytes live
        // in the same firmware pool as the three payloads above, so they get
        // the same landing treatment.
        let pstore = &mut *crate::globals::PLATFORM_SOURCE_STORE.get();
        let psrc = kernel_info.platform_sources;
        land_slice(psrc, pstore, "platform_sources");
        let param = &mut *crate::globals::PARAM_BUF_STORE.get();
        let csrc = kernel_info.param_buf;
        assert!(
            csrc.len() <= param.len(),
            "store_kernel_info: param_buf exceeds the .bss landing pad"
        );
        if csrc.as_ptr() != param.as_ptr() {
            // Fixed-length semantics: zero the pad, then copy the bytes so a
            // short buffer has a defined all-zero tail.
            param.fill(0);
            param[..csrc.len()].copy_from_slice(csrc);
        }
        // Provenance records are the last slice with the same disease:
        // the v1 decoder already lands them in `.bss`, but the direct and
        // auxiliary faces can hand over a firmware-pool pointer. Land it
        // here so `store_kernel_info` really makes the snapshot self-contained.
        // There is no startup-object class: its only reader was a strict-gate
        // check, which the map tables now serve.
        let sstore = &mut *crate::globals::SOURCE_RECORD_STORE.get();
        let ssrc = kernel_info.source_records;
        land_slice(ssrc, sstore, "source_records");
        let mut k = *kernel_info;
        k.reserved_regions = core::slice::from_raw_parts(store.as_ptr(), src.len());
        k.memmap = core::slice::from_raw_parts(mstore.as_ptr(), msrc.len());
        k.boot_modules = core::slice::from_raw_parts(bstore.as_ptr(), bsrc.len());
        k.platform_sources = core::slice::from_raw_parts(pstore.as_ptr(), psrc.len());
        k.param_buf = core::slice::from_raw_parts(param.as_ptr(), csrc.len());
        k.source_records = core::slice::from_raw_parts(sstore.as_ptr(), ssrc.len());
        *KERNEL_INFO.get() = Some(k);
    }
    crate::kernel_info().expect("store_kernel_info: KERNEL_INFO just written")
}

/// Kernel main — called after the higher-half transition.
///
/// This function runs at the kernel's high virtual address.
/// It orchestrates the six-phase boot sequence:
///
/// [ARCH: kernel-text-rx] Narrow the bootstrap root's high-half kernel
/// text mapping from coarse RWX huge leaves down to RX 4 KiB pages (the
/// bootstrap-root half of the kernel-text W^X work; per-process roots get
/// their RX text from `map_kernel` at construction time).
///
/// Window contract (verified per architecture in
/// `rewrite-notes/01-stage-kernel/c-board-rulings-brief.md` §11.2): the
/// call site runs before `smp_init` with the BKL held. On all three
/// architectures the AP release calls live inside `smp_init`, and an AP
/// never executes kernel code nor loads the kernel root before its release
/// (the riscv64 loser-park `wfi` loop runs with paging off), so no CPU but
/// the BSP can hold a stale TLB entry for this root; the PTE write path's
/// built-in local invalidation is therefore sufficient and no cross-core
/// shootdown is needed.
///
/// Promise scope (the precise wording the design review settled on):
/// per-process roots carry RX text from `map_kernel` (real text page count
/// from `vm_handoff`); this function only narrows the bootstrap root's
/// high-half alias; the bootstrap root's low 4 GiB identity mapping (coarse
/// RWX leaves) stays as-is — the writable-executable alias of text pages on
/// the bootstrap root is deliberately not removed. On the text/data
/// boundary leaf, 4 KiB pages past `text_end` keep their inherited flags
/// (the bootstrap root's data region keeps its existing coarse behavior).
#[cfg(all(not(feature = "mock"), not(feature = "qemu_test")))]
fn harden_kernel_text() {
    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
    use minix_arch::CurrentPaging;
    use minix_arch::paging::PageFlags;
    use minix_arch::paging::Paging as _;

    const MIB_2: u64 = 2 << 20;
    const PAGE: u64 = 4096;
    Console::write_str("kernel: text harden enter\n");

    let kinfo = crate::kernel_info().expect("harden_kernel_text: KERNEL_INFO store missing");
    let kern_virt = kinfo.kern_virt_base.0;
    // The medany direct-boot form has a virt == phys image view (step 1
    // only builds the high-half alias when virt != phys) — there is no
    // high-half alias to split; the identity view stays as-is per the
    // promise scope above.
    if kern_virt == kinfo.kern_phys_base.0 {
        return;
    }
    // Placeholder handoff (non-kernel-text-rx builds report data_pages ==
    // 0): skip the split — "whole span as text" would RX the bootstrap
    // root's data region too and the first kernel data write would fault.
    // Production images (kernel-text-rx) carry the real boundary; the
    // placeholder always takes the legacy path.
    let text_pages = crate::vm_handoff::kern_text_pages(kinfo);
    if kinfo.kern_size / 4096 == text_pages {
        return;
    }
    let root = crate::current_root_phys()
        .expect("harden_kernel_text: live root not recorded (set_current_root_phys)");
    let text_end = kern_virt + text_pages as u64 * PAGE;

    // SAFETY: the bootstrap root is the currently-loaded root (paging live,
    // DM established by the boot flow's step 4); `from_active_root` wraps
    // it without zeroing on the **KernelDm** channel (the kernel boot face
    // — PTE reads/writes go through the kernel DM window, each write
    // carrying a local TLB invalidation). The VmDm-channel
    // `adopt_active_root` is the VM-side constructor: in kernel boot
    // context the VM window is unreachable and its first PTE read faults
    // (observed: kernel-leg load page fault at VM_DM+root_paddr+0x800).
    // Single hart + BKL per the window contract above.
    let mut pt = unsafe {
        <CurrentPaging as minix_arch::paging::Paging>::from_active_root(
            root,
            minix_arch::paging::PteChannel::KernelDm,
        )
    };
    Console::write_str("kernel: text harden adopted\n");

    let mut leaf = kern_virt; // image base, 2 MiB aligned (link-script closure)
    while leaf < text_end {
        // split_huge splits all the way down (1 GiB -> 2 MiB -> 4 KiB in
        // one call); idempotent on an already-4 KiB leaf (returns false).
        // SAFETY: as above — live-root edit under the boot window contract.
        unsafe {
            pt.split_huge(minix_types::VirBytes(leaf))
                .expect("harden_kernel_text: split_huge failed");
        }
        let leaf_text_end = (leaf + MIB_2).min(text_end);
        let mut page = leaf;
        while page < leaf_text_end {
            // SAFETY: as above — live-root edit; the write path flushes.
            unsafe {
                pt.update_flags(minix_types::VirBytes(page), PageFlags::kernel_executable())
                    .expect("harden_kernel_text: update_flags failed");
            }
            page += PAGE;
        }
        leaf += MIB_2;
    }
    Console::write_str("kernel: text harden done\n");
}

/// Phase A (this function): Entry — validate kinfo, allow kernel alloc
/// Phase B: cstart — prot_init + clock + intr (software only) + arch_init
/// Phase C: proc_init + arch_boot_proc
/// Phase D: arch_post_init + memory_init
/// Phase E: (no boot-time action — C `system_init` became the `enum Syscall`
///          dispatch; per-call work runs when syscalls arrive)
/// Phase F: bsp_finish_booting (timer program+register+gate) + switch_to_user
///
/// C: main.c:115-147
#[cfg(all(not(feature = "mock"), not(feature = "qemu_test")))]
pub fn kmain(_kernel_info: &KernelInfo) -> ! {
    use minix_platform::platform_desc;

    // Root-cause fix on the consuming side: switch to the KernelInfo copy that
    // arch_boot stored in the .bss global; stop trusting the `kernel_info`
    // parameter reference that lands on the kernel stack. Evidence: the entry
    // RDI was already correct (the jump uses the .bss RIP-relative address)
    // and the global slot's content bootstrap_len==0 was intact; but the
    // parameter reference was spilled by the compiler to the kernel stack
    // (near kern_stack_top) and reloaded from a stack slot corrupted by
    // aliasing/overwrites — every field read through the parameter was .rdata
    // ASCII garbage, so validate panicked. The global slot lives in .bss, does
    // not go through that stack area, and is the only trustworthy source.
    // This shadow can be deleted once the diagnostics below confirm it.
    let kernel_info: &KernelInfo =
        crate::kernel_info().expect("kmain: KERNEL_INFO must be stored by arch_boot before jump");
    // Evidence capture (temporary, v3): read RDI in entry asm (the real
    // handoff value) plus the global slot pointer, the current RSP and the
    // bootstrap_len read through the slot.
    // x86_64 gate: rdi/rdx are the x86 SysV handoff registers (aarch64 uses
    // x0, riscv64 uses a0); bare asm without an arch gate breaks the
    // aarch64/riscv64 carrier builds.
    #[cfg(target_arch = "x86_64")]
    {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        let entry_rdi: u64;
        unsafe {
            core::arch::asm!("mov {}, rdi", out(reg) entry_rdi, options(nomem, nostack, preserves_flags));
        }
        let rsp: u64;
        unsafe {
            core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, pure));
        }
        Console::write_str("kernel: preA rdi=");
        Console::write_hex(entry_rdi);
        Console::write_str(" kinfo=");
        Console::write_hex(kernel_info as *const minix_boot::KernelInfo as u64);
        Console::write_str(" rsp=");
        Console::write_hex(rsp);
        Console::write_str("\n");
    }
    boot_stage!("kernel: kmain Phase A enter\n");
    // Phase A: Entry
    // Validate KernelInfo invariants before any use.
    kernel_info.validate();
    // Cross-boot-form postcondition check. `validate()` only checks
    // non-address invariants; this layer additionally checks the one
    // remaining relation: `kern_virt_base > u32::MAX` (the image must not
    // sit below 4 GiB). The derived-scalar relations (entry inside the
    // image / user stack below the kernel base / kernel stack not landing in
    // the user half) have no separate fields to read. kmain is the single
    // common end point of all five boot forms, so calling it here applies to
    // every form.
    kernel_info.check_boot_postconditions();
    boot_stage!("kernel: kmain A.1 validate ok\n");

    // Initialize the early console first so any boot diagnostic output
    // uses the correct baud rate / UART configuration.
    // C: ser_init(); the EarlyConsole trait owns the initialization.
    {
        use crate::kmess::{EarlyConsole, KernelConsole as Console};
        Console::init();
    }

    // No re-store here. The global slot is written once per leg before
    // kmain (prepare_kernel_image_v1 / rust_image_main), and
    // `kernel_info` above already reads that .bss copy. Re-storing the
    // identical value has no effect, and a second store would rewrite a
    // live &'static slot (a guard failure, see store_kernel_info). C parity:
    // main.c reads kerninfo and never re-stores.
    boot_stage!("kernel: kmain A.1b console+kinfo ok\n");

    // Phase A.2 is intentionally empty: the kernel keeps no mutable copy of
    // the boot memmap. C's `kinfo.memmap[]` bookkeeping (pre_init.c add/cut)
    // is served by construction here — every boot-time allocator excludes
    // the occupied ranges it must not touch (`vm_boot_exclusions` for the VM
    // bootstrap pools, `classify` for the handoff free list), and the only
    // firmware blob read after this point (the DTB, Phase A.5) is consumed
    // before the first allocator can reach it. Nothing may re-read that blob
    // afterwards; a future consumer must copy it or register it as a live
    // object. Boot module blobs, the kernel
    // image, the self root page and the boot bump region stay reserved for
    // the whole run by the A2 deduction record.
    // Phase A.5: Platform discovery — initialize PlatformContext from KernelInfo.
    // This MUST run before init_clock_and_interrupts() because the clock,
    // interrupt controller, and arch_init all read hardware parameters
    // from the global platform descriptor (see 04-platform-discovery.md §3.4).
    // SAFETY: Single-threaded boot context; no concurrent access.
    unsafe {
        minix_platform::init_from_kinfo(kernel_info);
    }

    boot_stage!("kernel: kmain A.5 platform ok\n");
    // Phase B: cstart — protection + clock + interrupt
    // GS.BASE three-point sampling (kmain entry / after init_protection /
    // before the RS switch). current_cpu_id's gs:0x10 identity anchor took a
    // PF@cr2=0x10 after the first RS switch; this sampling determines whether
    // GS was "never programmed" or "cleared midway".
    // GS_BASE/KERNEL_GS_BASE are x86-only MSRs and rdmsr compiles only on
    // x86_64 (the aarch64 carrier's kernel lib once failed to compile because
    // this block lacked an arch gate).
    #[cfg(target_arch = "x86_64")]
    {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        let (g_lo, g_hi): (u32, u32);
        // SAFETY: rdmsr of GS_BASE is a read-only side-effect-free probe.
        unsafe {
            core::arch::asm!("rdmsr", in("ecx") 0xC000_0101u32, out("eax") g_lo, out("edx") g_hi, options(nomem, nostack))
        };
        let (k_lo, k_hi): (u32, u32);
        // SAFETY: rdmsr of KERNEL_GS_BASE is read-only.
        unsafe {
            core::arch::asm!("rdmsr", in("ecx") 0xC000_0102u32, out("eax") k_lo, out("edx") k_hi, options(nomem, nostack))
        };
    }
    init_protection(kernel_info); // prot_init equivalent
    init_clock_and_interrupts(); // clock + intr + arch_init (covered in 05)

    // The boot clock source's three-part sequence — program the source,
    // register the local-timer hook, open the delivery gate — runs EXACTLY
    // ONCE, at the C `boot_cpu_init_timer` position, inside
    // `bsp_finish_booting` Step 6 (the terminal Phase-F call below).
    // `boot_init_timer` must stay the single touch point: a second init
    // here would leave the hook unregistered, the pin2 mask unopened, and
    // ticks never arriving; Phase F already wires `bsp_finish_booting`.
    // See 08-system-init-boot-finish.md §4.5.

    boot_stage!("kernel: kmain B clock+intr ok\n");
    #[cfg(target_arch = "x86_64")]
    {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        let (g_lo, g_hi): (u32, u32);
        // SAFETY: rdmsr probe, see gs0 above.
        unsafe {
            core::arch::asm!("rdmsr", in("ecx") 0xC000_0101u32, out("eax") g_lo, out("edx") g_hi, options(nomem, nostack))
        };
    }
    // Phase B.5: kernel information page — build, user-map, publish.
    // From here on the MINIX_KERNINFO IPC call (= 6) answers OK with the
    // page address instead of EBADCALL (C: proc.c:685-693, publication at
    // memory.c:913). Runs before proc_init so boot processes are born with
    // the page already queryable, mirroring the C order where the cstart
    // kuserinfo fill (main.c:438-440) precedes proc_init.
    kerninfo::init_kerninfo(kernel_info);

    boot_stage!("kernel: kmain B.5 kerninfo page ok\n");
    // Phase C: proc_init + arch_boot_proc
    // Populates the global `PROC_TABLE` / `PRIV_TABLE` statics.
    init_proc_and_boot(kernel_info); // covered in 06

    // Phase C.5: IPCF_POOL_INIT — initialize IPC filter pool
    // C: IPCF_POOL_INIT() — main.c:158-162 (called after proc_init)
    // In C, this is memset(&ipc_filter_pool, 0, sizeof(ipc_filter_pool)).
    // In Rust, the pool is already zero-initialized (all slots = None),
    // but we explicitly create and store it for clarity and to match
    // the C boot sequence.
    // SAFETY: This runs during boot (single-threaded, before BKL needed).
    unsafe {
        *IPC_FILTER_POOL.get() = crate::ipc_filter::IpcFilterPool::new();
    }

    // Phase C.6: krandom init — mark the global as ready for IRQ entropy.
    // C: `krandom.random_sources = RANDOM_SOURCES;` — main.c:48.
    // In Rust, the static is already initialized via `const fn new()`, so
    // this just sets the init flag for `try_krandom()` callers. Must run
    // before the first IRQ could fire (IRQs are enabled in Phase B's
    // `init_clock_and_interrupts`, but BKL is held until `switch_to_user`).
    crate::krandom::init();

    boot_stage!("kernel: kmain C proc_init ok\n");
    // Phase D: arch_post_init + memory_init → Direct Map readiness check.
    // C: arch_post_init() — protect.c:370 (x86) / protect.c:97 (ARM)
    // C: memory_init() — memory.c:707 (x86) / memory.c:612 (ARM)
    // SAFETY: boot is single-threaded before BKL exists.
    let proc_table = unsafe { crate::proc_table_boot_unchecked() };
    init_post_and_memory(proc_table); // covered in 07

    // Phase E: (no boot-time action) — C `system_init()` registered the
    // syscall table (system.c:168-278 call_vec[]); in Rust the `enum
    // Syscall` + match dispatch IS the table, and the per-call work runs
    // when syscalls arrive. IrqManager (IRQ hook pool) and alarm-timer
    // initialization happen in Phase B / ClockState instead.
    // See 08-system-init-boot-finish.md §4.4

    boot_stage!("kernel: kmain D memory_init ok\n");
    // Phase F: bsp_finish_booting.
    // C reclaims the unpaged-kernel section here (`add_memmap(&kinfo,
    // bootstrap_start, bootstrap_len)`); the Rust port runs higher-half from
    // the first instruction and `KernelInfo` has no bootstrap-span fields,
    // so there is nothing to reclaim and no field to validate.

    // D7: bsp_finish_booting() -> ! — never returns.
    // bsp_finish_booting takes &mut ProcessTable + &mut SmpState to
    // perform step 2 (bill_ptr = idle_proc), step 4 (RTS_PROC_STOP unset
    // for NR_BOOT_PROCS-NR_TASKS boot processes), and step 5/7
    // (cycles_accounting_init + fpu_init on the BSP's CpuLocal).
    // See 08-system-init-boot-finish.md §4.5-4.6
    //
    // The SmpState is created here (single-CPU BSP-only configuration)
    // and stored in the global `SMP_STATE` so that `cpu_load()` and
    // `notify_scheduler()` can reach per-CPU state without it being
    // threaded through every call site.
    //
    // Query the platform's CPU topology (MADT/DTB
    // parsed by minix-platform) instead of hardcoding single-CPU. The
    // SmpState records the total CPU count, but only the BSP is marked
    // READY — APs join the scheduler after boot_ap completes
    // (16-smp.md). Single-CPU QEMU/-smp1 configs get the same
    // behavior as before (nr_cpus=1).
    //
    init_smp_state();

    // C main.c:149 parity — the BKL is acquired ONCE, early in main, before
    // smp_start_aps: the AP bring-up path (wait_for_aps) does its
    // UNLOCK→wait→LOCK dance against this acquisition, and every AP that
    // enters the kernel expects the held-BKL convention. The guard leaks
    // deliberately (transfer) — the BKL stays held until switch_to_user
    // releases it.
    crate::smp::bkl_lock().transfer();

    // [ARCH: kernel-text-rx] Bootstrap-root text split — must run before
    // `smp_init` (window contract in the harden_kernel_text docs: no AP has
    // loaded the kernel root before its release, so local invalidation
    // suffices).
    harden_kernel_text();

    // C main.c:311 parity — full SMP bring-up (smp_start_aps + wait_for_APs
    // inside). C: "if smp_init() returns it means that it failed and we try
    // to finish single CPU booting" — our smp_init embeds the same degrade-
    // and-continue semantics (per-AP timeout skip + tolerant wait), so
    // falling through to bsp_finish_booting is the C-shaped flow. Returns
    // with the BKL held (wait_for_aps re-acquired it after the dance).
    crate::smp::smp_init();

    // SAFETY: boot is single-threaded before BKL exists.
    unsafe {
        let smp_state = crate::smp_state_boot_unchecked();
        let proc_table = crate::proc_table_boot_unchecked();
        bsp_finish_booting(proc_table, smp_state)
    }
}

/// Hardware id of the CPU actually executing the BSP.
///
/// x86 always boots CPU 0, so the topology value is the truth. aarch64 and
/// riscv64 record the elected boot hart in `BOOT_HART_ELECTED` (hart id + 1,
/// 0 = election not run), because QEMU/OpenSBI may enter `_start` on any
/// hart. Falling back to the topology value keeps hosted tests and pre-election
/// callers working.
fn elected_bsp_hw_id(topo: &minix_boot::platform::CpuTopology) -> u64 {
    #[cfg(target_arch = "riscv64")]
    {
        let e = minix_arch::riscv64::ap_early_entry::BOOT_HART_ELECTED
            .load(core::sync::atomic::Ordering::Acquire);
        if e != 0 {
            return e - 1;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        let e = minix_arch::arm64::ap_early_entry::BOOT_HART_ELECTED
            .load(core::sync::atomic::Ordering::Acquire);
        if e != 0 {
            return e - 1;
        }
    }
    #[cfg(target_arch = "x86_64")]
    {
        // CPUID leaf 1 EBX[31:24] is this CPU's initial APIC ID. The
        // firmware's `bsp_id` is only the first enabled MADT record, which
        // ACPI does not require to be the boot CPU; use the executing
        // CPU's identity when it is present in the discovered set.
        let apic_id = core::arch::x86_64::__cpuid(1).ebx >> 24;
        if (0..topo.nr_cpus as usize).any(|i| topo.cpus[i].hw_id == apic_id as u64) {
            return apic_id as u64;
        }
    }
    topo.bsp_id as u64
}

/// Assemble the global SMP state from the platform topology — single file
/// kmain Phase D block (topology from MADT/DTB, BSP marked READY,
/// APs join later), extracted so the L3 bring-up test driver
/// (`test-timer-irq`) replays the production phase in the production order.
///
/// C: the BSP-only SMP state assembly in main()/bsp_finish_booting's
/// callers (arch_smp.c bsp_cpu_init parity).
pub fn init_smp_state() {
    use minix_platform::{PlatformDesc, platform_desc};
    // SAFETY: boot is single-threaded before BKL exists.
    unsafe {
        let topo = platform_desc().cpu_topology();
        // The same configuration bound `smp_init` enforces must hold here:
        // this runs first and indexes the fixed per-CPU state with the
        // topology's logical ids. Panic with the names rather than letting
        // the first out-of-bounds index surface as a bare Rust message.
        assert!(
            topo.nr_cpus as usize <= smp::MAX_CPUS,
            "init_smp_state: topology reports {} CPUs, MAX_CPUS is {}",
            topo.nr_cpus,
            smp::MAX_CPUS
        );
        // The DTB/ACPI `bsp_id` is a hint, not the truth: OpenSBI may hand
        // control to any hart (QEMU -smp 4 has booted hart 1 and hart 2), and
        // the entry election cell records the real one. Resolve it here,
        // before per-CPU state is seeded, so the BSP's READY flag, ptproc and
        // clock state all land on the CPU that is actually running.
        //
        // If no topology entry matches the elected/hinted hardware id,
        // `bsp_id` must not be used as an index: it is a hardware id, not a
        // logical slot, and can name the wrong CPU or run past `nr_cpus`.
        // Fall back to logical slot 0 — the same bounded conservative choice
        // as `init_protection` — and let the `MAX_CPUS` assert catch a
        // configuration that cannot even hold slot 0.
        let bsp_hw = elected_bsp_hw_id(&topo);
        let bsp_logical = (0..topo.nr_cpus as usize)
            .find(|&i| topo.cpus[i].hw_id == bsp_hw)
            .unwrap_or(0) as u32;
        assert!(
            (bsp_logical as usize) < smp::MAX_CPUS,
            "init_smp_state: BSP logical id {} >= MAX_CPUS {}",
            bsp_logical,
            smp::MAX_CPUS
        );
        let mut state = smp::SmpState::with_ncpus(
            topo.nr_cpus.max(1),
            crate::proc::CpuId::new_unchecked(bsp_logical),
        );
        // Publish BSP_LOGICAL and the two masks now; smp_init's later seeding
        // is then idempotent.
        state.seed_bsp_masks(bsp_logical);
        *SMP_STATE.get() = Some(state);
    }
    // The process table is zeroed at creation, so every boot process starts
    // with `p_cpu = 0`. On x86 that is the BSP; on riscv64 the elected BSP can
    // be another hart, and leaving the boot set on CPU 0 puts the first
    // runnable process on an idle AP whose wakeup IPI this firmware cannot
    // deliver (`-smp 4` hung right after "entering scheduler"). Re-home the
    // whole boot set onto the elected BSP before the scheduler starts.
    //
    // A runnable process is already on CPU 0's run queue (proc_init enqueued
    // it). Merely rewriting `p_cpu` would leave the queue entry behind and
    // strand the process, so mirror C's affinity-change order in
    // `sched_proc`: RTS_SET(NO_QUANTUM) dequeues, the CPU store moves the
    // home, RTS_UNSET re-enqueues on the new CPU.
    use crate::proc::RtsFlagsBits;
    let smp_state = unsafe { crate::smp_state_boot_unchecked() };
    let bsp_logical = smp_state.bsp_cpu_id().raw();
    // SAFETY: single-threaded boot; the scheduler has not started.
    unsafe {
        let table = crate::proc_table_boot_unchecked();
        for raw in -(crate::proc_table::NR_TASKS as i32)..(crate::proc_table::NR_PROCS as i32) {
            let nr = crate::proc::ProcNr::new(raw);
            let Some(homed) = table
                .get(nr)
                .map(|p| p.p_sched.cpu.load(core::sync::atomic::Ordering::Acquire))
            else {
                continue;
            };
            if homed == bsp_logical {
                continue;
            }
            let runnable = table.get(nr).is_some_and(|p| p.is_runnable());
            if runnable {
                table.rts_set(nr, RtsFlagsBits::NO_QUANTUM);
            }
            if let Some(p) = table.get_mut(nr) {
                p.p_sched
                    .cpu
                    .store(bsp_logical, core::sync::atomic::Ordering::Release);
            }
            if runnable {
                table.rts_unset(nr, RtsFlagsBits::NO_QUANTUM);
            }
        }
    }
    // The BSP's ptproc slot = VM (C: arch_post_init
    // `get_cpulocal_var(ptproc) = vm` — protect.c:372; each AP installs its
    // own in smp_ap_tail). The SMP state must exist before the slot can be
    // written — hence this install lives in Phase D, not Phase C.
    if let Some(local) = smp_state.cpu_local_mut(smp_state.bsp_cpu_id()) {
        local.ptproc = Some(crate::proc::proc_nr::VM_PROC_NR);
    }
}

/// Minimal kmain for QEMU integration tests (feature = "qemu_test").
///
/// This is a naked function that captures SP/PC/FP at the exact entry point
/// (before any function prologue modifies them), then calls the real test
/// verification logic.
///
/// The HigherHalf trait's `jump_to_kmain` lands here. We verify that:
/// - Stack pointer is at a high virtual address (kern_stack_top was used)
/// - Stack pointer is 16-byte aligned (ABI requirement at function entry)
/// - Frame pointer is zero (set by HigherHalf transition)
///
/// # Architecture dispatch
///
/// `kmain` is `#[cfg]`-switched across three `naked_asm!` blocks for
/// x86_64 / aarch64 / riscv64. Each block captures SP/PC/FP at entry
/// using arch-specific register names, then calls `kmain_verify`.
///
/// `#[cfg(target_arch)]` here selects **asm register names**, not
/// behavior — `naked_asm!` requires literal register names at compile
/// time, so this is the idiomatic Rust pattern for multi-arch naked
/// functions (cf. `core::arch::asm!` docs). This does NOT violate the
/// "no `#[cfg(target_arch)]` behavior selection" rule.
///
/// # Safety (naked function)
///
/// This function has no prologue — the HigherHalf transition jumps directly
/// here with SP/PC/FP set by the arch-specific trampoline. The inline asm
/// captures these register values and passes them to `kmain_verify`. Safe
/// because: (1) the trampoline sets up a valid stack, (2) the asm only reads
/// registers and calls a safe Rust function, (3) no local variables exist
/// before the asm block (naked guarantee).
#[cfg(feature = "qemu_test")]
#[unsafe(naked)]
pub extern "C" fn kmain(kernel_info: &KernelInfo) -> ! {
    // The HigherHalf impl puts kinfo in RDI (System V convention).
    // On UEFI target (Windows x64 ABI), kmain_verify expects:
    //   RCX = kernel_info, RDX = sp, R8 = pc, R9 = fp
    // We rearrange registers accordingly.
    #[cfg(target_arch = "x86_64")]
    core::arch::naked_asm!(
        "mov r9, rbp",         // capture FP at entry → R9 (4th arg)
        "lea r8, [rip]",       // capture PC at entry → R8 (3rd arg)
        "mov rdx, rsp",        // capture SP at entry → RDX (2nd arg)
        "mov rcx, rdi",        // kinfo from RDI → RCX (1st arg, Windows ABI)
        "sub rsp, 40",         // shadow space (32B) + alignment (8B)
        "call {verify}",
        verify = sym kmain_verify,
    );
    #[cfg(target_arch = "aarch64")]
    core::arch::naked_asm!(
        "mov x1, sp",          // capture SP at entry
        "adr x2, .",           // capture PC at entry
        "mov x3, x29",         // capture FP at entry
        "bl {verify}",
        verify = sym kmain_verify,
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::naked_asm!(
        "mv a1, sp",           // capture SP at entry
        "auipc a2, 0",         // capture PC at entry
        "mv a3, s0",           // capture FP at entry
        "call {verify}",
        verify = sym kmain_verify,
    );
}

/// Verification function called from the naked kmain entry point.
///
/// Receives captured register values as arguments:
/// - kernel_info: pointer to KernelInfo
/// - sp: stack pointer at kmain entry
/// - pc: program counter at kmain entry
/// - fp: frame pointer at kmain entry
#[cfg(feature = "qemu_test")]
fn kmain_verify(kernel_info: &KernelInfo, sp: u64, pc: u64, fp: u64) -> ! {
    use crate::kmess::{EarlyConsole, KernelConsole as Console};

    // Ensure early console is initialized for test kernel output.
    Console::init();

    // B-X: per-arch banner strings come from minix-platform::test_support
    // (arch-dispatched re-export) — the kernel carries no
    // `#[cfg(target_arch)]` for output selection.
    Console::write_str(minix_platform::test_support::REACHED_BANNER);

    let kern_high = kernel_info.kern_virt_base.0;

    // B-X: banner labels are arch-dispatched from
    // minix_platform::test_support (ARCH_NAME/SP_LABEL/PC_LABEL/FP_LABEL).

    // ── Print captured register values ──
    Console::write_str("### test-higher-half ###\n");
    Console::write_str("  arch: ");
    Console::write_str(minix_platform::test_support::ARCH_NAME);
    Console::write_str("\n");
    Console::write_str(minix_platform::test_support::SP_LABEL);
    Console::write_hex(sp);
    Console::write_str("\n");
    Console::write_str(minix_platform::test_support::PC_LABEL);
    Console::write_hex(pc);
    Console::write_str("\n");
    Console::write_str(minix_platform::test_support::FP_LABEL);
    Console::write_hex(fp);
    Console::write_str("\n");
    Console::write_str("  kern_virt_base: ");
    Console::write_hex(kern_high);
    Console::write_str("\n");

    // ── Assertions ──
    // SP must be at a high virtual address (kern_stack_top was used)
    let sp_ok = sp >= kern_high;
    // SP entry alignment is ABI-specific:
    // - x86_64 System V: (RSP + 8) % 16 == 0, i.e. RSP ≡ 8 (mod 16)
    // - aarch64 AAPCS64 / riscv64 psABI: SP 16-byte aligned, i.e. 0 (mod 16)
    #[cfg(target_arch = "x86_64")]
    let sp_aligned = sp & 0xF == 8;
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    let sp_aligned = sp & 0xF == 0;
    // Frame pointer must be zero (set by HigherHalf transition)
    let fp_zero = fp == 0;

    // Note: PC check is architecture-dependent. In the real kernel, kmain
    // is linked at a high virtual address, so PC >= kern_high. In QEMU
    // test kernels, x86_64/aarch64 UEFI loads the test binary at a low
    // address, so PC may not be in the high range; those two keep PC as
    // informational. The riscv64 carrier is different: its direct leg adds
    // `delta` before `jalr`, so entering kmain at a low PC means the delta
    // path is broken — PC is a required condition there.
    let pc_at_high = pc >= kern_high;

    #[cfg(target_arch = "riscv64")]
    let pc_ok = pc_at_high;
    #[cfg(not(target_arch = "riscv64"))]
    let pc_ok = true;

    if sp_ok && sp_aligned && fp_zero && pc_ok {
        Console::write_str("### TEST_RESULT: PASS test-higher-half ###\n");
    } else {
        if !sp_ok {
            Console::write_str("  FAIL: SP not at high address\n");
        }
        #[cfg(target_arch = "riscv64")]
        if !pc_at_high {
            Console::write_str("  FAIL: PC not at high address (riscv64 delta path broken)\n");
        }
        #[cfg(not(target_arch = "riscv64"))]
        if !pc_at_high {
            Console::write_str("  INFO: PC not at high address (expected in test kernel)\n");
        }
        if !sp_aligned {
            Console::write_str("  FAIL: SP not at the architecture's entry alignment\n");
        }
        if !fp_zero {
            Console::write_str("  FAIL: FP not zero\n");
        }
        Console::write_str("### TEST_RESULT: FAIL test-higher-half ###\n");
    }

    loop {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: `hlt` halts the CPU until the next interrupt. nomem/nostack
        // guarantee no memory access or stack modification. Used in an infinite
        // loop after test completion — safe because no shared state is modified.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
        #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
        // SAFETY: `wfi` (Wait For Interrupt) halts the CPU until an interrupt
        // arrives. nomem/nostack guarantee no memory access or stack modification.
        // Used in an infinite loop after test completion — safe because no
        // shared state is modified.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}

/// Initialize protection structures: GDT/TSS (x86-64), VBAR_EL1 (aarch64), stvec (riscv64).
///
/// This must be the very first thing called in kmain, because without valid
/// GDT/TSS (x86-64) or VBAR_EL1/stvec (aarch64/riscv64), any exception will
/// cause an unrecoverable triple fault.
///
/// The trap table is initialized, populated with the real stub addresses
/// (`install_trap_stubs`), configured for SYSCALL, and loaded here; there is
/// no later recreate-and-discard phase.
///
/// C: prot_init() — protect.c:321 (x86) / protect.c:77 (ARM)
#[cfg(not(feature = "mock"))]
/// Protection + trap-entry bring-up — C prot_init/idt_init parity (kmain
/// Phase B). `pub`: the production phase is reused verbatim by the L4
/// bring-up test driver (`test-smp-aps`) — the AP tail's init_ap consumes
/// the PROTECTION global this function fills.
pub fn init_protection(kernel_info: &KernelInfo) {
    use minix_arch::{CurrentProtection, CurrentTrapEntry, ProtectionArch, TrapEntryArch};

    // The BSP's logical id is the single source for `init`, `program_entry_base`
    // and the AP-skip loop in `smp_init`. The firmware's `bsp_id` is only a
    // hint; the entry election records the CPU actually running this code.
    // Compute it before `init` so the TSS slot loaded via `ltr` and the GS
    // area written below name the same CPU even when the BSP is not slot 0.
    let topo = minix_platform::platform_desc().cpu_topology();
    let bsp_hw = elected_bsp_hw_id(&topo);
    let bsp_logical = (0..topo.nr_cpus as usize)
        .find(|&i| topo.cpus[i].hw_id == bsp_hw)
        .unwrap_or(0) as u32;

    // Step 1: Initialize protection structures.
    // x86-64: GDT + TSS; aarch64: SP_EL1; riscv64: sscratch
    // C: tss_init(0, &k_boot_stktop) — protect.c:338
    // Lifetime fix: the instance moves into the PROTECTION
    // global BEFORE load() — the trait's load() contract requires the tables
    // to stay put for as long as they are loaded ("kept in a static/global
    // location"), and a stack local dropped here would leave lgdt/lidt
    // pointing at reused stack memory. The global also gives the AP tail
    // (smp_init's registered continuation) access to init_ap without a
    // second instance.
    let prot = CurrentProtection::init(bsp_logical, kernel_info.kern_stack_top());
    store_protection(prot);
    // E1: the instance's address changed when it moved into the global —
    // the TSS descriptors embedded in the GDT still encode the pre-move
    // stack-local bases. Rebuild them from the final addresses before
    // ltr; otherwise the first CPL3→CPL0 transition (user-mode entry)
    // reads a dead TSS (sp0 = 0 → page fault at VA -8 on the frame push).
    // x86-only: the TSS descriptor rebuild + lgdt are GDT/TSS semantics
    // (aarch64/riscv64 keep register-state protection).
    #[cfg(target_arch = "x86_64")]
    with_protection_mut(|prot| prot.refresh_descriptors());
    with_protection(|prot| prot.load());

    // Program the CPU's entry base. x86-64 writes the GS area; the
    // aarch64/riscv64 default is a no-op (init/set_kernel_stack already
    // wrote TPIDR_EL1/sscratch). `bsp_logical` was resolved before `init`
    // so this write names the same CPU the TSS/GDT path did.
    with_protection(|prot| prot.program_entry_base(bsp_logical, kernel_info.kern_stack_top()));

    // Step 2: Prepare the trap entry table metadata.
    // x86-64: IDT metadata + SYSCALL MSR; aarch64: VBAR_EL1 metadata;
    // riscv64: stvec metadata.
    // C: idt_init() sets gate metadata with real handler addresses — protect.c:245-268;
    //     Rust populates the gates with the real stub addresses via
    //     install_trap_stubs and loads the table below.
    // C: SYSCALL MSR setup — protect.c:189-205
    let mut trap = CurrentTrapEntry::init();

    // The entry stubs exist — install their addresses into
    // every gate (C: idt_init() fills real entry addresses, protect.c:245-268),
    // point LSTAR at the kernel's own SYSCALL asm entry (C parity: tss_init
    // uses the asm label, protect.c:189-205 — KernelInfo.syscall_entry is
    // reference-only boot metadata), register the kernel-side dispatch bodies
    // with the arch entry gate, and only then make the table live
    // (C: idt_reload(), protect.c:268). Stage invariant from here on: no
    // empty-vector / empty-handler gate, kernel runs IF=0 except at
    // explicitly controlled sti points (all gates are interrupt gates).
    use minix_arch::{install_trap_stubs, syscall_entry_va};
    install_trap_stubs(&mut trap);
    trap.configure_syscall(syscall_entry_va());
    // Register the arch-shaped production dispatch bodies. The cfg
    // selection lives in trap_dispatch (the arch-dispatch module), not in
    // this OS-level function.
    trap_dispatch::register_current_arch();
    store_trap_entry(trap);
    with_trap_entry(|trap| trap.load());
}

/// The live protection instance (GDT/TSS image) — a resident static, per the
/// `ProtectionArch::load` lifetime contract. `None` before init_protection.
#[cfg(target_arch = "x86_64")]
pub(crate) static PROTECTION: SyncUnsafeCell<
    Option<minix_arch::x86_64::protection::X86_64Protection>,
> = SyncUnsafeCell::new(None);

/// The live trap-entry instance (IDT image) — same lifetime contract via
/// `TrapEntryArch::load`.
#[cfg(target_arch = "x86_64")]
pub(crate) static TRAP_ENTRY: SyncUnsafeCell<
    Option<minix_arch::x86_64::trap_entry::X86_64TrapEntry>,
> = SyncUnsafeCell::new(None);

/// Move `prot` into the global (before its first `load`).
#[cfg(target_arch = "x86_64")]
pub(crate) fn store_protection(prot: minix_arch::x86_64::protection::X86_64Protection) {
    // SAFETY: boot is single-threaded; no loads reference the global yet.
    unsafe { *PROTECTION.get() = Some(prot) }
}

/// Move `trap` into the global (before its first `load`).
#[cfg(target_arch = "x86_64")]
pub(crate) fn store_trap_entry(trap: minix_arch::x86_64::trap_entry::X86_64TrapEntry) {
    // SAFETY: boot is single-threaded; no loads reference the global yet.
    unsafe { *TRAP_ENTRY.get() = Some(trap) }
}

/// Run `f` with the live protection instance (BKL-serialized callers).
#[cfg(target_arch = "x86_64")]
pub(crate) fn with_protection<R>(
    f: impl FnOnce(&minix_arch::x86_64::protection::X86_64Protection) -> R,
) -> R {
    let p = unsafe { (*PROTECTION.get()).as_ref() }
        .expect("PROTECTION not initialized — init_protection must run first");
    f(p)
}

/// Mutable variant for boot-time fixes that must run against the live
/// tables (E1: the TSS descriptor rebuild after the instance moved into
/// the global). Single-threaded boot only. x86-only: the live-image
/// statics and the TSS fix-ups are x86 semantics (aarch64/riscv64 keep
/// register-state protection — nothing mutable is stored).
#[cfg(target_arch = "x86_64")]
pub(crate) fn with_protection_mut<R>(
    f: impl FnOnce(&mut minix_arch::x86_64::protection::X86_64Protection) -> R,
) -> R {
    let p = unsafe { (*PROTECTION.get()).as_mut() }
        .expect("PROTECTION not initialized — init_protection must run first");
    f(p)
}

/// Run `f` with the live trap-entry instance (BKL-serialized callers).
#[cfg(target_arch = "x86_64")]
pub(crate) fn with_trap_entry<R>(
    f: impl FnOnce(&mut minix_arch::x86_64::trap_entry::X86_64TrapEntry) -> R,
) -> R {
    let t = unsafe { (*TRAP_ENTRY.get()).as_mut() }
        .expect("TRAP_ENTRY not initialized — init_protection must run first");
    f(t)
}

#[cfg(not(target_arch = "x86_64"))]
use minix_arch::{CurrentProtection, CurrentTrapEntry};

#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn store_protection(prot: CurrentProtection) {
    let _ = prot; // aarch64/riscv64: register-state protection, nothing to keep
}
#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn store_trap_entry(trap: CurrentTrapEntry) {
    let _ = trap;
}
#[cfg(not(target_arch = "x86_64"))]
static PROTECTION: CurrentProtection = CurrentProtection::new();
#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn with_protection<R>(f: impl FnOnce(&CurrentProtection) -> R) -> R {
    f(&PROTECTION)
}
#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn with_trap_entry<R>(f: impl FnOnce(&mut CurrentTrapEntry) -> R) -> R {
    use minix_arch::arch::trap_entry::TrapEntryArch as _;
    f(&mut CurrentTrapEntry::init())
}

/// Initialize clock and interrupt controller.
///
/// C: init_clock() + intr_init(0) + arch_init() — main.c:403-481
/// Covered in detail in 05-clock-interrupt-init.md.
///
/// **Hardware timer programming is NOT done here**:
/// C `init_clock()` (clock.c:48-66) is pure software, and the hardware
/// timer is programmed + its handler registered + its gates opened — in
/// that order — at the C `boot_cpu_init_timer` position
/// (`bsp_finish_booting` Step 6, clock.c:294). An earlier revision called
/// `ClockArch::init_timer` here, which on aarch64/riscv64 also opened the
/// timer gate (`CNTP_CTL_EL0.Enable` / `sie.STIE`), leaving a live
/// interrupt source running through the rest of boot with no handler.
///
/// **Order still matters** (cf. 05-clock-interrupt-init.md §1.1):
/// 1. The controller is initialized with every line masked
///    (`init` calls `mask_all` internally) — C `intr_init` parity.
/// 2. `arch_init` runs after the controller is live; it no
///    longer enables any interrupt (the riscv64 `sie = STIE | SSIE` write
///    moved to its gate owners: the timer gate to
///    `TimerIrqGate::enable_timer_irq` at Step 6, the IPI gate to the SMP
///    bring-up).
#[cfg(not(feature = "mock"))]
/// Initialize clock software state, the interrupt controller (every line
/// masked), and arch-init. C: init_clock() + intr_init(0) + arch_init().
///
/// `pub`: a production boot phase (kmain Phase B)
/// that the L3 bring-up test driver (`test-timer-irq`) calls in the same
/// kmain order — the test must exercise the production sequence, not a
/// re-implementation of it.
pub fn init_clock_and_interrupts() {
    use crate::clock::ClockState;
    use minix_arch::{ArchInit, CurrentArchInit};
    use minix_plat::{CurrentInterruptController, InterruptRouter};
    use minix_platform::{PlatformDesc, platform_desc};

    // Obtain the platform descriptor (initialized earlier from KernelInfo).
    // This is the single source of truth for all hardware parameters.
    let pd = platform_desc();

    // Step 1: Initialize clock state (software).
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    // No env_get("hz") needed — DEFAULT_HZ is compile-time constant.

    // Step 2: Initialize interrupt controller.
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:24
    //
    // Instance-based design: construct the interrupt controller from the
    // interrupt controller descriptor, then call `init` on the instance.
    // The instance is then moved into the global `IRQ_MANAGER` so that
    // trap entry points and `bsp_finish_booting` can reach it.
    let mut intr = CurrentInterruptController::new(pd.interrupt_controller());
    // PD-22 step 2 check-and-reject + device-window VA injection (x86_64):
    // BSP's IA32_APIC_BASE must equal the window's LAPIC page (per-CPU
    // checks re-run on every AP at its own first LAPIC MMIO — clock /
    // EOI paths); after validation the LAPIC driver's MMIO goes through
    // the high-half device window.
    #[cfg(target_arch = "x86_64")]
    {
        let regions = minix_arch::arch::device_window::device_window_regions();
        let lapic_region = &regions[1];
        assert!(
            lapic_region.phys_base == 0xFEE0_0000,
            "device-window LAPIC page drifted from 0xFEE00000"
        );
        // The descriptor is the contract: the control page the firmware
        // reported (MADT LAPIC address / IOAPIC record) must be the page the
        // device window maps. Refuse a relocated controller instead of
        // silently programming the wrong physical address.
        assert_eq!(
            intr.lapic_base(),
            lapic_region.phys_base as usize,
            "descriptor LAPIC base {:#x} != device-window page {:#x} \
             (relocated LAPIC unsupported)",
            intr.lapic_base(),
            lapic_region.phys_base
        );
        assert_eq!(
            intr.ioapic_base(),
            regions[0].phys_base as usize,
            "descriptor IOAPIC base {:#x} != device-window page {:#x} \
             (relocated IOAPIC unsupported)",
            intr.ioapic_base(),
            regions[0].phys_base
        );
        let lapic_va = minix_arch::arch::device_window::device_va_for(regions, 1, 0xFEE0_0000)
            .expect("LAPIC page inside its own window") as usize;
        minix_plat::x86_64_interrupt_set_lapic_va(lapic_va);
        // The IO-APIC page is slot 0; runtime re-mask/unmask goes through
        // the same window (process roots carry no identity after PD22S2E).
        let ioapic_va =
            minix_arch::arch::device_window::device_va_for(regions, 0, regions[0].phys_base)
                .expect("IO-APIC page inside its own window") as usize;
        minix_plat::x86_64_interrupt_set_ioapic_va(ioapic_va);
        minix_arch::arch::device_window::set_x86_window_live();
    }
    // PD-22 step 2 check-and-reject + device-window VA injection (riscv64):
    // the DTB-discovered PLIC base must equal the base the kernel device
    // window maps — a mismatch means the window and the driver would talk
    // to different devices. After validation the driver's MMIO goes through
    // the high-half device window, never PA-as-VA.
    #[cfg(target_arch = "riscv64")]
    {
        let regions = minix_arch::arch::device_window::device_window_regions();
        let plic_region = &regions[0];
        assert!(
            intr.plic_base() == plic_region.phys_base as usize,
            "PLIC base {:#x} != device-window base {:#x} — DTB/window drift, \
             refusing (PD-22 check-and-reject)",
            intr.plic_base(),
            plic_region.phys_base
        );
        let plic_va =
            minix_arch::arch::device_window::device_va_for(regions, 0, plic_region.phys_base)
                .expect("PLIC base inside its own window") as usize;
        intr.set_plic_va_base(plic_va);
        crate::irq_manager::set_injected_plic_va(plic_va);
        // PD22S2H diag: confirm the injected base round-trips.
        minix_plat::riscv64_early_console_write_str("  [inject-diag] plic_va set\n");
    }
    intr.init(); // mask_all() called internally

    // D10: Store the interrupt controller in the global `IRQ_MANAGER`.
    // This replaces the previous pattern of dropping `intr` after init.
    // SAFETY: Boot is single-threaded before BKL exists; no concurrent access.
    unsafe {
        *IRQ_MANAGER.get() = Some(crate::irq_manager::IrqManager::new(intr));
    }

    // aarch64: `init()` masks every SGI in the BSP's redistributor. Open the
    // scheduler SGI gate on the BSP now; an AP opens its own gate in its
    // bring-up path (when that path enters the kernel scheduler — see
    // C2D-A64SGI on the C board). The clamp keeps all processes on the BSP,
    // so this is the only CPU that can currently receive one.
    #[cfg(target_arch = "aarch64")]
    {
        // Use the election truth, not the firmware's first-record `bsp_id`:
        // the same BSP identity drives init_smp_state/init_protection, and
        // the SGI set-enable bit must land on the CPU actually running the
        // kernel (§4.9). `elected_bsp_hw_id` falls back to `bsp_id`.
        let topo = pd.cpu_topology();
        crate::irq_manager::enable_sched_sgi_for_cpu(elected_bsp_hw_id(&topo) as usize);
    }

    // Step 3: Architecture-specific initialization.
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    //
    // Instance-based design: construct the arch-init from the arch-misc
    // descriptor, then call `init` on the instance.
    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
    arch_init.init();

    // Store the software clock into the
    // global — the timer IRQ handler (`clock::clock_irq_handler`) reads it
    // at dispatch time to advance uptime and expire alarm timers.
    // SAFETY: boot is single-threaded before BKL exists.
    unsafe {
        *CLOCK_STATE.get() = Some(clock);
    }
}

/// Initialize process table and boot processes.
///
/// This is the Rust equivalent of C's `proc_init()` + the boot image loop
/// in `main.c:157-282`. It:
/// 1. Creates the process table (all slots SLOT_FREE, p_nr/p_endpoint set)
/// 2. Creates the privilege table (all slots free)
/// 3. Iterates over boot modules, assigning privileges and initializing
///    each boot process
/// 4. Loads VM ELF into the bootstrap page table (architecture-specific)
///
/// C: proc_init() — proc.c:119
/// C: boot image loop — main.c:157-282
/// C: arch_boot_proc() — protect.c:388 (x86) / protect.c:115 (ARM)
///
/// Occupied ranges the VM bootstrap allocator must never hand out.
///
/// The boot-shim memmap reports all conventional RAM as free, so every range
/// still live at the first VM allocation has to be carved out here. Two of
/// them are not part of the kernel image: the handoff root page (`arch_boot`
/// allocated it separately) and the boot bump region (bootstrap page tables
/// plus early allocations). On the UEFI legs both are `LOADER_DATA` and never
/// appear in the memmap, so these cuts are no-ops there; on the self-boot legs
/// the bump is carved *from* the memmap, which is exactly the case this list
/// protects. The publish step (`build_vm_handoff`) runs later and only reads —
/// it cannot protect the first allocation.
const VM_BOOT_EXCLUSION_CAP: usize = crate::proc::NR_BOOT_MODULES + 3;

fn vm_boot_exclusions(
    kern_base: PhysBytes,
    kern_len: usize,
    modules: &[minix_boot::BootModule],
    root: Option<PhysBytes>,
    bump: Option<(u64, u64)>,
) -> ([MemoryRegion; VM_BOOT_EXCLUSION_CAP], usize) {
    let mut exclusions = [MemoryRegion {
        base: PhysBytes(0),
        len: 0,
    }; VM_BOOT_EXCLUSION_CAP];
    let mut n = 0usize;
    // The membership lives in one place (`for_each_live_bootstrap`); this
    // pre-allocation view has no bootstrap-allocator suffixes yet, so it
    // passes an empty list.
    crate::vm_handoff::for_each_live_bootstrap(
        kern_base,
        kern_len as u64,
        modules,
        root,
        bump,
        &[],
        |base, len| {
            push_exclusion(
                &mut exclusions,
                &mut n,
                PhysBytes(base),
                usize::try_from(len).expect("boot exclusion length exceeds usize"),
            );
        },
    );
    (exclusions, n)
}

/// Collects one occupied physical range into a fixed-size exclusion
/// array (boot is zero-heap — no `Vec`). The boot-shim memmap reports
/// all conventional RAM as free *including* kernel-image and boot-module
/// regions; `VmBootRegion::select_multi` needs them excluded so
/// `VmBootRegion ∩ ReservedRegions = ∅` holds (see `frame.rs` module
/// doc-comment, "Invariants enforced by construction").
/// Callers pre-allocate [`VM_BOOT_EXCLUSION_CAP`] entries.
fn push_exclusion(exclusions: &mut [MemoryRegion], n: &mut usize, base: PhysBytes, len: usize) {
    if len == 0 {
        return;
    }
    assert!(
        *n < exclusions.len(),
        "push_exclusion: exclusion list overflow (NR_BOOT_MODULES too small)"
    );
    exclusions[*n] = MemoryRegion { base, len };
    *n += 1;
}

#[cfg(not(feature = "mock"))]
pub fn init_proc_and_boot(kernel_info: &KernelInfo) {
    use crate::proc::NR_BOOT_MODULES;
    use crate::proc::{
        BOOT_MODULE_PROC_NRS, KERNEL_TASKS, ProcName, ProcNr, RtsFlagsBits, proc_nr,
    };
    use crate::proc_table::NR_TASKS;
    use minix_arch::{
        CpuContextArch, CurrentCpuContextArch, EntrySpec, ProcKind, VmLoadError, load_vm_elf,
    };

    // Step 1+2: Acquire global process + privilege tables (SyncUnsafeCell, BSS).
    // C: `EXTERN struct proc proc[]` / `EXTERN struct priv priv[]` — glo.h.
    // The tables are `const fn`-initialized at compile time (all slots
    // SLOT_FREE / s_proc_nr=None); per-process setup happens below.
    // SAFETY: boot is single-threaded before BKL exists.
    let proc_table = unsafe { crate::proc_table_boot_unchecked() };
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };

    // C: NR_BOOT_MODULES check — main.c:160-162
    // Use getter method (preferred API).
    assert_eq!(
        kernel_info.boot_modules.len(),
        NR_BOOT_MODULES,
        "expected {} boot modules, found {}",
        NR_BOOT_MODULES,
        kernel_info.boot_modules.len()
    );

    // Step 3a: Initialize kernel tasks (hardcoded, not from multiboot modules).
    // C: image[0..NR_TASKS] in table.c — ASYNCM, IDLE, CLOCK, SYSTEM, KERNEL
    // Kernel tasks are compiled into the kernel, not loaded from GRUB modules.
    for &(name, nr) in KERNEL_TASKS.iter() {
        let Some(proc) = proc_table.get_mut(nr) else {
            continue;
        };

        proc.set_boot_name(name);

        // Kernel tasks are always schedulable.
        // C: schedulable_proc = iskerneln(proc_nr) — main.c:173
        // C: priv(rp)->s_flags = (nr==IDLE ? IDL_F : TSK_F) — main.c:188-189
        // C: TSK_M = NO_M (no IPC), TSK_KC = NO_C (no kernel calls).
        // 06-proc-init-boot-proc.md §3.2 — grant_capability replaces assign_static
        // + configure_boot_priv pair; template encodes flags + masks by construction.
        let template = if nr == proc_nr::IDLE {
            crate::capability::CapabilityTemplate::Idle
        } else {
            crate::capability::CapabilityTemplate::KernelTask
        };
        let priv_id = priv_table
            .grant_capability(nr, template)
            .expect("grant_capability: kernel task priv slot occupied");
        // E1 slice 5: link the priv slot back into the process — C's
        // `get_priv(rp, id)` sets rp->p_priv (proc.h); without this the
        // process's priv_id stays None and every kernel call is denied
        // ECALLDENIED (observed live in test-user-trap).
        proc.priv_id = Some(priv_id);

        // Architecture-private CPU context. The arch layer decides the
        // initial PSW/PSR/sstatus, segment selectors, FPU policy, and
        // per-process FPU enable (aarch64) — the kernel layer never
        // sees these values.
        let cpu_context = <CurrentCpuContextArch as CpuContextArch>::build_cpu_context(
            ProcKind::KernelTask,
            nr,
            EntrySpec::KERNEL_TASK,
        );
        proc.set_boot_cpu_context(cpu_context);

        // Kernel tasks start stopped.
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    // Step 3b: Initialize user-space boot modules (from multiboot module list).
    // C: image[NR_TASKS..NR_BOOT_PROCS] in table.c
    // C: kinfo.module_list[i] corresponds to image[NR_TASKS + i]
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        // Map boot module index to process number using the C boot image order.
        // C: image[NR_TASKS + i].proc_nr — table.c
        let nr: ProcNr = BOOT_MODULE_PROC_NRS[i];

        let Some(proc) = proc_table.get_mut(nr) else {
            continue;
        };

        // Set process name.
        // C: strlcpy(rp->p_name, ip->proc_name, sizeof(rp->p_name))
        proc.set_boot_name(module.name);

        // Determine if this process is immediately schedulable.
        // C: schedulable_proc = (iskerneln(proc_nr) || isrootsysn(proc_nr) ||
        //                         proc_nr == VM_PROC_NR)
        // C: main.c:173-174
        let is_root_sys = nr == proc_nr::RS_PROC_NR;
        let is_vm = nr == proc_nr::VM_PROC_NR;
        let schedulable = is_root_sys || is_vm;

        if schedulable {
            // Assign static privilege + boot flags via capability template.
            // C: get_priv(rp, static_priv_id(proc_nr)) — main.c:200
            // C: priv(rp)->s_flags = VM_F (VM) or RSYS_F (RS) — main.c:179-209
            // C: ipc_to_m = SRV_M = ALL_M; kcalls = SRV_KC = ALL_C — main.c:184-213
            // 06-proc-init-boot-proc.md §3.2 — single template call encodes all of these.
            let template = if is_vm {
                crate::capability::CapabilityTemplate::Vm
            } else {
                crate::capability::CapabilityTemplate::RootService
            };
            let priv_id = priv_table
                .grant_capability(nr, template)
                .expect("grant_capability: static priv slot occupied");
            // E1 slice 5: same priv-link as the kernel-task loop — without
            // it the schedulable services fail every kernel call with
            // ECALLDENIED (C: get_priv(rp, id) → rp->p_priv).
            proc.priv_id = Some(priv_id);
        } else {
            // Don't let the process run for now.
            // C: RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM) — main.c:253
            proc.p_rts_flags
                .set(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM);
        }

        // For user-space boot processes, set up the arch-private CPU
        // context. The C code's `arch_boot_proc(ip, rp)` is split
        // here into: (1) ELF loading for VM (`load_vm_elf`), and (2)
        // CPU-context construction (`build_cpu_context`). Both are
        // arch-layer responsibilities — the kernel never inspects
        // the fields.
        //
        // Process kind mapping (06-proc-init-boot-proc.md §3.4):
        //   VM_PROC_NR    → ProcKind::Vm
        //   RS_PROC_NR    → ProcKind::RootService
        //   other user    → ProcKind::UserService (RS will load ELF later)
        let proc_kind = if is_vm {
            ProcKind::Vm
        } else if is_root_sys {
            ProcKind::RootService
        } else {
            ProcKind::UserService
        };

        // VM ELF loading (§12.2: now returns Result, no silent failure).
        let entry = if is_vm {
            #[cfg(feature = "mock")]
            {
                use minix_arch::DirectMapArch as _;
                use minix_arch::MockDirectMap;
                use minix_arch::arch::frame::{VmBootAllocator, VmBootRegion, VmBootRegions};
                use minix_arch::paging::mock::MockPaging;

                // VM image frames come from the VM Bootstrap Memory Handoff
                // (frame.rs module doc-comment). PA is decoupled from the
                // ELF VA. The boot-shim memmap reports all conventional RAM
                // as free — including the kernel image and boot-module
                // regions — so the occupied ranges must be excluded here
                // (invariant 1: `VmBootRegion ∩ ReservedRegions = ∅`).
                // Collect occupied ranges: kernel image + every boot module
                // + the handoff root page + the boot bump region (bootstrap
                // page tables live there) — all four are live at the first
                // VM allocation, which runs before any deduction is published.
                // (fixed array — boot is zero-heap, no Vec).
                let (exclusions, n_excl) = vm_boot_exclusions(
                    kernel_info.kern_phys_base,
                    kernel_info.kern_size as usize,
                    kernel_info.boot_modules,
                    current_root_phys(),
                    crate::boot_alloc::boot_alloc_region(),
                );
                let regions = VmBootRegion::select_multi(kernel_info.memmap, &exclusions[..n_excl])
                    .expect("init_proc_and_boot: VM bootstrap region selection failed")
                    .unwrap_or_else(|| {
                        panic!(
                            "init_proc_and_boot: no free memory for VM bootstrap region \
                             (after excluding kernel, boot modules, root page and bump)"
                        )
                    });
                let mut vm_alloc = VmBootAllocator::new(regions);
                let mut paging = MockPaging::new_from_page(PhysBytes(0), minix_arch::paging::PteChannel::Identity);
                let access = MockDirectMap;
                let vm_result =
                    load_vm_elf(module, kernel_info, &mut paging, &mut vm_alloc, &access)
                        .unwrap_or_else(|e| {
                            panic!("load_vm_elf: VM ELF is required at boot: {e:?}")
                        });
                // C: protect.c:450-451 clears the module's mod_start/
                // mod_end to hand its pages back to the boot allocator. The
                // Rust port keeps every module blob reserved instead: the
                // A2 deduction record covers it, and the kernel's boot-exec
                // reads each blob while loading the servers, so reclaiming
                // it would let the VM bootstrap allocator overwrite a blob
                // that is still in use (gh53/58).
                EntrySpec::loaded(vm_result.pc, vm_result.sp, vm_result.ps_strings)
            }
            #[cfg(not(feature = "mock"))]
            {
                // Real VM ELF loading at boot.
                //
                // This branch loads the VM ELF at boot. VM must be runnable
                // immediately after boot so it can handle VMCTL/PRIVCTL
                // syscalls from other boot processes; a VM with PC=0 cannot
                // serve any syscall.
                //
                // The bootstrap page table (created by `arch_boot_impl`)
                // is still active — we wrap it via `from_active_root` and
                // map the VM ELF segments into it. Direction D
                // (see `frame.rs` VM Bootstrap Memory Handoff):
                // segment frames come from a verified
                // `VmBootRegions`; `load_vm_elf` maps VM VA → the
                // allocated PA (VA ≠ PA — no identity assumption).
                //
                // After the ELF is loaded, the VM module's physical memory
                // is reclaimed via `add_memmap` (undoes the `cut_memmap`
                // from Phase A.2), matching C: protect.c:450-451.
                use minix_arch::CurrentPaging;
                use minix_arch::arch::frame::{VmBootAllocator, VmBootRegion, VmBootRegions};
                use minix_arch::paging::Paging as _;

                // Same exclusion set as the mock path: boot-shim memmap is
                // unfiltered, so occupied ranges (kernel + modules) must be
                // excluded (invariant 1: `VmBootRegion ∩ ReservedRegions = ∅`).
                let (exclusions, n_excl) = vm_boot_exclusions(
                    kernel_info.kern_phys_base,
                    kernel_info.kern_size as usize,
                    kernel_info.boot_modules,
                    current_root_phys(),
                    crate::boot_alloc::boot_alloc_region(),
                );
                let regions = VmBootRegion::select_multi(kernel_info.memmap, &exclusions[..n_excl])
                    .expect("init_proc_and_boot: VM bootstrap region selection failed")
                    .unwrap_or_else(|| {
                        panic!(
                            "init_proc_and_boot: no free memory for VM bootstrap region \
                             (after excluding kernel, boot modules, root page and bump)"
                        )
                    });
                let mut vm_alloc = VmBootAllocator::new(regions);

                let root_phys = current_root_phys().expect(
                    "init_proc_and_boot: bootstrap root not set \
                             — arch_boot_impl must run first",
                );
                let mut paging =
                    CurrentPaging::from_active_root(root_phys, minix_arch::paging::PteChannel::KernelDm);
                // B-X (2026-09-09): the per-arch ZST selection moved to the
                // sanctioned `CurrentDirectMap` alias in minix-arch
                // (mirrors each `Arch` bundle's `type Dm`) — kernel code
                // carries no behavior-selection cfg.
                let access = minix_arch::CurrentDirectMap::default();
                let vm_result =
                    load_vm_elf(module, kernel_info, &mut paging, &mut vm_alloc, &access)
                        .unwrap_or_else(|e| {
                            panic!("load_vm_elf: VM ELF is required at boot: {e:?}")
                        });

                // C: protect.c:450-451 reclaims the module's pages; this
                // port keeps the whole blob reserved (see the mock branch
                // above and the A2 deduction record).
                // Vacate the boot-identity leftovers under
                // VM's runtime user VA layout. ELF segment VAs are handled
                // inside `load_elf_into` (per-page eviction). The remaining
                // window is VM's HeapArena [VM_HEAP_BASE, +VM_HEAP_SIZE): VM
                // maps heap pages there at runtime via vm_self_mappages →
                // Paging::map, which structurally fails with AlreadyMapped
                // while arch_boot_impl Step 1's supervisor identity huge
                // leaves stand (walk_alloc refuses to clobber huge leaves,
                // x86_64/paging.rs:383/399). Split + unmap at 4 KiB
                // granularity. Placed AFTER `load_vm_elf` returned, so a
                // module blob read through its identity mapping is never
                // evicted mid-copy; the arena's backing PAs have no other
                // kernel-side identity consumers.
                {
                    let heap_lo = <minix_arch::CurrentDirectMap as minix_arch::arch::direct_map::DirectMapArch>::VM_HEAP_BASE;
                    let heap_end = heap_lo
                        + <minix_arch::CurrentDirectMap as minix_arch::arch::direct_map::DirectMapArch>::VM_HEAP_SIZE;
                    let page = CurrentPaging::PAGE_SIZE as u64;
                    let mut hva = heap_lo;
                    while hva < heap_end {
                        let v = VirBytes(hva);
                        if paging.query(v).is_some() {
                            paging.split_huge(v).unwrap_or_else(|e| {
                                panic!(
                                    "heap-window eviction: split_huge failed ({e:?}) — \
                                 arm64/riscv64 must implement it before their first light"
                                )
                            });
                            // Open the intermediate levels for CPL3 too:
                            // VM's runtime `vm_self_mappages` → Paging::map
                            // installs user leaves here, and `walk_alloc`
                            // does not amend the supervisor identity
                            // intermediates it finds in place (#PF
                            // err=0x15 — same shape as the ELF segment
                            // eviction in `load_elf_into`).
                            paging.grant_user_walk(v).unwrap_or_else(|e| {
                                panic!("heap-window eviction: grant_user_walk failed ({e:?})")
                            });
                            paging.unmap(v).expect("heap-window eviction: unmap failed");
                        }
                        hva += page;
                    }
                }

                // After both the ELF eviction and the heap-window eviction
                // have run, query the VM entry page directly to distinguish
                // "the loader eviction missed" (flags lack USER or are still
                // HUGE) from "it was remapped after installation".
                {
                    use crate::kmess::{EarlyConsole as _, KernelConsole as DiagC};
                    let probe = vm_result.pc;
                    DiagC::write_str("kernel: diag pc=");
                    DiagC::write_hex(probe.0);
                    match paging.query(VirBytes(probe.0 & !0xFFF)) {
                        Some((pa, f)) => {
                            DiagC::write_str(" pa=");
                            DiagC::write_hex(pa.0);
                            DiagC::write_str(" flags=");
                            DiagC::write_hex(f.bits() as u64);
                        }
                        None => DiagC::write_str(" UNMAPPED"),
                    }
                    DiagC::write_str("\n");
                }

                // A1 address-space identity hand-off: publish VM's bootstrap
                // root to VM itself. The kernel writes a `VmBootHandoff`
                // page and maps it user read-only at `VM_BOOT_HANDOFF_VA`;
                // VM reads it at startup and adopts the root as its own
                // page table (`VmSelfPageTable::adopt`, see
                // 07-paging_init_design §4). The handoff frame is allocated
                // from the VM bootstrap allocator, so it lands in the
                // LiveBootstrap record that the A2 free-region
                // classification deducts.
                let handoff_frame = vm_alloc
                    .alloc_page()
                    .expect("init_proc_and_boot: no frame for VM boot handoff page");
                let hv = access.frame_virt(handoff_frame);
                // Map the handoff page user read-only: VM consumes it once
                // at startup and must not be able to rewrite it. The VA is
                // above the identity window and below every architecture's
                // user-VA limit (see minix_types::VM_BOOT_HANDOFF_VA).
                paging
                    .map(
                        VirBytes(VM_BOOT_HANDOFF_VA),
                        handoff_frame.start(),
                        PageFlags::read_only(),
                    )
                    .expect("init_proc_and_boot: failed to map VM boot handoff page");
                // Classification point (07-paging_init_design §6.0):
                // every bootstrap allocation — ELF segments, the PT
                // hierarchy, stacks, the handoff frame itself, and any
                // page-table page the `map` above just installed — is now
                // handed out, so the cut below sees the complete
                // LiveBootstrap(t_classify) record. `i` is VM's index in
                // the boot-module list: every module blob — VM's own
                // included — is kernel-reserved until VM starts, so it **is**
                // in the deduction record; the VM-exclusion rule applies only
                // to the handoff module list (see `classify`'s doc).
                let handoff =
                    crate::vm_handoff::build_vm_handoff(kernel_info, root_phys, &vm_alloc, i);
                // SAFETY: `hv` covers one whole frame (PhysAccess contract)
                // and `VmBootHandoff` fits well within a page.
                unsafe {
                    core::ptr::write(hv.0 as *mut VmBootHandoff, handoff);
                }

                // Record VM's page-table root addresses in p_seg so
                // `init_post_and_memory` (Phase D) can assert them valid and
                // install VM as the kernel-level ptproc
                // (`set_current_ptproc_nr`). The bootstrap root IS VM's
                // initial and permanent root: VM adopts it at startup
                // (A1 hand-off via the VmBootHandoff page above), so there
                // is no later "VM installs its own page table" step —
                // VMCTL SetAddrSpace only re-points *other* processes'
                // page tables built by VM.
                proc.p_seg.phys_root = root_phys;
                // The virtual address of the root is the identity-mapped
                // address (VA = PA during bootstrap).
                proc.p_seg.virt_root = Some(VirBytes(root_phys.0));

                EntrySpec::loaded(vm_result.pc, vm_result.sp, vm_result.ps_strings)
            }
        } else {
            // Other user processes have no ELF loaded at boot.
            // RS will load them at runtime.
            // Module physical memory stays cut (reserved) until RS loads
            // the ELF and reclaims it.
            EntrySpec::DEFERRED
        };

        let cpu_context =
            <CurrentCpuContextArch as CpuContextArch>::build_cpu_context(proc_kind, nr, entry);
        proc.set_boot_cpu_context(cpu_context);

        // VM inhibit: all user processes except VM must wait for VM to
        // create their page tables.
        // C: main.c:264-267
        if nr != proc_nr::VM_PROC_NR {
            proc.p_rts_flags
                .set(RtsFlagsBits::VMINHIBIT | RtsFlagsBits::BOOTINHIBIT);
        }

        // All boot processes start stopped.
        // C: rp->p_rts_flags |= RTS_PROC_STOP — main.c:272
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);

        // Mark slot as in use.
        // C: rp->p_rts_flags &= ~RTS_SLOT_FREE — main.c:273
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    // Step 4: Update boot procs info for VM.
    // C: memcpy(kinfo.boot_procs, image, sizeof(kinfo.boot_procs)) — main.c:282
    // In Rust, kernel_info is immutable and boot_modules already contains this info.
    //
    // The global `PROC_TABLE` / `PRIV_TABLE` statics now hold the initialized
    // tables; callers acquire them via `crate::proc_table()` / `crate::priv_table()`.
}

/// Initialize post-boot architecture state.
///
/// This is the Rust equivalent of C's `arch_post_init()` + `memory_init()`,
/// reduced under Direct Map to a **readiness confirmation**:
///
/// 1. Assert VM's page-table root is valid (established in Phase C).
/// 2. Confirm VM is the kernel-level ptproc (the `set_current_ptproc_nr`
///    write happens in `switch_address_space`, and the CpuLocal slot is
///    installed in `init_smp_state` — this step records nothing itself) so
///    `dispatch_vmctl(SetAddrSpace)` Step 3 can decide whether to reload
///    the hardware root register (C: `setcr3()`, arch_do_vmctl.c:19-33; the
///    `p == ptproc` check is at :25).
/// 3. Assert the VM Direct Map base is configured.
///
/// The C behavior this replaces — `arch_post_init`'s `ptproc = VM` + `pg_info`
/// and `memory_init`'s freepdes allocation — is intentionally absent:
/// Direct Map is a permanent mapping, so no temporary-window allocation or
/// borrowed page-directory registration is needed (ARCH: Direct Map, see
/// 07-cross-space-init.md §3.5).
///
/// Must be called after `init_proc_and_boot()` (Phase C), because VM's
/// process slot and page table must already be initialized.
///
/// C: arch_post_init() — protect.c:370 (x86) / protect.c:97 (ARM)
/// C: memory_init() — memory.c:707 (x86) / memory.c:612 (ARM)
#[cfg(not(feature = "mock"))]
pub fn init_post_and_memory(proc_table: &crate::proc_table::ProcessTable) {
    use minix_arch::{CurrentDirectMap, DirectMapArch};

    // Step 1: Assert VM's page-table root is valid.
    //
    // The root was installed in Phase C (`init_proc_and_boot`): for the
    // non-mock path, `p_seg.phys_root` records the bootstrap root and
    // `p_seg.virt_root` its identity-mapped VA (assigned earlier in
    // `init_proc_and_boot`). Both must
    // be present before VM can be trusted as the page-table process.
    let vm_proc = proc_table
        .get(crate::proc::proc_nr::VM_PROC_NR)
        .expect("VM process must be initialized before init_post_and_memory");
    assert!(
        vm_proc.p_seg.phys_root.0 != 0,
        "VM page-table root (phys) must be valid after stage C"
    );
    assert!(
        vm_proc.p_seg.virt_root.is_some(),
        "VM page-table root (virt) must be kernel-mapped after stage C"
    );

    // Step 2: Confirm the kernel-level ptproc is already recorded (this step
    // itself records nothing — see the note below).
    //
    // C: get_cpulocal_var(ptproc) = vm — protect.c:372 (x86) / protect.c:99 (ARM)
    //
    // This is the kernel-level counterpart of C's per-CPU `ptproc`: it lets
    // `dispatch_vmctl(SetAddrSpace)` Step 3 decide whether to reload the
    // hardware root register when the target process is the current ptproc.
    // It is NOT part of the createpde temporary-window mechanism (that arch
    // layer is gone — `PostInitArch`/`MemoryInitArch` were superseded by
    // Direct Map), so it survives Direct Map (07-cross-space-init.md §1.4, P9-4).
    //
    // SAFETY: BKL is held during boot, only this CPU accesses the global.
    // The slot lives in CpuLocal — installed for the BSP in
    // `init_smp_state` (the SMP state must exist first; C: arch_post_init
    // per-CPU parity).

    // Step 3: Assert the VM Direct Map base is configured.
    //
    // The VM Direct Map window is established by `establish_boot_dm` at the
    // arch-boot Step 4 (the “step4 DM coverage” call site); Phase C only
    // records the page-table root (07-cross-space-init.md §3.2). Its
    // base is a per-architecture compile-time constant; this check is a
    // documented-contract assertion that fails fast at boot if a future
    // architecture ever configures a zero base.
    assert!(
        CurrentDirectMap::VM_DIRECT_MAP_BASE != 0,
        "VM direct map base must be configured"
    );
}

// ── Phase E-F: system_init + bsp_finish_booting (08-system-init-boot-finish.md) ──

use core::sync::atomic::Ordering;

#[cfg(test)]

/// Raw accessor for the global clock state.
///
/// # Safety
///
/// Caller must hold the BKL (the timer IRQ handler runs with it held —
/// same contract as the other global accessors).
///
/// **Prefer [`clock_state_with`]** which takes a `BklSection` witness.
pub unsafe fn clock_state() -> &'static mut crate::clock::ClockState {
    // SAFETY: raw-pointer read/write avoids the `static_mut_refs` lint.
    unsafe {
        (*CLOCK_STATE.get())
            .as_mut()
            .expect("CLOCK_STATE not initialized — init_clock_and_interrupts must run first")
    }
}

/// Clock state with BKL witness.
pub fn clock_state_with(
    _section: &crate::smp::BklSection<'_>,
) -> &'static mut crate::clock::ClockState {
    // SAFETY: BklSection witness proves the BKL is held.
    unsafe {
        (*CLOCK_STATE.get())
            .as_mut()
            .expect("CLOCK_STATE not initialized — init_clock_and_interrupts must run first")
    }
}

/// Boot-time clock state accessor: init paths run before
/// the BKL exists and before any IRQ can fire — single-threaded boot.
///
/// # Safety
///
/// Only safe during single-threaded boot (`init_clock_and_interrupts`).
/// After boot, use [`clock_state_with`].
pub unsafe fn clock_state_boot_unchecked() -> &'static mut crate::clock::ClockState {
    // SAFETY: caller guarantees single-threaded boot context.
    unsafe {
        (*CLOCK_STATE.get())
            .as_mut()
            .expect("CLOCK_STATE not initialized — init_clock_and_interrupts must run first")
    }
}

/// Try-accessor: `None` before `init_clock_and_interrupts`.
pub fn try_clock_state() -> Option<&'static mut crate::clock::ClockState> {
    // SAFETY: BKL discipline as above; absence is a valid pre-boot state.
    unsafe { (*CLOCK_STATE.get()).as_mut() }
}

/// Set the kbill_kcall marker (C system.c:160) with BKL witness.
pub fn set_kbill_kcall_with(nr: crate::proc::ProcNr, _section: &crate::smp::BklSection<'_>) {
    // SAFETY: BklSection witness proves the BKL is held.
    unsafe { *KBILL_KCALL.get() = Some(nr) };
}

/// Raw read of the kbill_kcall marker.
///
/// # Safety
///
/// Caller must hold the BKL (production hooks run between BKL-acquiring
/// dispatch and `bkl_unlock`); tests are single-threaded.
///
/// **Prefer [`kbill_kcall_raw_with`]** which takes a `BklSection` witness.
pub unsafe fn kbill_kcall_raw() -> Option<crate::proc::ProcNr> {
    // SAFETY: static is never re-assigned to an invalid value (Option<ProcNr>).
    unsafe { *KBILL_KCALL.get() }
}

/// kbill_kcall marker read with BKL witness.
pub fn kbill_kcall_raw_with(_section: &crate::smp::BklSection<'_>) -> Option<crate::proc::ProcNr> {
    // SAFETY: BklSection witness proves the BKL is held; static is never
    // re-assigned to an invalid value (Option<ProcNr>).
    unsafe { *KBILL_KCALL.get() }
}

/// Consume the kbill_kcall marker: attribute `delta` TSC cycles to the
/// in-flight kernel call's process `p_cycles.kcall`, then clear the
/// marker (C arch_clock.c:279-281). Returns `true` if a marker was
/// present and consumed.
///
/// Called from the context_stop equivalents (`finish_and_restore` /
/// `idle`) while the BKL is still held — C consumes after its early BKL
/// release (arch_clock.c:226-233 vs :279), a window Rust's single-lock
/// discipline closes; single-CPU semantics are identical.
pub(crate) fn consume_kbill_kcall(
    table: &mut crate::proc_table::ProcessTable,
    delta: u64,
    section: &crate::smp::BklSection<'_>,
) -> bool {
    // SAFETY: BKL held by caller (see doc).
    let Some(nr) = kbill_kcall_raw_with(section) else {
        return false;
    };
    if let Some(p) = table.get(nr) {
        p.p_cycles.add_kcall_cycles(delta);
    }
    // SAFETY: as above.
    unsafe { *KBILL_KCALL.get() = None };
    true
}

/// Set the kbill_ipc marker (C `proc.c:607`) with BKL witness.
///
/// `dispatch_ipc` sets it **before** the permission/trace checks (C sets
/// `kbill_ipc = caller_ptr` at `do_ipc` entry); it is a separate marker from
/// `kbill_kcall` — when IPC is issued inside a kernel call both are present,
/// and C's consumption block adds the same delta once each to the same
/// process's `kipc` and `kcall` (`arch_clock.c:274-281` rough estimate).
pub fn set_kbill_ipc_with(nr: crate::proc::ProcNr, _section: &crate::smp::BklSection<'_>) {
    // SAFETY: BklSection witness proves the BKL is held.
    unsafe { *KBILL_IPC.get() = Some(nr) };
}

/// Raw read of the kbill_ipc marker.
///
/// # Safety
///
/// Caller must hold the BKL (production hooks run between BKL-acquiring
/// dispatch and `bkl_unlock`); tests are single-threaded.
pub unsafe fn kbill_ipc_raw() -> Option<crate::proc::ProcNr> {
    // SAFETY: static is never re-assigned to an invalid value (Option<ProcNr>).
    unsafe { *KBILL_IPC.get() }
}

/// Consume the kbill_ipc marker: attribute `delta` TSC cycles to the
/// in-flight IPC's process `p_cycles.kipc`, then clear the marker
/// (C arch_clock.c:274-277). Returns `true` if a marker was
/// present and consumed.
///
/// Called from the same context_stop equivalents as
/// [`consume_kbill_kcall`] and with the **same delta** — C's two blocks are
/// parallel `if`s, `kbill_ipc` first and `kbill_kcall` second, each
/// consuming and clearing its own marker.
pub(crate) fn consume_kbill_ipc(
    table: &mut crate::proc_table::ProcessTable,
    delta: u64,
    section: &crate::smp::BklSection<'_>,
) -> bool {
    // SAFETY: BKL held by caller (see doc).
    let Some(nr) = (unsafe { kbill_ipc_raw() }) else {
        return false;
    };
    // The section parameter exists only as a BklSection RAII witness; this
    // function does not otherwise need it, so `let _` just asserts ownership.
    #[allow(clippy::let_underscore_must_use)]
    let _ = section;
    if let Some(p) = table.get(nr) {
        p.p_cycles.add_kipc_cycles(delta);
    }
    // SAFETY: as above.
    unsafe { *KBILL_IPC.get() = None };
    true
}

/// **The context_stop half of the interrupt entry**: settle the
/// "interrupted segment" against the interrupted process.
///
/// C's `context_stop` has two sources; this function is the second one: the
/// assembly interrupt entry (`hwint_master`, `mpx.S:76-90`) first uses
/// `TEST_INT_IN_KERNEL` to tell whether the interrupt came from user mode or
/// kernel mode — from user mode it pushes the **interrupted process** as the
/// argument and calls `context_stop` (accounting the user segment); from
/// kernel mode it takes the `context_stop_idle` route (no user accounting).
/// After the interrupt is handled, the tail of `switch_to_user` calls
/// `context_stop(proc_addr(KERNEL))` once more to account the kernel segment
/// — both sites share the same TSC baseline, so the two segments neither
/// overlap nor leave a gap.
///
/// Rust's counterpart criterion is the BKL's two states
/// (`bkl_try_lock`'s acquired/inherited in
/// [`crate::irq_manager::dispatch_hardware_irq`]): **free = the interrupt hit
/// user mode or idle** (settle the interrupted party), **inherited = it hit
/// kernel mode** (do not account here; leave it to the switch site).
///
/// Settling does four things (all following C's `context_stop`): quantum
/// decrement ([`crate::clock::decrement_quantum_in_with_delta`],
/// `arch_clock.c:326-330`), state bucket + total cycles + cpuavg
/// ([`account_process_stop`]), and consumption of the kcall/kipc markers
/// (C consumes at both sites; the rough-estimate method is copied verbatim).
///
/// The accounted process is the CPU's `proc_ptr` — during idle it is IDLE
/// (C's `context_stop_idle` is exactly `context_stop(idle_proc)`, and the
/// state bucket lands in CP_IDLE).
pub(crate) fn account_interrupt_stop(section: &crate::smp::BklSection<'_>) {
    account_interrupt_stop_with(section, crate::clock::read_tsc());
}

/// Inject-TSC version of [`account_interrupt_stop`] (for host tests — under
/// `cfg(test)` `read_tsc()` is always 0, so the real delta can only come from
/// the caller).
pub(crate) fn account_interrupt_stop_with(section: &crate::smp::BklSection<'_>, current_tsc: u64) {
    let smp = crate::smp_state_with(section);
    // Use **the same** CPU id as `decrement_quantum_in_with_delta` — the
    // baseline is per-CPU (`CpuLocal::tsc_ctr_switch`), so the wrong CPU
    // yields the wrong baseline.
    let cpu = smp.bsp_cpu_id();
    let Some(nr) = smp.cpu_local(cpu).and_then(|l| l.proc_ptr) else {
        return; // early-boot window: no "current process" yet, nothing to settle
    };
    let table = crate::proc_table_with(section);
    let delta = {
        let Some(p) = table.get_mut(nr) else {
            return;
        };
        // C `arch_clock.c:314`: kernel/idle tasks are quantum-exempt, but the
        // delta is still returned (total cycles and cpuavg are recorded for
        // them too — C's common tail does not distinguish types).
        let (_exhausted, delta) =
            crate::clock::decrement_quantum_in_with_delta(smp, p, current_tsc);
        delta
    };
    if delta == 0 {
        return;
    }
    account_process_stop(table, smp, nr, delta, section);
    // C `arch_clock.c:274-281`: both markers are consumed at **every**
    // context_stop.
    consume_kbill_ipc(table, delta, section);
    consume_kbill_kcall(table, delta, section);
}

// ── Panic diagnostic (C utility.c:22-50) ───────────────────────────────

/// Re-entrancy guard for [`kernel_panic_diagnostic`] (borne out at first
/// boot): if a `util_stacktrace` frame-chain read in the panic context takes
/// a page fault, IDT dispatch panics again (the exception arm reads SMP
/// state), and SMP is necessarily uninitialized before proc_init — the hook
/// itself becomes a second fault source through infinite recursion, and the
/// serial port is flooded with the same message. Hook entry does
/// `swap(true)`: if already inside this hook, skip the stacktrace (the
/// message itself is still printed; the original crash site is not
/// swallowed). The hook never "exits normally and is reused" — the panic
/// tail is an infinite loop, so the flag needs no reset.
static PANIC_DIAG_ACTIVE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Kernel panic renderer: C's `panic()` body minus the shutdown
/// (C utility.c:30-39; `minix_shutdown(0)` at :49 stays deferred — zero
/// Rust foundation, see 27-kernel-utility.md §6.3). Invoked by the
/// minix-rt `#[panic_handler]` through the registered diagnostic hook
/// (dependency direction: the handler lives in minix-rt, which cannot
/// reach the kernel's EarlyConsole / CPU id / stack walker).
///
/// Robustness contract (diagnostic must not become the second fault):
/// every step uses only EarlyConsole writes, the BSP constant, and
/// [`util_stacktrace`] (fail-soft walker). No `.expect()` on the panic
/// path.
fn kernel_panic_diagnostic(message: &str) {
    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
    use minix_arch::EarlyConsole as _;

    // C utility.c:30-36 — "kernel panic: " + formatted message.
    Console::write_str("kernel panic: ");
    Console::write_str(message);
    // C utility.c:38 — "kernel on CPU %d: ". Single-CPU build: always
    // the BSP (per-CPU id lands with the pending todo item); SMP_STATE
    // uninitialized (panic before init_proc_and_boot) falls back to 0.
    // Correction: the original implementation used
    // smp_state_boot_unchecked() (which panics when uninitialized), directly
    // contradicting this function's contract "the diagnostic must not become
    // the second fault; no expect on the panic path" (comment above) — any
    // panic before proc_init would recurse through the hook and panic again,
    // flooding the serial port. Switched to the try form with a BSP fallback.
    let cpu = unsafe { try_smp_state() }
        .map(|smp| smp.bsp_cpu_id().raw() as u64)
        .unwrap_or(0);
    Console::write_str("kernel on CPU ");
    Console::write_hex(cpu);
    Console::write_str(": ");
    // C utility.c:39 — util_stacktrace(). Re-entrancy guard: a panic that
    // enters again inside the hook (typical: a page fault during the stack
    // walk → the exception arm sees SMP uninitialized) only stops the walk
    // and does not expand a second diagnostic.
    if PANIC_DIAG_ACTIVE.swap(true, Ordering::AcqRel) {
        Console::write_str("(stacktrace skipped: recursive panic)\n");
        return;
    }
    crate::stacktrace::util_stacktrace();
}

/// Register [`kernel_panic_diagnostic`] as the minix-rt panic handler's
/// diagnostic hook. Called once from `bsp_finish_booting` (the earliest
/// point where EarlyConsole and SMP_STATE are usable); panics before
/// registration fall back to minix-rt's stage-1 sink path.
pub fn register_panic_diagnostic() {
    minix_types::set_panic_diagnostic_hook(Some(kernel_panic_diagnostic));
}

/// Get a reference to the global process table.
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
///
/// **Prefer [`proc_table_with`]** which takes a `BklSection` witness for
/// compile-time BKL proof (see the review rule on witness parameters). This
/// unsafe version is retained for
/// paths that have not yet been migrated.
pub unsafe fn proc_table() -> &'static mut crate::proc_table::ProcessTable {
    // SAFETY: caller guarantees BKL (or single-threaded boot). We use raw
    // pointer dereference (not `&mut PROC_TABLE`) to avoid the
    // `static_mut_refs` lint (Rust 2024 compatibility).
    unsafe { &mut *PROC_TABLE.get() }
}

/// Get a reference to the global process table with BKL witness.
///
/// The `BklSection` parameter is a compile-time capability token proving
/// the caller holds the BKL. See [`crate::smp::BklGuard::section`] and
/// [`crate::smp::bkl_lock_section`].
pub fn proc_table_with(
    _section: &crate::smp::BklSection<'_>,
) -> &'static mut crate::proc_table::ProcessTable {
    // SAFETY: BklSection witness proves BKL is held. We use raw pointer
    // dereference to avoid the `static_mut_refs` lint (Rust 2024).
    unsafe { &mut *PROC_TABLE.get() }
}

/// Boot-time accessor: access process table without BKL witness.
///
/// # Safety
///
/// Only safe during single-threaded boot (before BKL exists or before
/// secondary CPUs are started). After boot, use [`proc_table_with`].
pub unsafe fn proc_table_boot_unchecked() -> &'static mut crate::proc_table::ProcessTable {
    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { &mut *PROC_TABLE.get() }
}

/// Get a reference to the global privilege table.
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
///
/// **Prefer [`priv_table_with`]** which takes a `BklSection` witness.
pub unsafe fn priv_table() -> &'static mut crate::kpriv::PrivTable {
    // SAFETY: caller guarantees BKL (or single-threaded boot). We use raw
    // pointer dereference (not `&mut PRIV_TABLE`) to avoid the
    // `static_mut_refs` lint (Rust 2024 compatibility).
    unsafe { &mut *PRIV_TABLE.get() }
}

/// Get a reference to the global privilege table with BKL witness.
pub fn priv_table_with(
    _section: &crate::smp::BklSection<'_>,
) -> &'static mut crate::kpriv::PrivTable {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *PRIV_TABLE.get() }
}

/// Boot-time accessor: access privilege table without BKL witness.
///
/// # Safety
///
/// Only safe during single-threaded boot. After boot, use [`priv_table_with`].
pub unsafe fn priv_table_boot_unchecked() -> &'static mut crate::kpriv::PrivTable {
    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { &mut *PRIV_TABLE.get() }
}

/// Get a reference to the global IRQ manager.
///
/// # Panics
///
/// Panics if `IRQ_MANAGER` has not been initialized yet. Initialization
/// happens in `init_clock_and_interrupts` (non-mock builds) or must be
/// done manually in test setups.
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
/// Concurrent access from multiple CPUs without BKL is a data race.
///
/// **Prefer [`irq_manager_with`]** which takes a `BklSection` witness.
pub unsafe fn irq_manager()
-> &'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController> {
    // SAFETY: caller guarantees BKL (or single-threaded boot). We use raw
    // pointer dereference to avoid the `static_mut_refs` lint.
    unsafe { &mut *IRQ_MANAGER.get() }
        .as_mut()
        .expect("IRQ_MANAGER not initialized — init_clock_and_interrupts must run first")
}

/// Get a reference to the global IRQ manager with BKL witness.
///
/// # Panics
///
/// Panics if `IRQ_MANAGER` has not been initialized yet.
pub fn irq_manager_with(
    _section: &crate::smp::BklSection<'_>,
) -> &'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController> {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *IRQ_MANAGER.get() }
        .as_mut()
        .expect("IRQ_MANAGER not initialized — init_clock_and_interrupts must run first")
}

/// Boot-time IRQ manager accessor: hook registration in
/// `bsp_finish_booting` runs before the BKL exists and before any IRQ can
/// fire — single-threaded boot.
///
/// # Safety
///
/// Only safe during single-threaded boot (`bsp_finish_booting` /
/// `init_clock_and_interrupts`). After boot, use [`irq_manager_with`].
pub unsafe fn irq_manager_boot_unchecked()
-> &'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController> {
    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { &mut *IRQ_MANAGER.get() }
        .as_mut()
        .expect("IRQ_MANAGER not initialized — init_clock_and_interrupts must run first")
}

/// Try to get a reference to the global IRQ manager.
///
/// Returns `None` if `IRQ_MANAGER` has not been initialized yet (e.g. in
/// unit tests that skip `init_clock_and_interrupts`). Callers that can
/// tolerate the absence (e.g. `GET_IRQACTIDS` before boot init) should
/// prefer this over [`irq_manager`].
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
///
/// **Prefer [`try_irq_manager_with`]** which takes a `BklSection` witness.
pub unsafe fn try_irq_manager()
-> Option<&'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>> {
    // SAFETY: caller guarantees BKL (or single-threaded boot).
    unsafe { &mut *IRQ_MANAGER.get() }.as_mut()
}

/// Try to get a reference to the global IRQ manager with BKL witness.
pub fn try_irq_manager_with(
    _section: &crate::smp::BklSection<'_>,
) -> Option<&'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>> {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *IRQ_MANAGER.get() }.as_mut()
}

/// Install an empty `IrqManager` into the global `IRQ_MANAGER` for unit
/// tests that exercise dispatch paths calling `irq_manager()` (e.g.
/// `dispatch_clear`'s IRQ-hook cleanup).
///
/// The interrupt controller is constructed via `new` only — `init()` is
/// NOT called, so no hardware I/O is performed. The cleanup loops under
/// test only read the (empty) hook table, so no controller method is
/// ever invoked. This mirrors how `init_clock_and_interrupts` populates
/// the global at boot (lib.rs:656-659), minus the hardware init.
///
/// # Safety
///
/// Caller must ensure single-threaded access. The verification harness
/// runs with `--test-threads=1`; the assignment is idempotent (installs
/// a fresh empty manager each call), so test ordering does not matter.
#[cfg(test)]
pub(crate) unsafe fn init_irq_manager_for_test() {
    unsafe {
        let ctrl = new_test_interrupt_controller();
        // SAFETY: test-only; single-threaded under `--test-threads=1`. Uses
        // `addr_of_mut!` to avoid the `static_mut_refs` lint, same as boot.
        *IRQ_MANAGER.get() = Some(crate::irq_manager::IrqManager::new(ctrl));
    }
}

/// Construct a `CurrentInterruptController` for unit tests without
/// touching hardware.
///
/// B-X: the per-arch descriptor construction lives in
/// `minix_platform::test_support` (arch-dispatched re-export, one
/// `unit_test_irq_desc()` per arch submodule) — the kernel carries no
/// `#[cfg(target_arch)]` arms for behavior selection.
#[cfg(test)]
fn new_test_interrupt_controller() -> minix_plat::CurrentInterruptController {
    use minix_plat::{InterruptRouter, PerCpuInterruptUnit};
    minix_plat::CurrentInterruptController::new(&minix_platform::test_support::unit_test_irq_desc())
}
/// Get a reference to the global SMP state.
///
/// # Panics
///
/// Panics if `SMP_STATE` has not been initialized yet. Initialization
/// happens in `init_proc_and_boot` (non-mock builds).
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
/// Concurrent access from multiple CPUs without BKL is a data race.
///
/// **Prefer [`smp_state_with`]** which takes a `BklSection` witness.
pub unsafe fn smp_state() -> &'static mut crate::smp::SmpState {
    // SAFETY: caller guarantees BKL (or single-threaded boot). We use raw
    // pointer dereference to avoid the `static_mut_refs` lint.
    unsafe { &mut *SMP_STATE.get() }
        .as_mut()
        .expect("SMP_STATE not initialized — init_proc_and_boot must run first")
}

/// Get a reference to the global SMP state with BKL witness.
///
/// # Panics
///
/// Panics if `SMP_STATE` has not been initialized yet.
pub fn smp_state_with(_section: &crate::smp::BklSection<'_>) -> &'static mut crate::smp::SmpState {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *SMP_STATE.get() }
        .as_mut()
        .expect("SMP_STATE not initialized — init_proc_and_boot must run first")
}

/// Boot-time accessor: access SMP state without BKL witness.
///
/// # Safety
///
/// Only safe during single-threaded boot. After boot, use [`smp_state_with`].
pub unsafe fn smp_state_boot_unchecked() -> &'static mut crate::smp::SmpState {
    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { &mut *SMP_STATE.get() }
        .as_mut()
        .expect("SMP_STATE not initialized — init_proc_and_boot must run first")
}

/// Try to get a reference to the global SMP state.
///
/// Returns `None` if `SMP_STATE` has not been initialized yet (e.g. in
/// unit tests that exercise `ProcessTable` methods directly without the
/// full boot sequence). Callers that can tolerate the absence (clock load
/// measurement, cpu id readout) should prefer this over [`smp_state`].
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
///
/// **Prefer [`try_smp_state_with`]** which takes a `BklSection` witness.
pub unsafe fn try_smp_state() -> Option<&'static mut crate::smp::SmpState> {
    // SAFETY: caller guarantees BKL (or single-threaded boot).
    unsafe { &mut *SMP_STATE.get() }.as_mut()
}

/// Try to get a reference to the global SMP state with BKL witness.
pub fn try_smp_state_with(
    _section: &crate::smp::BklSection<'_>,
) -> Option<&'static mut crate::smp::SmpState> {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *SMP_STATE.get() }.as_mut()
}

/// Get a reference to the global KernelInfo.
///
/// Returns `None` if called before kmain stores the info (should never happen
/// in production; only possible in unit tests that skip boot).
///
/// Caller must ensure BKL is held if called after boot initialization.
/// C: `kinfo` global access.
pub(crate) fn kernel_info() -> Option<&'static KernelInfo> {
    // SAFETY: After boot, KERNEL_INFO is read-only.
    // Caller is responsible for BKL synchronization.
    unsafe { (*KERNEL_INFO.get()).as_ref() }
}

/// Get a mutable reference to the global IPC filter pool.
///
/// Caller must ensure BKL is held if called after boot initialization.
/// C: `ipc_filter_pool` global array access.
///
/// Get a mutable reference to the global IPC filter pool with BKL witness.
pub(crate) fn ipc_filter_pool_with(
    _section: &crate::smp::BklSection<'_>,
) -> &'static mut crate::ipc_filter::IpcFilterPool {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *IPC_FILTER_POOL.get() }
}

/// BSP finish booting — the last step of kmain.
///
/// C: bsp_finish_booting() in main.c:38-97
///
/// Transitions the kernel from "initialization" to "running" state:
/// 1. Set vm_running = false (VM not yet started)
/// 2. Unset RTS_PROC_STOP on boot processes
/// 3. Initialize clock timer
/// 4. Initialize FPU
/// 5. Call switch_to_user() — never returns
///
/// The C kernel_may_alloc gate (main.c:105) has no Rust counterpart; the
/// body's Step 8 records why (D01-F08 removed the C-shaped memmap ledger).
///
/// Design decision D7 (08 §3): returns `!` to express never-returning in the type system.
/// Design decision D6 (08 §3): vm_running is CpuLocal for SMP correctness.
///
/// `proc_table` is the `ProcessTable` built by `init_proc_and_boot` and
/// threaded through Phase D→F. Step 2 (bill_ptr/proc_ptr=IDLE) and
/// step 4 (RTS_PROC_STOP unset) operate on it.
/// Program the boot clock source, register the clock IRQ hook, and open the
/// timer gate — the C `boot_cpu_init_timer(system_hz)` three-part sequence
/// (clock.c:294), extracted from `bsp_finish_booting` Step 6 so the L3
/// bring-up test (`test-timer-irq`) reuses the production wiring verbatim.
///
/// Order contract (the reason this is one function):
/// program the source, register the handler, open the gate. Reordering any
/// two steps leaves a live, unhandled interrupt source across boot.
///
/// C: boot_cpu_init_timer — clock.c:294 (init_local_timer +
///    register_local_timer_handler); intr_init/apic gates.
pub fn boot_init_timer() {
    // Step 6: boot_cpu_init_timer(system_hz)
    // C: boot_cpu_init_timer(system_hz) — clock.c:294, called from
    // bsp_finish_booting (main.c:73). This is the
    // ONLY place the boot clock source's hardware is touched, and it
    // mirrors the C three-part sequence exactly:
    //   (a) program the source — C `init_local_timer(freq)`:
    //       PIT divisor on x86_64, CNTP_CVAL (gate kept closed,
    //       CNTP_CTL_EL0 = Enable=0/IMASK=1) on aarch64, mtimecmp
    //       (gate `sie.STIE` untouched) on riscv64.
    //   (b) register the handler — C `register_local_timer_handler`
    //       → `put_irq_handler` (arch_clock.c:190); the Rust
    //       `IrqManager::register_hook` carries the same first-handler
    //       unmask rule (interrupt.c:65), which opens the controller-side
    //       delivery gate (IOAPIC input 2 for ISA IRQ0 / GICR_ISENABLER0
    //       bit 30 / no PLIC line for the local timer) under
    //       `minix_plat::TIMER_IRQ`.
    //   (c) open the module-local gate — the analog of the C APIC path's
    //       gate handling inside `init_local_timer`; on aarch64 it sets
    //       CNTP_CTL_EL0 = Enable=1/IMASK=0, on riscv64 it sets
    //       `sie.STIE`, and on x86_64 it is a documented no-op (the PIT
    //       has no module-local gate; `[ARCH: gate-semantics]`).
    // The three steps run as one sequence at the terminal Phase-F position;
    // separating them would leave a live, unhandled timer across the boot.
    //
    // Instance-based design (04-platform-discovery.md §3.4): construct a
    // transient clock arch instance from the global platform descriptor
    // and call `init_timer` on it.
    use minix_arch::{ClockArch, CurrentClockArch, CurrentTimerIrqGate, TimerIrqGate};
    use minix_platform::platform_desc;
    use minix_types::Endpoint;
    let pd = platform_desc();
    let mut clock_arch = CurrentClockArch::new(pd.timer());
    clock_arch.init_timer(
        crate::clock::DEFAULT_HZ,
        crate::clock::current_cpuid().raw(),
    );

    // Software clock half: register the
    // clock IRQ hook with the global IrqManager. The handler
    // (`clock::clock_irq_handler`) advances the software clock and
    // delivers expired alarm notifications from the CLOCK source.
    // Hardware half (x86_64 asm IRQ stubs + IDT load + entry routing)
    // remains deferred — see todo.md.
    let clock_ep = Endpoint::from_generation_slot(0, crate::proc::proc_nr::CLOCK.0);
    // A1: boot context — no BKL, no IRQs yet; boot_unchecked accessor.
    unsafe { crate::irq_manager_boot_unchecked() }
        .register_hook(
            minix_plat::TIMER_IRQ,
            crate::clock::clock_irq_handler,
            clock_ep,
            minix_plat::IrqNotifyId(0),
            minix_plat::IrqPolicy::REENABLE,
        )
        .expect("register clock IRQ hook: no free slots in IRQ_MANAGER");
    // (c) — gates last, handler already registered (see the Step 6 header
    // for the per-architecture semantics of this call).
    <CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq();

    // x86_64 timer bring-up gap fix (counterpart of C i8259.c intr_init): the
    // 8259A PIC was never initialized before — the firmware-left vector base/
    // mask kept the PIT's IRQ0 from ever reaching the 0x50 gate (over 150 s
    // the tick arm was called fewer than 1000 times). Remap master→0x50 /
    // slave→0x70 and unmask only IRQ0 + the cascade line.
    //
    // The call site is at the end of this phase (after init_timer programs the
    // PIT + register_hook + enable_timer_irq): PIC remapping only changes the
    // 8259's vector base/mask and does not depend on PIT counter programming
    // or IrqManager hook registration. It only has to finish before the timer
    // IRQ is actually delivered (the timer gate is already open here, but the
    // IRQ needs the EOI loop to drive it and pic_init takes effect right
    // afterwards), so placing it last does not affect correctness.
    //
    // x86_64 only: aarch64/riscv64 have no 8259 PIC — the ARM generic timer
    // uses GIC PPI 30, and the riscv64 S-mode timer is a CPU-local interrupt
    // (gated by sie.STIE, no PLIC source); their clock gating is done by
    // enable_timer_irq() above and needs no PIC remapping (ground truth:
    // minix3 i8259.c belongs to arch/i386; per-arch TIMER_IRQ semantics are
    // in plat/src/lib.rs:60-69).
    #[cfg(target_arch = "x86_64")]
    minix_plat::pic_init();
}

#[cfg(not(feature = "mock"))]
fn bsp_finish_booting(
    proc_table: &mut crate::proc_table::ProcessTable,
    smp_state: &mut crate::smp::SmpState,
) -> ! {
    use crate::proc::{ProcNr, RtsFlagsBits, proc_nr};

    // Step -1: register the kernel panic diagnostic (C
    // utility.c:22-50) — as early as the diagnostic context (EarlyConsole,
    // SMP_STATE, util_stacktrace) is usable. Panics before this point fall
    // back to minix-rt's stage-1 sink path (message only).
    register_panic_diagnostic();

    // Step 0: cpu_identify() — probe BSP CPU identity into the global table.
    // C: cpu_identify() — main.c:45, the first statement of bsp_finish_booting;
    // i386 impl arch_system.c:212-243 (CPUID leaves 0/1 → cpu_info[cpu]).
    // Rust: the register read is the arch crate's CurrentCpuIdentity probe
    // (x86 CPUID / aarch64 MIDR_EL1 / riscv64 marchid·mimpid); the result is
    // recorded into the kernel-side CPU_INFO table (smp.rs) — the Rust
    // counterpart of C's cpu_info[CONFIG_MAX_CPUS] (glo.h).
    // AP path: C identifies APs in ap_finish_booting (arch_smp.c:232); the
    // same helper will be called there once SMP bring-up lands (16-smp.md),
    // so single-CPU boot fills only the BSP slot.
    crate::smp::cpu_identify();

    // Step 1: vm_running = 0
    // C: vm_running = 0 — main.c:47 (declared glo.h:74)
    // Rust: global `VM_RUNNING: AtomicBool` (definition below).
    // C keeps vm_running as a plain global (glo.h:74) — the Rust mirror is
    // a global AtomicBool with identical semantics. A future per-CPU split
    // would diverge from C and needs an [ARCH] marker if ever proposed.
    VM_RUNNING.store(false, Ordering::Release);

    // Step 2: bill_ptr = proc_ptr = idle_proc
    // C: get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc) — main.c:54-55
    // Rust: CpuLocal::set_running(IDLE) — see `CpuLocal::set_running` in smp.rs.
    // We plumb this through the (single-CPU) SmpState when one exists. For now
    // we record the intent by setting the bill pointer inside proc_table via
    // the sched-side bookkeeping hook used by the rest of the kernel.
    proc_table.set_bill_to_idle();

    // Step 3: announce() — print MINIX banner
    // C: announce() in main.c:60 — printf("MINIX %s ...\n", OS_RELEASE)
    // Rust: print to EarlyConsole so QEMU serial captures it.
    use crate::kmess::{EarlyConsole, KernelConsole as Console};
    Console::write_str("\nMINIX-RS 0.1.0 (rust rewrite) — scheduling live\n");

    // Step 4: Unset RTS_PROC_STOP on boot processes
    // C: for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++)
    //       RTS_UNSET(proc_addr(i), RTS_PROC_STOP);
    // Rust: ProcessTable::rts_unset auto-enqueues a newly-runnable process
    // (see `ProcessTable::rts_unset` in proc_table.rs). Iterate from 0 (first user boot module) up to
    // but excluding kernel tasks (which were marked PROC_STOP during
    // init_proc_and_boot and must stay stopped — they're invoked lazily).
    // C also skips kernel tasks (the loop is `for i in 0..NR_BOOT_PROCS-NR_TASKS`).
    for nr in 0..(ProcNr(crate::proc::NR_BOOT_PROCS as i32)
        - ProcNr(crate::proc_table::NR_TASKS as i32))
    .0
    {
        proc_table.rts_unset(ProcNr(nr), RtsFlagsBits::PROC_STOP);
    }

    // Step 5: cycles_accounting_init() — set BSP TSC baseline.
    // C: cycles_accounting_init() — proc.c (resets per-CPU cycle counters).
    // Sets the BSP's TSC baseline so the first context switch has a
    // correct reference point. `note_context_switch(tsc)` records
    // `cpu_last_tsc = tsc` and `cpu_last_idle = tsc` in the BSP's
    // per-CPU state.
    // The TSC is the cycle counter (rdtsc on x86-64, CNTPCT_EL0 on aarch64,
    // mtime on riscv64). See `clock::read_tsc` for the per-arch wrapper.
    let tsc = crate::clock::read_tsc();
    let bsp_id = smp_state.bsp_cpu_id();
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.note_context_switch(tsc);
    }
    // Note: `tsc_ctr_switch` is set inside `note_context_switch` only if
    // we extend the API. For now the field starts at 0; the next quantum
    // check will see `cpu_last_tsc = tsc` (a non-zero value) and reset
    // `tsc_ctr_switch` on the first context switch. This matches the
    // deferred-but-functional behavior: the BSP gets a clean TSC baseline.

    // The three-part sequence moved to the standalone
    // `boot_init_timer` (same order, contract intact) so the
    // test-timer-irq bring-up kernel reuses the production wiring instead
    // of duplicating it.
    boot_init_timer();

    // Step 7: fpu_init() — set BSP FPU presence.
    // C: fpu_init() — arch-specific (arch_system.c, i386/earm).
    // This is a global "is FPU present" probe that updates
    // `cpulocals.fpu_presence`. In Rust, `CpuLocal::fpu_presence` is
    // already a `bool` field (see smp.rs:134). All three target
    // architectures (x86-64, aarch64, riscv64) have FPUs, so we set
    // the BSP's fpu_presence to `true`. The per-process FPU init is
    // handled by `CpuContextArch::build_cpu_context` via the per-arch FPU
    // strategy field (`fpu_policy` on x86_64, `fpu_enable_el0` on aarch64,
    // `sstatus.FS` on riscv64 — see 06-proc-init-boot-proc.md §4.1)
    // (already built for every boot process — see init_proc_and_boot).
    //
    // aarch64's CPACR_EL1.FPEN lazy-trap gating is established by the **first
    // finish_and_restore's disable()** (before any user FP instruction), and
    // `FpuArch::init()` (0b01) stays unwired — during the window where the
    // firmware reset value 0b00 holds, the kernel is built without NEON, so no
    // EL1 FP use exists and this is safe; wiring it on real hardware came with
    // a recurrence of the early PM↔VFS handshake wall (a pm fb201 flood), so
    // it is not worth the risk. x86_64/riscv64 init semantics are likewise
    // carried by their boot paths' existing policy bits.
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.fpu_presence = true;
    }

    // Step 8: C's kernel_may_alloc = 0 (main.c:105) has no Rust
    // counterpart: the switch's only reader is the add_memmap assertion;
    // with the ledger replaced by FreeRangeSet, the boot-vs-runtime write
    // permission is guaranteed by the call graph and needs no runtime
    // boolean.

    // Step 8.5: BKL held.
    // C: BKL_LOCK() — main.c:149, early in main(). The acquisition moved to
    // kmain before smp_init (this function's only caller): wait_for_aps's
    // UNLOCK→wait→LOCK dance ends with the BKL held, so re-acquiring here
    // would deadlock the non-reentrant CAS lock. Debug-pinned so a future
    // second caller cannot skip the acquisition silently.
    debug_assert!(
        smp::bkl_is_locked(),
        "bsp_finish_booting requires the BKL (kmain acquires it before smp_init)"
    );

    // Step 9: switch_to_user() — never returns
    // C: switch_to_user(); NOT_REACHABLE;
    // Rust: Divergent function, type `-> !`
    // Covered in detail in 10-switch-to-user.md
    //
    // Suppress unused-idle warning: IDLE slot was used by step 2.
    let _ = proc_nr::IDLE;
    switch_to_user()
}

/// Read the `vm_running` flag. C: `EXTERN int vm_running` — glo.h:74.
pub fn vm_running() -> bool {
    VM_RUNNING.load(Ordering::Acquire)
}

/// Set the `vm_running` flag.
///
/// Called by `dispatch_vmctl(VMCTL_SETADDRSPACE)` when VM completes its
/// address-space switch. C: this write is missing in Minix3 (see
/// `VM_RUNNING` doc comment for details) — Rust corrects the omission.
///
/// # Concurrency
///
/// BKL must be held by the caller (single-writer guarantee). The `Release`
/// ordering ensures the flag is visible to other CPUs after the address-space
/// switch is complete.
pub fn set_vm_running(v: bool) {
    VM_RUNNING.store(v, Ordering::Release);
}

/// The ptproc proc-nr moved from the `CURRENT_PTPROC_NR`
/// global atomic to the `CpuLocal.ptproc` field — C is per-CPU
/// (`get_cpulocal_var(ptproc)`, protect.c:372); the global was a
/// single-CPU-era simplification. Each CPU's own slot is written by its own
/// bring-up path (BSP: `init_proc_and_boot`; AP: `smp_ap_tail`), and read
/// back on the same CPU, so plain non-atomic access under the BKL contract
/// suffices.
///
/// Identity anchor: `current_cpu_id()` (x86-64: `gs:0x10`, programmed by
/// `program_gs` in init_protection per CPU; mock/hosted: BSP).

/// The logical id of the CPU executing this code.
///
/// x86-64: the per-CPU GS area (`gs:0x10`), programmed by `program_gs` —
/// the BSP's area is filled in `init_protection` (from the topology match),
/// each AP's in its `init_ap`. aarch64 reads `MPIDR_EL1.Aff0` through the
/// bring-up map; riscv64 derives the logical id from the per-CPU kernel
/// stack (see the cfg-specific bodies). Mock/hosted builds return BSP
/// (CpuId 0) — single-CPU semantics, matching the mock's whole-world view.
#[cfg(not(target_arch = "riscv64"))]
pub fn current_cpu_id() -> crate::proc::CpuId {
    let id = minix_arch::ap_cpu_id_readback();
    crate::proc::CpuId::new_unchecked(id as u32)
}

/// riscv has no readable S-mode hart id, `tp` is restored from the trap frame
/// on user return, and `sscratch` already carries the user sp. The kernel
/// stack is the stable per-CPU anchor: an AP's stub loads its own stack top,
/// and every kernel entry runs on that stack. The BSP falls back to the slot
/// recorded by `seed_bsp_masks`.
#[cfg(target_arch = "riscv64")]
pub fn current_cpu_id() -> crate::proc::CpuId {
    let sp: usize;
    // SAFETY: reading the stack pointer has no side effects.
    unsafe { core::arch::asm!("mv {}, sp", out(reg) sp) };
    if let Some(cpu) = crate::smp::cpu_id_for_stack_pointer(sp) {
        return cpu;
    }
    crate::proc::CpuId::new_unchecked(crate::smp::bsp_logical())
}

/// Read THIS CPU's ptproc proc-nr.
///
/// Returns `None` if ptproc has not been set yet on this CPU (before the
/// CPU's bring-up path installs VM — C: before `arch_post_init()`).
///
/// # Concurrency
///
/// Caller must hold the BKL (same contract as the other CpuLocal accessors).
/// Stale reads (no BKL) are safe — the only consequence is skipping a CR3
/// reload, which the next context switch corrects.
pub fn current_ptproc_nr() -> Option<crate::proc::ProcNr> {
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = current_cpu_id();
    smp.cpu_local(cpu).and_then(|l| l.ptproc)
}

/// Set THIS CPU's ptproc proc-nr.
///
/// Called by the CPU's own bring-up path: BSP in `init_proc_and_boot`
/// (C: `get_cpulocal_var(ptproc) = vm` — protect.c:372), each AP in its
/// tail. (The arch-level ptproc state C also installs here has no Rust
/// counterpart — it recorded `virt_root` for the createpde temporary
/// window, superseded by Direct Map.)
///
/// # Concurrency
///
/// BKL must be held (or boot-phase single-threaded). The CPU writes only
/// its own slot.
pub fn set_current_ptproc_nr(nr: crate::proc::ProcNr) {
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = current_cpu_id();
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.ptproc = Some(nr);
    }
}

// ── Bootstrap page-table root tracking ──────────────────────────────────────
//
// The bootstrap page table is created by `arch_boot_impl` (Step 1+2 of boot)
// and dropped after `enable()`. Later phases — specifically
// `init_proc_and_boot` loading the VM ELF — need to add more mappings to the
// *same* page table. `from_active_root` ( Paging trait) lets them wrap the
// root into a fresh `Paging` handle, but they first need to know the root's
// physical address.
//
// We record it in this global right after `enable()` succeeds.
//
// The global's role: it is the **bootstrap page-table
// root seed** — written once by `arch_boot_impl`. The *active* root is per
// CPU (`CpuLocal::root_phys`), matching C, which reads the live CR3 register
// in `__switch_address_space` (klib.S:618) — a hardware register that is
// inherently per-CPU. Runtime switches (`set_active_root_tracked`, the
// `switch_address_space` same-root sync, `dispatch_vmctl(VMCTL_SETADDRSPACE)`)
// update the owning CPU's slot and compare against the live register; they
// never touch this global, so a switch on one CPU cannot clobber what
// another CPU's IPC/VM cross-address-space copy reads.
//
// # Concurrency
//
// Single writer during boot (the seed), readers hold the BKL. Without the
// BKL a stale seed read is safe — the only consequence is one extra root
// reload (a TLB flush), which the next context switch corrects.
//
// # SMP
//
// Per-CPU root tracking IS installed for runtime switches,
// mirroring the ptproc migration: `current_root_phys()` reads this
// CPU's `CpuLocal::root_phys`, and `set_active_root_tracked` / the
// `switch_address_space` same-root sync update only that slot. The global
// `CURRENT_ROOT_PHYS` remains the shared **bootstrap seed** — written once
// by `arch_boot_impl` — that a CPU observes until it performs its own first
// address-space switch (APs run no cross-space IPC/VM work before that).

/// Sentinel value indicating `CURRENT_ROOT_PHYS` has not been initialized.
/// Distinct from any valid physical address (4KB-aligned, non-zero).
/// Read THIS CPU's active page-table root (C's live CR3/TTBR0/satp).
///
/// Returns the root installed on this CPU's `CpuLocal::root_phys` once it
/// has performed an address-space switch; before that (and for hosted/unit
/// tests) it falls back to the shared bootstrap seed, returning `None` only
/// if `arch_boot_impl` has not yet run (paging not enabled).
///
/// # Concurrency
///
/// Caller must hold the BKL to observe a consistent value. Without the
/// BKL, the value may be stale — but stale reads are safe because the
/// bootstrap root is never freed during normal operation.
pub fn current_root_phys() -> Option<minix_types::PhysBytes> {
    // The active root is per-CPU (C reads the live CR3/TTBR0/
    // satp register). Prefer THIS CPU's installed root; fall back to the
    // shared bootstrap seed (`CURRENT_ROOT_PHYS`, written only by
    // `arch_boot_impl`) for a CPU that has not switched yet and for the
    // hosted/unit-test path.
    let per_cpu = {
        // `try_smp_state` (not `smp_state_boot_unchecked`): `current_root_phys`
        // is read during `arch_boot_impl`'s bootstrap seed, before
        // `init_proc_and_boot` installs `SMP_STATE` — the checked accessor
        // would panic there. No per-CPU slot yet → fall through to the seed.
        let smp = unsafe { crate::try_smp_state() };
        match smp {
            Some(smp) => smp.cpu_local(current_cpu_id()).and_then(|l| l.root_phys),
            None => None,
        }
    };
    if let Some(r) = per_cpu {
        return Some(r);
    }
    let v = CURRENT_ROOT_PHYS.load(Ordering::Acquire);
    if v == ROOT_PHYS_UNSET {
        None
    } else {
        Some(minix_types::PhysBytes(v))
    }
}

/// Record the bootstrap page-table root physical address.
///
/// Called once from `arch_boot_impl` after `Paging::enable()` succeeds.
/// The value remains valid until the bootstrap table is replaced (e.g.,
/// by a VMCTL SetAddrSpace that installs VM's page table as the active
/// root).
///
/// # Concurrency
///
/// Single-threaded boot context; `Release` ordering is sufficient.
pub fn set_current_root_phys(phys: minix_types::PhysBytes) {
    // Bootstrap seed: record on THIS CPU's slot and in the shared seed so a
    // CPU that has not switched yet still observes the bootstrap root.
    CURRENT_ROOT_PHYS.store(phys.0, Ordering::Release);
    set_this_cpu_root_phys(phys);
}

/// Record `root` as the active page-table root on the **current** CPU only.
///
/// Runtime address-space switches (`set_active_root_tracked`,
/// the `switch_address_space` same-root sync) update the owning CPU's slot
/// without touching the shared bootstrap seed — so a switch on one CPU can
/// cannot clobber what another CPU's IPC/VM cross-address-space copy
/// reads via `current_root_phys()`. Safe to call before `SmpState` is live
/// (no-op on the slot; the bootstrap seed covers that phase).
pub(crate) fn set_this_cpu_root_phys(root: minix_types::PhysBytes) {
    // Tolerate the pre-`SMP_STATE` bootstrap phase (see `current_root_phys`)
    // — during `arch_boot_impl` only the shared seed exists; per-CPU slots
    // become live once `init_proc_and_boot` installs `SMP_STATE`.
    if let Some(smp) = unsafe { crate::try_smp_state() } {
        let cpu = current_cpu_id();
        if let Some(local) = smp.cpu_local_mut(cpu) {
            local.root_phys = Some(root);
        }
    }
}

/// Install a page-table root on the current CPU and keep the software
/// mirror ([`CURRENT_ROOT_PHYS`]) in sync.
///
/// Rust equivalent of C's `write_cr3()`: the hardware write goes through
/// `TlbArch::set_active_root` (CR3 / TTBR0_EL1 / satp per architecture),
/// and `CURRENT_ROOT_PHYS` mirrors the register that C's
/// `__switch_address_space` reads back directly (`mov %cr3, %ecx` —
/// klib.S:618) to skip redundant reloads. Keeping the mirror exact is what
/// makes the scheduler's address-space switch a no-op when the picked
/// process already owns the active root.
///
/// Every site that changes the active root must go through this helper —
/// currently `VMCTL_SETADDRSPACE` (dispatch_vmctl) and the scheduler's
/// `switch_address_space`.
///
/// # Concurrency
///
/// Callers hold the BKL (single-writer guarantee for the mirror).
pub(crate) fn set_active_root_tracked(root: PhysBytes) {
    use minix_arch::TlbArch;
    // Experiment safety net: the experiment flag (an interrupts-off sticky
    // window) must not survive any address-space switch — after a test
    // process aborts/exits the flag is always cleared, so the system never
    // keeps scheduling other processes with interrupts off.
    #[cfg(all(target_arch = "riscv64", not(feature = "mock")))]
    minix_arch::riscv64::trap_return::NK4C_CLI.store(0, core::sync::atomic::Ordering::Relaxed);
    // SAFETY: `root` is a boot-established or VM-validated page-table
    // root (see `TlbArch::set_active_root` safety contract); the caller
    // holds the BKL, so no other CPU is concurrently switching roots.
    unsafe {
        minix_arch::CurrentTlbArch::set_active_root(root);
    }
    // Update only THIS CPU's slot — the shared
    // `CURRENT_ROOT_PHYS` stays the pure bootstrap seed (never clobbered by
    // a runtime switch), matching C's per-CPU CR3 semantics.
    set_this_cpu_root_phys(root);
}

/// Pick the next runnable process and update the bill pointer.
///
/// C: `pick_proc()` — proc.c:1785-1813, including the `bill_ptr` side
/// effect: when the picked process's privilege is BILLABLE, it becomes the
/// recipient of system-time accounting (`get_cpulocal_var(bill_ptr) = rp`
/// — proc.c:1809). The C function reads the *local CPU's* run queues
/// (`get_cpulocal_var(run_q_head)`, proc.c:1801); Rust mirrors this with
/// the per-CPU `ProcessTable::sched[cpu]` array indexed by `cpu`.
///
/// Returns `None` when every queue is empty — the caller falls into
/// `idle()` (C: `while (!(p = pick_proc())) idle();` — proc.c:338).
fn pick_and_bill(
    table: &mut crate::proc_table::ProcessTable,
    smp: &mut crate::smp::SmpState,
    priv_table: &crate::kpriv::PrivTable,
    cpu: crate::proc::CpuId,
) -> Option<crate::proc::ProcNr> {
    let picked = table.sched_for_cpu(cpu).pick_proc(table.procs_slice())?;

    // C: proc.c:1808-1809 — `if (priv(rp)->s_flags & BILLABLE)
    // get_cpulocal_var(bill_ptr) = rp;`
    if is_billable(table, priv_table, picked) {
        if let Some(local) = smp.cpu_local_mut(cpu) {
            local.bill_ptr = Some(picked);
        }
    }
    Some(picked)
}

/// Whether a process's privilege is BILLABLE (receives CPU-time billing).
///
/// C: `priv(p)->s_flags & BILLABLE` — proc.c:186 (idle) / proc.c:1808
/// (pick_proc). Kernel tasks like IDLE are billable (IDL_F = SYS_PROC |
/// BILLABLE); a missing priv slot or priv id means "not billable".
fn is_billable(
    table: &crate::proc_table::ProcessTable,
    priv_table: &crate::kpriv::PrivTable,
    nr: crate::proc::ProcNr,
) -> bool {
    table
        .get(nr)
        .and_then(|p| p.priv_id)
        .and_then(|pid| priv_table.get(pid))
        .is_some_and(|k| k.is_billable())
}

/// Re-queue a PREEMPTED process according to its remaining quantum.
///
/// C: proc.c:322-330 (inside `not_runnable_pick_new`). The flag is cleared
/// with the raw flag primitive — deliberately NOT `rts_unset`, whose
/// auto-enqueue is tail-only; C re-decides head-vs-tail from
/// `p_cpu_time_left` (a process preempted mid-quantum re-enters at the
/// HEAD of its priority queue to finish its slice). Both the head and tail
/// re-enqueue target the process's assigned CPU (`rp->p_cpu`, C
/// proc.c:1688/1614), not a hardcoded one.
///
/// A process that is not runnable after the clear (blocked again by the
/// preempting work) is left alone — C: proc.c:324 guards the enqueue the
/// same way.
fn requeue_if_preempted(table: &mut crate::proc_table::ProcessTable, nr: crate::proc::ProcNr) {
    use crate::proc::RtsFlagsBits;
    use core::sync::atomic::Ordering;

    let preempted = table
        .get(nr)
        .is_some_and(|p| p.p_rts_flags.is_set(RtsFlagsBits::PREEMPTED));
    if !preempted {
        return;
    }
    if let Some(p) = table.get_mut(nr) {
        p.p_rts_flags.clear(RtsFlagsBits::PREEMPTED);
    }
    if table.get(nr).is_some_and(|p| p.is_runnable()) {
        let has_quantum = table
            .get(nr)
            .is_some_and(|p| p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0);
        // C routes both the head and tail re-enqueue to the process's own
        // CPU (`rp->p_cpu`), never a hardcoded BSP.
        let cpu_id = table.get(nr).map_or(crate::proc::CpuId::BSP, |p| {
            crate::proc::CpuId::new_unchecked(p.p_sched.cpu.load(Ordering::Acquire))
        });
        if has_quantum {
            table.sched_enqueue_head(nr, cpu_id);
        } else {
            // C proc.c:326-330 — the slice is spent, so the process
            // re-enters at the TAIL via enqueue(p), which also evaluates
            // preemption against the CPU-local current.
            table.sched_enqueue(nr, cpu_id);
        }
    }
}

/// The CPU has nothing to run: become IDLE until the next interrupt.
///
/// C: `idle()` — proc.c:175-229. Sequence and single-CPU parity:
///
/// 1. `proc_ptr = idle_proc` (C:185) and `bill_ptr = idle_proc` when IDLE
///    is billable (C:186-187) — idle time is billed to IDLE.
/// 2. `switch_address_space_idle()` (C: proc.c:190 → 161-171) is
///    `CONFIG_SMP`-only in C. On the multi-CPU build (`ncpus > 1`) the Rust
///    port mirrors it — switch to VM's address space before halting so the
///    wake interrupt finds kernel + VM pages mapped (C comment: this "is only
///    a problem if more than 1 cpus are available"). On the single-CPU build
///    it is skipped, exactly like a C build without SMP.
/// 3. `cpu_is_idle = 1` (C:192); on the multi-CPU build the AP branch masks
///    the local timer (`stop_local_timer()`, C:194-196) so a halted AP stays
///    quiet until an interrupt wakes it — this was restored (it had
///    been omitted alongside the adjacent proc.c:190 step). The BSP branch
///    calls `restart_local_timer()` (C:198-204) — see that helper: on the
///    periodic PIT clock source it is a no-op in C too.
/// 4. `context_stop(KERNEL)` (C:207) starts idle-time accounting: the
///    kernel-execution delta since the last switch is charged to the
///    KERNEL pseudo-process (its TSC baseline is advanced).
/// 5. `halt_cpu()` (C:209, klib.S:407-414) enables interrupts and halts.
///    The CPU sleeps until an interrupt wakes it, at which point this
///    function returns and the caller retries `pick_proc()`.
///
/// The `sprofiling` polling variant (C:211-229) is deferred with the
/// statistical-profiling subsystem (`sprofiling == false` in a default
/// build, so C takes the plain `halt_cpu()` branch — parity holds for the
/// default configuration).
///
/// # BKL
///
/// C's `context_stop(KERNEL)` releases the BKL before the halt (the
/// `must_bkl_unlock` branch — arch_clock.c:226-233); the interrupt that
/// ends the idle window re-acquires it at handler entry. Rust mirrors the
/// release before `idle_halt()` and re-acquires on wake, so every shared-
/// state region in the scheduler loop keeps its "runs under BKL" contract
/// (see `process_misc_flags`).
fn idle(
    section: &crate::smp::BklSection<'_>,
    table: &mut crate::proc_table::ProcessTable,
    smp: &mut crate::smp::SmpState,
    priv_table: &crate::kpriv::PrivTable,
    cpu: crate::proc::CpuId,
) {
    use crate::proc::proc_nr;
    use minix_arch::SmpArch;

    let idle_nr = smp
        .cpu_local(cpu)
        .map(|l| l.idle_proc)
        .unwrap_or(proc_nr::IDLE);

    // 1. proc_ptr = idle_proc (C:185).
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.proc_ptr = Some(idle_nr);
    }
    // bill_ptr = idle_proc if BILLABLE (C:186-187).
    if is_billable(table, priv_table, idle_nr)
        && let Some(local) = smp.cpu_local_mut(cpu)
    {
        local.bill_ptr = Some(idle_nr);
    }

    // 2. switch_address_space_idle() (C: proc.c:190 → 161-171) — `CONFIG_SMP`
    //    in C. Before halting, point this CPU at VM's address space so the
    //    kernel + VM pages are guaranteed mapped when the wake interrupt runs;
    //    C's own comment is explicit that this matters "only if more than 1
    //    cpus are available" — exactly the `-smp4` path the standard launcher
    //    exercises. Gated on `ncpus > 1` to mirror C's `#ifdef CONFIG_SMP` and
    //    preserve the single-CPU (`-smp1`) boot milestone. Runs under the BKL
    //    (released only at step 4 below), so the root switch is serialized.
    //    Premise: `switch_address_space` records the root on the hardware
    //    `current_cpu_id()` slot — in production a CPU only ever runs its own
    //    `idle`, so `cpu` (the loop parameter) == the executing hardware CPU.
    if smp.ncpus() > 1 {
        switch_address_space(table, proc_nr::VM_PROC_NR);
    }

    // 3. cpu_is_idle = 1 (C:192-193) + the AP stop-local-timer branch
    //    (C: proc.c:194-196 — `if (cpuid != bsp_cpu_id) stop_local_timer();
    //    else restart_local_timer();`). C masks a halted AP's LAPIC timer so
    //    the AP stays quiet until an interrupt wakes it (C's `context_stop_idle`
    //    at interrupt entry then restarts it). The one-shot local timer is
    //    re-armed unconditionally by `local_tick` on every CPU, so without this
    //    mask a halted AP keeps taking timer ticks forever and never truly
    //    idles. The mask pairs with the adjacent
    //    `switch_address_space_idle` (proc.c:190), completing parity with
    //    proc.c:176-206. The BSP branch keeps
    //    `restart_local_timer`
    //    (a no-op on the auto-reloading clock sources — see the helper).
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.cpu_is_idle = true;
    }
    if smp.ncpus() > 1 && cpu != smp.bsp_cpu_id() {
        // C pairs this AP timer mask with `context_stop_idle`
        // (arch_clock.c:351-369): the wake entries call
        // `context_stop_idle_current`, which clears `cpu_is_idle` and runs
        // `restart_local_timer` (C2D-WAKETIMER), so a woken AP resumes
        // ticking. The C2D prerequisites are all in place (AP tails in
        // `scheduler_loop`, SGI/SWI flag application, riscv RFENCE, bounded
        // ack/READY), and C2E removed the BSP affinity clamp on
        // x86_64/aarch64: the caller's cpu lands on an AP, and
        // `sched_enqueue_with` sends the C `proc.c:1647` SCHED IPI to a
        // different idle CPU. riscv64 keeps a narrow clamp (in `sched_proc`)
        // because its default firmware does not deliver a cross-hart SSIP;
        // `test-smp-ipi-riscv64` passes only with `aclint=on`. That is a
        // platform gate, not a shared-scheduler difference.
        crate::clock::stop_local_timer();
    } else {
        restart_local_timer();
    }

    // 4. context_stop(KERNEL) — charge the kernel-execution delta and
    // advance the TSC baseline (C:207; the quantum decrement itself is
    // skipped for the endpoint < 0 pseudo-process, arch_clock.c:314).
    // # Resolution note: C also accumulates `kernel_ticks[cpu]`
    // and `p->p_cycles` here (arch_clock.c:231-232). `kernel_ticks[cpu]`
    // (glo.h:86) is deliberately NOT wired — a whole-C-tree grep shows no
    // reader anywhere (declare-only in glo.h, accumulate-only in
    // arch_clock.c), so wiring it would be write-only state (the
    // no-write-only-state standard). `p_cycles` IS observable (GET_PROC, misc.rs) and the
    // KERNEL-branch delta is accumulated below.
    let tsc = crate::clock::read_tsc();
    let kernel = table
        .get_mut(proc_nr::KERNEL)
        .expect("idle: KERNEL pseudo-process slot must exist");
    let (_exhausted, tsc_delta) = crate::clock::decrement_quantum_in_with_delta(smp, kernel, tsc);
    // The context_stop KERNEL branch's **entire** cycle accounting
    // (state bucket + total cycles + cpuavg) — a single point, no double
    // counting and no gaps.
    account_process_stop(table, smp, crate::proc::proc_nr::KERNEL, tsc_delta, section);
    // idle's context_stop equivalent also consumes kbill (C's consumption
    // block is the common context_stop tail and does not distinguish
    // USER/KERNEL/IDLE branches).
    if tsc_delta > 0 {
        // C's two blocks are parallel (ipc first, then kcall); the same delta
        // is recorded once in each.
        consume_kbill_ipc(table, tsc_delta, section);
        consume_kbill_kcall(table, tsc_delta, section);
    }

    // 5. BKL release (C: context_stop's must_bkl_unlock — arch_clock.c:
    // 226-233) then halt with interrupts enabled until the next interrupt.
    crate::smp::bkl_unlock();
    minix_arch::CurrentSmpArch::idle_halt();

    // Re-acquire the BKL: the halt window released it and the wake
    // interrupt's handler has returned. Every state access below the
    // return point (pick_proc, queues, priv table) must hold the BKL.
    crate::smp::bkl_lock().transfer();

    // No end-of-idle accounting here — C measures idle time from the NEXT
    // context_stop after the interrupt (proc.c:221-222 comment).
}

/// Re-arm this CPU's local timer after an idle wake.
///
/// C: `restart_local_timer()` — arch_clock.c:168-175. The BSP keeps its
/// PIT/auto-reloading clock source (an `[ARCH]` choice; restarting its LAPIC
/// here would conflict with that source), so the kernel guards the BSP out.
/// APs use the one-shot local timer that `idle()` masks via
/// `stop_local_timer`; the wake interrupt must reload it or the AP never
/// ticks again. Caller holds the BKL.
fn restart_local_timer() {
    #[cfg(not(test))]
    {
        use minix_arch::{ClockArch, CurrentClockArch};
        let section = unsafe { crate::smp::BklSection::assume_held() };
        let cpu = crate::current_cpu_id();
        if cpu == crate::smp_state_with(&section).bsp_cpu_id() {
            return; // BSP stays on the PIT/auto-reloading source
        }
        let icr = crate::clock::local_tick_icr();
        if icr == 0 {
            return; // timer not calibrated yet
        }
        let pd = minix_platform::platform_desc();
        let mut clock = CurrentClockArch::new(pd.timer());
        clock.restart_local_timer(cpu.raw(), icr);
    }
}

/// C `context_stop_idle` (arch_clock.c:351-369): at interrupt entry, read and
/// clear this CPU's `cpu_is_idle`; if it *was* set, restart the local timer.
/// The clearing half alone leaves a woken AP with its one-shot still masked.
/// Caller holds the BKL.
pub(crate) fn context_stop_idle_current() {
    let section = unsafe { crate::smp::BklSection::assume_held() };
    let cpu = crate::current_cpu_id();
    if crate::smp_state_with(&section).clear_idle_flag(cpu) {
        restart_local_timer();
    }
}

/// Switch the active address space to `nr`'s page-table root.
///
/// C: `switch_address_space(p)` → `__switch_address_space(p, &ptproc)` —
/// klib.S:605-626 (i386) / arch_system.c:196-226 (earm). Three outcomes,
/// preserved exactly:
///
/// 1. `p_cr3 == 0` (kernel task): no-op — the process has no own root, so
///    the current kernel mapping stays active (klib.S:610-612). Kernel
///    tasks run in the kernel's address space.
/// 2. `p_cr3 == current CR3`: no-op — reloading the same root would only
///    cost a pointless TLB flush (klib.S:614-620). `ptproc` IS updated
///    defensively (the C asm skips ptproc on this path,
///    but the Rust version cannot distinguish "already the current
///    ptproc" from "mirror diverged from HW" without the extra store;
///    a redundant store is harmless).
/// 3. otherwise: load the root into the MMU (`TlbArch::set_active_root`)
///    and record `p` as the current `ptproc` (klib.S:621-624).
///
/// C reads the live CR3 register for comparison 2; Rust does the same
/// via `TlbArch::get_active_root()` — eliminating the previous mirror-
/// based check that could produce a false skip when the software mirror
/// (`CURRENT_ROOT_PHYS`) diverged from the actual hardware root
/// (root cause: boot-path `set_current_root_phys` writing
/// only the mirror while the scheduler relied on it for the comparison).
///
/// `ptproc` and the active root are tracked **per CPU** (`ptproc` migrated
/// to `CpuLocal::ptproc`, the root to `CpuLocal::root_phys`), matching C's
/// `get_cpulocal_var(ptproc)` and its
/// read of the live per-CPU CR3. `dispatch_vmctl(VMCTL_SETADDRSPACE)`
/// compares against the same per-CPU slots.
///
/// # BKL
///
/// Caller holds the BKL (scheduler loop) — same protection C relies on.
fn switch_address_space(table: &crate::proc_table::ProcessTable, nr: crate::proc::ProcNr) {
    let idx = match crate::proc_table::nr_to_idx(nr) {
        Some(i) => i,
        None => return,
    };
    let root = match table.get_by_index(idx) {
        Some(p) => p.p_seg.phys_root,
        None => return,
    };

    if root.0 == 0 {
        return; // kernel task — keep the kernel address space (klib.S:611-612)
    }
    // Read the LIVE hardware register (CR3/TTBR0/satp)
    // instead of the software mirror `CURRENT_ROOT_PHYS`. The C scheduler
    // does `mov %cr3, %ecx` (klib.S:618) — comparing against the actual
    // loaded root. The mirror could diverge from hardware when
    // `VMCTL_SETADDRSPACE` updates `p_seg.phys_root` for a not-yet-running
    // process and a later `set_current_root_phys` (boot path or other)
    // writes only the mirror; in that scenario the old mirror-based check
    // falsely skipped the hardware load, leaving TTBR0 pointing at a
    // foreign page table.
    use minix_arch::TlbArch;
    // SAFETY: paging is enabled (kernel code executing); BKL held by caller
    // so no other CPU concurrently switches the root.
    let live_root = unsafe { minix_arch::CurrentTlbArch::get_active_root() };
    if live_root == root {
        // already active — skip the TLB flush (klib.S:618-620).
        // Sync the mirror unconditionally: if HW matches root but the
        // mirror is stale (the exact bug class we're fixing), bring it
        // up to date so future consumers see a consistent value.
        set_this_cpu_root_phys(root);
        crate::set_current_ptproc_nr(nr);
        return;
    }
    set_active_root_tracked(root);
    crate::set_current_ptproc_nr(nr); // klib.S:622-624
}

/// Final dispatch: last scheduler bookkeeping, then transfer to user mode.
///
/// C: proc.c:437-474 (from `arch_finish_switch_to_user()` to
/// `restore_user_context()`). Steps in C order, with the Rust mapping:
///
/// 1. `arch_finish_switch_to_user()` (arch_system.c:495-513) has two
///    effects: (a) store the process pointer at the kernel-stack top for
///    the assembly restore path — no Rust counterpart, the Rust restore
///    receives state as values, not via stack layout; (b) OR `IF_MASK`
///    into the saved PSW so the restored context runs with interrupts
///    enabled — moved into the arch restore impls (the `TrapReturnArch`
///    contract, guarantee 2), where it belongs to the mode-switch
///    instruction boundary.
/// 2. `context_stop(KERNEL)` (C:440): charge the kernel-execution delta to
///    the KERNEL pseudo-process and advance the TSC baseline — the Rust
///    `decrement_quantum_in` (quantum-exempt for endpoint < 0). On SMP this
///    is also where C releases the BKL (`must_bkl_unlock`,
///    arch_clock.c:226-233); Rust releases it explicitly right after.
/// 3. FPU ownership (C:443-446): a non-owner gets the FP-exception trap
///    (`enable_fpu_exception` sets CR0.TS — Rust `FpuArch::disable`), the
///    owner runs FP instructions directly (`disable_fpu_exception` = clts
///    — Rust `FpuArch::enable`).
/// 4. Clear `MF_CONTEXT_SET` (C:451): the context was just materialized
///    for dispatch; a kernel entry before the next dispatch must save
///    state afresh.
/// 5. `MF_FLUSH_TLB` consume (C:458-464): when the pick point found the
///    process's translations resident on this CPU and flagged stale, flush
///    the local TLB; the flag clears either way. C gates the flag itself on
///    `CONFIG_SMP` (do_vmctl.c:134 sets it only there); the Rust build sets
///    it on the local vminhibit path unconditionally, so the consume runs
///    unconditionally too — a TLB flush is semantically transparent to user
///    code (it changes which translations are cached, never their values).
/// 6. `restart_local_timer()` (C:466) — no-op on auto-reloading clock
///    sources (see the helper's doc comment).
/// 7. Rebuild the trap frame from the process's `cpu_context` and restore.
///    The `cpu_context` plays C's `p_reg` role (simultaneously initial and
///    saved state): EVERY dispatch rebuilds the frame from it, so a
///    process that never ran before and one resuming after a trap take
///    the same path. `restore_to_user` never returns — the loop is
///    re-entered from the next trap, with the BKL re-acquired at entry.
///
/// # Safety (restore call)
///
/// `frame`/`ctx` describe the picked process's saved user state; the
/// address space was switched in `switch_address_space` before the misc/
/// quantum stages; the BKL was released in step 2 — the exact precondition
/// list of `TrapReturnArch::restore_to_user`.
/// The context_stop KERNEL-branch accounting shared by both C:440/C:208
/// call-site equivalents (finish_and_restore step 2 / idle step 4): the
/// pseudo-process's TSC delta lands in the CP_INTR per-state bucket
/// (arch_clock.c:340) and in its `p_cycles` (arch_clock.c:232/250).
///
/// Closure note: before this helper the per-state bucket was missing. This
/// helper performs the **cycle accounting of the context_stop common tail**
/// (C `arch_clock.c:290-340`: the cpuavg block and the state bucket) and
/// charges it per process.
///
/// Three things, all following C's shape:
/// 1. **State bucket**: `tsc_per_state[cpu][classify_cpu_state(p)] += delta`
///    (C splits by process class CP_USER/CP_NICE/CP_SYS/CP_INTR/CP_IDLE, see
///    [`crate::clock::classify_cpu_state`]) — this is what the `getcputicks`
///    family reads;
/// 2. **Total cycles**: `p_cycles.total += delta` (C `p->p_cycles +=
///    tsc_delta`);
/// 3. **CPU average**: accumulate delta into `p_cycles.tick` (C's
///    `p_tick_cycles`); each time a tick's worth of cycles has accumulated
///    (`tpt = tsc_per_ms * 1000 / hz`, C i386's `tsc_per_tick[cpu]`) advance
///    once ([`minix_types::cpuavg::increment`]).
///
/// **Call sites** (C's two context_stop sources, mirrored one-to-one):
/// - the three kernel-context sites (`proc.c:208` idle / `:440`
///   `switch_to_user` tail / `:1956`) → charge **KERNEL** (see the calls in
///   `idle`/`finish_and_restore`);
/// - the assembly interrupt entry (`mpx.S:77-90` pushes the interrupted
///   process) → charge **the interrupted process** (see
///   [`account_interrupt_stop_with`]).
///
/// Both sites share the same TSC baseline (`CpuLocal::tsc_ctr_switch`), so
/// "user segment + kernel segment = total time, with no overlap and no gap"
/// is structurally guaranteed.
fn account_process_stop(
    table: &mut crate::proc_table::ProcessTable,
    smp: &mut crate::smp::SmpState,
    nr: crate::proc::ProcNr,
    tsc_delta: u64,
    section: &crate::smp::BklSection<'_>,
) {
    if tsc_delta == 0 {
        return;
    }
    // 1. State bucket + 2. total cycles: C `arch_clock.c:271` (`p->p_cycles +=
    //    delta`) and `:334-340` (`tsc_per_state[cpu][counter] += delta`).
    //    `smp` is passed in by the caller (it cannot be fetched again through
    //    `smp_state_with` — that would be a second `&mut` alias, which Rust's
    //    aliasing rules forbid).
    let bucket = {
        let priv_table = crate::priv_table_with(section);
        table
            .get(nr)
            .map(|p| crate::clock::classify_cpu_state(p, priv_table))
    };
    if let Some(bucket) = bucket {
        smp.account_tsc_per_state(crate::current_cpu_id(), bucket, tsc_delta);
    }
    if let Some(p) = table.get_mut(nr) {
        p.p_cycles.add_cycles(tsc_delta);
    }
    let clock = crate::clock_state_with(section);
    let hz = clock.hz() as u64;
    if hz == 0 {
        return;
    }
    let uptime = clock.uptime();
    let tsc_per_ms = crate::globals::TSC_PER_MS.load(core::sync::atomic::Ordering::Acquire);
    // C earm's `tsc_per_tick[0] = tsc_per_ms[0] * 1000 / system_hz`
    // (arch_clock.c:44); i386's `cpu_get_freq(cpu) / system_hz` is the same
    // quantity.
    let tpt = tsc_per_ms.saturating_mul(1000) / hz;

    let Some(p) = table.get_mut(nr) else {
        return;
    };
    // C's `p->p_tick_cycles += tsc_delta` runs unconditionally **before** the
    // while loop; `tpt == 0` (cycles/tick not calibrated, e.g. host tests)
    // only keeps the loop from running, and the accumulation still happens.
    let mut tick_cycles = p
        .p_cycles
        .tick
        .load(core::sync::atomic::Ordering::Acquire)
        .saturating_add(tsc_delta);
    // C's `while (tpt > 0 && p->p_tick_cycles >= tpt)`: advance the average
    // once per full tick's worth of cycles (an imprecise clock frequency can
    // advance it several times in a row — C's comment says that is expected).
    let mut snapshot = minix_types::CpuAvgSnap {
        ca_base: p
            .p_cpuavg
            .ca_base
            .load(core::sync::atomic::Ordering::Acquire),
        ca_run: p
            .p_cpuavg
            .ca_run
            .load(core::sync::atomic::Ordering::Acquire),
        ca_last: p
            .p_cpuavg
            .ca_last
            .load(core::sync::atomic::Ordering::Acquire),
        ca_avg: p
            .p_cpuavg
            .ca_avg
            .load(core::sync::atomic::Ordering::Acquire),
        _padding: 0,
    };
    let mut bumped = false;
    // C's condition is `while (tpt > 0 && p->p_tick_cycles >= tpt)` — the
    // **`tpt > 0` guard must not be dropped**: in host tests `TSC_PER_MS` is
    // 0 so `tpt == 0`, and `tick_cycles -= 0` never decreases, making an
    // infinite loop (hit in this batch; see FIXLOG #115).
    while tpt > 0 && tick_cycles >= tpt {
        tick_cycles -= tpt;
        minix_types::cpuavg::increment(&mut snapshot, uptime, hz);
        bumped = true;
    }
    p.p_cycles
        .tick
        .store(tick_cycles, core::sync::atomic::Ordering::Release);
    if bumped {
        use core::sync::atomic::Ordering;
        p.p_cpuavg
            .ca_base
            .store(snapshot.ca_base, Ordering::Release);
        p.p_cpuavg.ca_run.store(snapshot.ca_run, Ordering::Release);
        p.p_cpuavg
            .ca_last
            .store(snapshot.ca_last, Ordering::Release);
        p.p_cpuavg.ca_avg.store(snapshot.ca_avg, Ordering::Release);
    }
}

/// Consume `MF_FLUSH_TLB` at the switch-to-user boundary.
///
/// C: proc.c:459-463 — the flag means "this process's translations may be
/// stale"; at dispatch it is consumed: when the pick point determined the
/// translations are resident on this CPU (`must_refresh`), the TLB is
/// flushed; the flag clears either way (another CPU holding the process as
/// its ptproc refreshes when it next schedules it). The flush verb is
/// injected so host tests can exercise the flag lifecycle without touching
/// the MMU (same pattern as the `proc_cr3` closure injection in
/// `data_copy_vmcheck`).
fn consume_flush_tlb_flag(p: &mut crate::proc::KProcess, must_refresh: bool, flush: impl FnOnce()) {
    if p.p_misc_flags.is_set(crate::proc::MiscFlagsBits::FLUSH_TLB) {
        if must_refresh {
            flush();
        }
        p.p_misc_flags.clear(crate::proc::MiscFlagsBits::FLUSH_TLB);
    }
}

/// riscv64 lazy FPU ownership gating: before returning to user,
/// `finish_and_restore` patches `ctx.sstatus.FS` according to ownership
/// (sstatus bits 13:14). RISC-V Priv ISA FS semantics (see the header doc of
/// `arch/src/riscv64/fpu.rs`):
/// - `Off(0b00)`: FPU instructions trap (this is what gating uses).
/// - `Initial(0b01)` / `Clean(0b10)` / `Dirty(0b11)`: FPU available, no
///   trap; Dirty marks "modified, needs saving".
///
/// Strictly aligned with the gating predicate
/// `riscv64_illegal_insn_is_fpu_trap` (trap_dispatch.rs): only `FS == Off`
/// makes scause=2 count as an FP rotation trap. A non-owner must therefore
/// write `Off` (first FP traps → rotation leg); writing Initial/Clean/Dirty
/// makes the rotation leg unreachable (root cause of the old implementation
/// writing Initial). The owner writes Dirty (live registers continue + need
/// saving).
///
/// Pure function, host-testable (regression pin).
#[cfg(any(target_arch = "riscv64", test))]
fn riscv_lazy_fs_field(is_owner: bool) -> u64 {
    const FS_OFF: u64 = 0b00 << 13;
    const FS_DIRTY: u64 = 0b11 << 13;
    if is_owner { FS_DIRTY } else { FS_OFF }
}

fn finish_and_restore(
    table: &mut crate::proc_table::ProcessTable,
    smp: &mut crate::smp::SmpState,
    picked: crate::proc::ProcNr,
    tlb_must_refresh: bool,
    section: &crate::smp::BklSection<'_>,
) -> ! {
    use core::sync::atomic::Ordering;
    use minix_arch::{
        CpuContextArch, CurrentCpuContextArch, CurrentFpuArch, CurrentTlbArch,
        CurrentTrapReturnArch, FpuArch, TlbArch, TrapReturnArch,
    };

    // C:438 — debug_assert(p->p_cpu_time_left). After the quantum stage
    // every path that reaches here has time left: kernel-scheduled
    // processes had their quantum reset (`sched_proc_no_time`), and an
    // exhausted preemptible process was dequeued (not runnable → the loop
    // re-picked before dispatch).
    debug_assert!(
        table
            .get(picked)
            .is_some_and(|p| p.p_sched.quantum.cpu_time_left.load(Ordering::Acquire) > 0),
        "finish_and_restore: picked process has no quantum left"
    );

    // 2. context_stop(KERNEL) — C:440. `smp` and `kernel` are distinct
    // objects (SmpState owns per-CPU scheduler data, not the process
    // table), so the two &mut borrows — `smp` via reborrow and `kernel`
    // out of `table` — never alias (same disjointness argument as
    // clock::decrement_quantum_in's doc comment).
    let tsc = crate::clock::read_tsc();
    let kernel = table
        .get_mut(crate::proc::proc_nr::KERNEL)
        .expect("finish_and_restore: KERNEL pseudo-process slot must exist");
    let (_exhausted, tsc_delta) = crate::clock::decrement_quantum_in_with_delta(smp, kernel, tsc);
    // Shared context_stop KERNEL-branch accounting (C:440 site — the earlier
    // code here missed BOTH the per-state bucket and the p_cycles accumulation
    // that the idle site had).
    account_process_stop(table, smp, crate::proc::proc_nr::KERNEL, tsc_delta, section);
    // C arch_clock.c:279-281 — consume kbill_kcall with the same
    // whole-delta context_stop uses; must run before the BKL release
    // below (see consume_kbill_kcall doc).
    if tsc_delta > 0 {
        // Same order as idle above (C arch_clock.c:274-281, the two parallel
        // blocks).
        consume_kbill_ipc(table, tsc_delta, section);
        consume_kbill_kcall(table, tsc_delta, section);
    }
    // C releases the BKL inside context_stop (must_bkl_unlock,
    // arch_clock.c:226-233); the restore below is the last kernel act.
    crate::smp::bkl_unlock();

    // 3. FPU ownership — C:443-446.
    // Per-CPU bookkeeping (C reads `get_cpulocal_var(fpu_owner)`): the
    // owner is tracked on the CPU that will run the picked process, not
    // the BSP — on SMP the stale read made non-BSP CPUs see the BSP's
    // owner and skip the disable, leaving both CPUs' user FP unlocked.
    let fpu_owner = smp
        .cpu_local(crate::current_cpu_id())
        .and_then(|l| l.fpu_owner);
    let fpu = CurrentFpuArch::default();
    if fpu_owner != Some(picked) {
        fpu.disable(); // non-owner: next FP instruction traps (#NM) — C: enable_fpu_exception
    } else {
        fpu.enable(); // owner keeps the FPU — C: disable_fpu_exception (clts)
    }
    // riscv FS bits live in sstatus / the frame — a live write is undone by
    // the trap stub's exit-time frame reload, so for user mode the FP gating
    // carrier is ctx. Ownership policy: owner == picked → FS=Dirty(0b11), no
    // trap, and marked "needs saving" (live registers continue — no other FP
    // user has run since its last trap-restore, so the physical registers are
    // still its own); non-owner → **FS=Off(0b00)**, so its first FP
    // instruction traps into the rotation leg (`riscv64_fpu_trap_body`: save
    // the old owner, restore itself, hand over ownership).
    // Old bug (P0, pinned by the retrospective audit of 2026-10-01): the
    // non-owner path wrongly wrote FS=Initial (0b01). But this crate's gating
    // predicate `riscv64_illegal_insn_is_fpu_trap` (trap_dispatch.rs) requires
    // `sstatus.FS==Off(0b00)` to classify an FP gating trap; with FS=Initial
    // "FPU available, no trap" (fpu.rs header doc + RISC-V Priv ISA FS field
    // semantics) ⇒ the first FP after a non-owner returns to user does **not**
    // trap ⇒ the rotation leg is unreachable dead code and riscv user↔user FP
    // is unprotected (FP disabling only covered kernel↔user). The fix is
    // FS_OFF for a non-owner. A live write suffices on aarch64 (CPACR is not
    // in the frame); riscv must go through ctx — both architectures' policy
    // points sit in their respective carriers, mirroring C proc.c:443-446.
    #[cfg(target_arch = "riscv64")]
    if let Some(p) = table.get_mut(picked) {
        let fs = riscv_lazy_fs_field(fpu_owner == Some(picked));
        p.cpu_context.set_fs_field(fs);
    }

    // 4. Clear MF_CONTEXT_SET — C:451.
    if let Some(p) = table.get_mut(picked) {
        p.p_misc_flags
            .clear(crate::proc::MiscFlagsBits::CONTEXT_SET);
    }

    // 5. MF_FLUSH_TLB consume — C:458-464: when the pick point found the
    // process's translations resident on this CPU and flagged stale, flush
    // the local TLB; the flag clears either way.
    if let Some(p) = table.get_mut(picked) {
        consume_flush_tlb_flag(p, tlb_must_refresh, || {
            // SAFETY: paging is enabled (kernel code is executing, about to
            // return to user mode); the flush is CPU-local — C refresh_tlb
            // (earm cpufunc.h:162-168; i386 write_cr3(read_cr3())).
            unsafe { CurrentTlbArch::flush_all() };
        });
    }
    // 6. restart_local_timer — no-op on auto-reloading sources.
    restart_local_timer();

    // 7. Dispatch on the recorded entry style, then rebuild the frame from
    // the arch-private context and restore.
    // C: restore_user_context (arch_system.c:577-610) — read the recorded
    // trap style, clear it (C:585), and select the register restore sequence.
    let (ctx, return_seq, fault_flush_va) = {
        let p = table
            .get_mut(picked)
            .expect("finish_and_restore: picked ProcNr out of table range");
        let style = p.trap_style;
        // Consume the record so the next dispatch cannot reuse this entry's
        // style (C:585 — p_kern_trap_style = KTS_NONE before branching).
        p.trap_style = TrapStyle::NoEntry;
        // F2 minimal closure of the clear point for the kernel-assisted
        // flush: when restoring after fault-fill, invlpg the faulting page
        // right now, with this process's CR3 active. INVLPG only shoots down
        // non-G entries tagged with the current CR3 — the clear point (VM's
        // CR3) cannot kill this process's stale entry, so it must happen in
        // the original context before iretq (real-hardware evidence from
        // 2026-09-22: RS at the same 0x2246c0 addr err=0x15 kept recurring
        // permanently while the memory PTE was already P|U|X).
        let fault_flush_va = p.p_fault_addr;
        p.p_fault_addr = None;
        (p.cpu_context, style.return_sequence(), fault_flush_va)
    };
    match return_seq {
        Some(ReturnSequence::FullContext) => {}
        // Fast-syscall entries save a skinny frame; returning through the
        // full-context path would feed the mode switch user-influenced
        // garbage. The pairing fast return lands with the asm entry work
        // (see the SMP todo list) — until then no entry path records Syscall, so
        // this arm converts silent corruption into a loud stop.
        Some(ReturnSequence::FastSyscall) => {
            panic!("restore_user_context: fast-syscall return sequence lands with S-8")
        }
        // C: arch_system.c:597-598 — panic("no entry trap style known").
        None => panic!("no entry trap style known"),
    }
    let mut frame = <CurrentCpuContextArch as CpuContextArch>::TrapFrame::default();
    <CurrentCpuContextArch as CpuContextArch>::apply_to_trap_frame(&ctx, &mut frame);

    // Fault-page invlpg — at this point CR3 is the restored process's own
    // root (switch_address_space installed it in stage 2); the last point
    // before iretq.
    #[cfg(target_arch = "x86_64")]
    if let Some(va) = fault_flush_va {
        // SAFETY: invlpg is a privileged single-line TLB invalidation; the
        // kernel is at CPL0 and `va` is never dereferenced.
        unsafe {
            core::arch::asm!("invlpg [{0}]", in(reg) va, options(nostack, preserves_flags));
        }
    }
    // The same restore-point flush on riscv64: the fill-root PTE write goes
    // through VmDm direct writes (no sfence — write_pte_dm only gates the
    // flush for KernelDm), so the restored process's TLB still caches the old
    // NP translation from before the fill; after the restore the CPU re-runs
    // the same instruction, still hits the old NP, and faults in a loop
    // (long-window evidence: satp root and fill ptroot agree, the RAM PTE is
    // valid, yet the CPU still faulted 26 times). sfence.vma zero, zero =
    // flush every VA in the whole address space (RISC-V Priv ISA); at this
    // point this hart is about to switch back to the restored process, so its
    // semantics are equivalent to x86 invlpg and broader.
    #[cfg(target_arch = "riscv64")]
    if fault_flush_va.is_some() {
        // Discriminating probe (meant to be removed after use): confirm the
        // sfence really executes in the refault loop — executed but still
        // faulting = QEMU-level hard evidence; not executed = the fix is in
        // the wrong place.
        #[cfg(all(target_arch = "riscv64", not(feature = "mock")))]
        {
            use core::sync::atomic::{AtomicUsize, Ordering as SOrd};
            static SF_N: AtomicUsize = AtomicUsize::new(0);
            if SF_N.load(SOrd::Relaxed) < 8 {
                SF_N.fetch_add(1, SOrd::Relaxed);
            }
        }
        // SAFETY: sfence.vma is a privileged instruction; the kernel is at
        // S-mode and the fence has no memory operand side effects.
        unsafe {
            core::arch::asm!("sfence.vma zero, zero", options(nostack, preserves_flags));
        }
    }
    // riscv64: the restore path re-anchors `sscratch` from the recorded
    // per-CPU trap-stack cell, but that cell is one scalar shared by every
    // hart. An AP's `init_ap` writes its own stack top there; without this
    // per-restore refresh the BSP's next user return would load the AP's
    // stack, `current_cpu_id()` (stack-based) would report the AP, and the
    // user's first kernel call would be attributed to that AP's idle proc
    // (observed as `KC cpu=1 nr=-4` and no VMCTL dispatch on -smp 2).
    #[cfg(target_arch = "riscv64")]
    {
        let cpu = crate::current_cpu_id();
        let stack_top = if cpu == smp.bsp_cpu_id() {
            crate::kernel_info()
                .map(|k| k.kern_stack_top().0)
                .unwrap_or(0)
        } else {
            crate::smp::ap_kernel_stack_top(cpu.raw())
        };
        if stack_top != 0 {
            <CurrentTrapReturnArch as TrapReturnArch>::prepare_user_return(
                cpu.raw(),
                minix_types::VirBytes::new(stack_top),
            );
        }
    }
    // SAFETY: all `TrapReturnArch::restore_to_user` preconditions hold:
    // - the picked process's address space is active (switch_address_space
    //   ran before the misc/quantum stages);
    // - frame/ctx are this process's saved user state (cpu_context is
    //   maintained by the boot/fork/signal paths and the future trap-entry
    //   save path);
    // - the BKL was released in step 2 (the release point C uses);
    // - we run on this CPU's kernel stack with paging enabled.
    unsafe { CurrentTrapReturnArch::restore_to_user(&frame, &ctx) }
}

/// Entry point for the scheduling loop — never returns to the caller.
///
/// C: `switch_to_user()` — proc.c:299-474 (`NOT_REACHABLE` at 473). All
/// three kernel re-entry paths (hardware interrupt, CPU exception,
/// syscall) funnel into this loop; it is the single place where the kernel
/// hands the CPU back to user code. Design decision D10-1: `-> !` — Rust
/// expresses "never returns" in the type system instead of C's
/// `NOT_REACHABLE` comment.
///
/// # BKL (Big Kernel Lock)
///
/// The loop runs **with the BKL held** and releases it at exactly the two
/// points C does:
///
/// - inside `context_stop(KERNEL)` on the dispatch tail
///   (`finish_and_restore` step 2 — arch_clock.c:226-233), i.e. just
///   before user code runs; the next trap entry re-acquires it
///   (`kernel_call_dispatch` / `dispatch_ipc_entry`);
/// - around the idle halt (`idle` step 5), where C's interrupt handlers
///   re-lock at entry.
///
/// This supersedes the earlier stub's "release at the top" arrangement:
/// the misc-flags stage mutates shared kernel state (see
/// `process_misc_flags` — `arch_do_syscall` re-dispatches IPC under the
/// documented "runs under BKL" contract), so the release must happen
/// after those stages, matching C. [ARCH: BKL release point realigned
/// with C's context_stop(KERNEL); single-CPU build, SMP re-validation
/// pending.]
///
/// # Loop structure vs C's gotos
///
/// C threads one function body through two labels
/// (`not_runnable_pick_new`, `check_misc_flags`). Rust re-expresses the
/// same control flow as an outer loop whose body runs the five stages in
/// order; the two C `goto not_runnable_pick_new` exits become `continue`.
/// Stage mapping:
///
/// | Stage | C lines | Rust |
/// |-------|---------|------|
/// | probe current process | proc.c:309-315 | runnability check on `proc_ptr` |
/// | not_runnable_pick_new | proc.c:321-349 | requeue + pick-or-idle + `switch_address_space` |
/// | check_misc_flags | proc.c:351-415 | `process_misc_flags` (bool return replaces goto) |
/// | quantum check | proc.c:421-428 | `check_quantum` (folded re-check) |
/// | finish + restore | proc.c:437-474 | `finish_and_restore` |
///
/// # Safety (global accessors)
///
/// The loop takes `&mut` to three distinct statics (`PROC_TABLE`,
/// `SMP_STATE`, `PRIV_TABLE`) once per invocation. The references never
/// alias (separate `SyncUnsafeCell` statics), and every access region
/// holds the BKL — the single-writer guarantee the unchecked accessors
/// require. This is the same access pattern the boot path
/// (`bsp_finish_booting`) uses; per-CPU dispatch under SMP will replace
/// the boot-unchecked accessors with `BklSection`-witnessed ones.
/// Boot tail: enter the scheduling loop and restore the first runnable
/// process to user mode. Public for QEMU test kernels, which boot the
/// production phases and then hand the CPU to user mode exactly like the
/// production `kmain` tail does (C main.c:73 `bsp_finish_booting`).
pub fn switch_to_user() -> ! {
    // T1 contract marker (test-cmd-smoke.sh stage-3 gate): the point where
    // the kernel enters the scheduling loop and hands the CPU over to
    // user-mode process scheduling (C main.c:73 bsp_finish_booting tails into
    // the scheduling loop).
    boot_stage!("entering scheduler\n");
    // C main.c:73 — bsp_finish_booting tails into switch_to_user with the
    // BKL held. The loop body is the shared `scheduler_loop`; the BSP
    // flavor only names its own CPU (seed + loop are per-CPU now).
    let cpu = unsafe { crate::smp_state_boot_unchecked() }.bsp_cpu_id();
    scheduler_loop(cpu)
}

/// The scheduling loop — shared by the BSP (switch_to_user) and
/// every AP (the kernel-registered AP tail enters here after ap_finish_
/// booting). §3.6: the entering CPU satisfies the same BKL-ownership
/// precondition as the BSP (kernel runs with the BKL held; the guard is
/// witnessed below); acquire/release/yield inside the loop reuse the
/// existing paths unchanged.
///
/// C: smp.c AP main loop parity — init_ap → ap_boot_finished → the same
/// `main()` loop body as the BSP.
#[cfg(not(feature = "mock"))]
pub fn scheduler_loop_addr() -> u64 {
    scheduler_loop as *const () as u64
}

#[cfg(not(feature = "mock"))]
pub fn finish_and_restore_addr() -> u64 {
    finish_and_restore as *const () as u64
}

pub(crate) fn scheduler_loop(cpu: crate::proc::CpuId) -> ! {
    use crate::proc::proc_nr;

    // A1: the BKL is held on entry (boot: acquired before smp_init per
    // C main.c:149; AP: acquired in smp_ap_tail after ap_finish_booting).
    // `assume_held` turns that convention into a debug-asserted witness;
    // everything below consumes `*_with(&section)` accessors, so a lost
    // lock panics instead of silently corrupting the tables.
    let section = unsafe { crate::smp::BklSection::assume_held() };
    let table = crate::proc_table_with(&section);
    let smp = crate::smp_state_with(&section);
    let priv_table = crate::priv_table_with(&section);

    // Observability: record that this CPU entered the loop (the L5
    // assertion reads these flags from the BSP after smp_init).
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.sched_loop_entered = true;
    }

    // Seed proc_ptr = IDLE (C: main.c:54 — bsp_finish_booting step 2's
    // per-CPU half; the accounting half lives in `set_bill_to_idle`).
    // IDLE is never queued (RTS_PROC_STOP), so the first pass falls
    // through to the pick path — the same first-dispatch behavior as C.
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.proc_ptr = Some(proc_nr::IDLE);
    }

    loop {
        // ── Stage 1+2: probe current process / pick a new one ──
        // C: proc.c:309-349. `current` is the per-CPU proc_ptr snapshot;
        // None (or a non-runnable process) routes into the pick path.
        let mut current: Option<crate::proc::ProcNr> = smp.cpu_local(cpu).and_then(|l| l.proc_ptr);

        // C: proc.c:314 — `if (proc_is_runnable(p)) goto check_misc_flags;`
        // The current process is re-dispatched only when it is still
        // runnable; anything else (None seed, blocked, stopped) enters the
        // pick path below.
        let need_pick = current.is_none_or(|nr| !table.get(nr).is_some_and(|p| p.is_runnable()));
        // C: proc.c:306 — `tlb_must_refresh` is schedule()-local.
        let mut tlb_must_refresh = false;
        if need_pick {
            // not_runnable_pick_new — C: proc.c:321-330.
            if let Some(cur) = current {
                requeue_if_preempted(table, cur);
            }
            // C: proc.c:338-340 — `while (!(p = pick_proc())) idle();`
            let picked = loop {
                if let Some(p) = pick_and_bill(table, smp, priv_table, cpu) {
                    break p;
                }
                idle(&section, table, smp, priv_table, cpu);
            };
            // C: proc.c:343 — `get_cpulocal_var(proc_ptr) = p;`
            current = Some(picked);
            if let Some(local) = smp.cpu_local_mut(cpu) {
                local.proc_ptr = Some(picked);
            }
            // C: proc.c:345-347 — if the picked process carries MF_FLUSH_TLB
            // and IS this CPU's ptproc, its translations are resident here
            // and possibly stale: switch_to_user must refresh the TLB even
            // when the root looks unchanged. Computed BEFORE
            // switch_address_space, which may retarget ptproc (C's order).
            tlb_must_refresh = table
                .get(picked)
                .is_some_and(|p| p.needs_tlb_refresh(crate::current_ptproc_nr()));
            // C: proc.c:349 — switch_address_space(p).
            switch_address_space(table, picked);
        }
        let picked = current.expect("scheduler loop: proc_ptr seeded or picked above");

        // ── Stage 3a: KCALL_RESUME consume (C system.c:612-638
        // kernel_call_resume) ──
        // After VM finishes servicing (memreq_reply sets KCALL_RESUME and
        // clears VMREQUEST), a suspended kernel call is re-dispatched here:
        // the kernel re-runs kernel_call (the target memory has now been
        // filled by VM, and check_resumed_caller short-circuits a second
        // suspension), the result is written back to RAX via
        // set_ipc_return_code, and then the normal restore follows. The Fault
        // arm mirrors C's vmresult≠OK → SIGSEGV. Must be consumed before
        // process_misc_flags (the same-named arm there only provides a Fault
        // fallback).
        if table.get(picked).is_some_and(|p| {
            p.p_misc_flags
                .is_set(crate::proc::MiscFlagsBits::KCALL_RESUME)
        }) {
            // State guard: re-dispatch only when Completed (VM has replied);
            // Pending/Fetched means VM is still servicing — skip dispatch this
            // round (continue re-picks; VM is runnable and will get a turn)
            // and resume when a later round sees Completed.
            let state_completed = table
                .get(picked)
                .and_then(|p| p.p_vm_suspend.as_ref())
                .map(|c| matches!(c.state, crate::vm::VmSuspendState::Completed(_)));
            if state_completed != Some(true) {
                continue;
            }
            let resumed = crate::vm::kernel_call_resume(picked, table);
            match resumed {
                crate::vm::VmCheckResult::Ok => {
                    // C: kernel_call_resume re-dispatches with saved.reqmsg
                    // (system.c:627: assert m_source then directly
                    // kernel_call_dispatch(caller, &saved.reqmsg)), not by
                    // copying again from user space — saved_msg was stored by
                    // the VmSuspend arm of the previous finish.
                    let mut msg = match table
                        .get(picked)
                        .and_then(|p| p.p_vm_suspend.as_ref())
                        .and_then(|c| c.saved_msg)
                    {
                        Some(m) => m,
                        None => continue,
                    };
                    let clock_state = unsafe { crate::clock_state_boot_unchecked() };
                    // C: kernel_call_dispatch itself does not take the BKL
                    // (BKL_LOCK exists only in the mpx.S trap entry); Rust's
                    // kernel_call/kernel_call_dispatch package bkl_lock()
                    // before dispatch — the scheduler loop already holds the
                    // lock (&section, assume_held once before the loop), so
                    // re-dispatching through them = non-reentrant spinlock
                    // self-deadlock (borne out on real hardware: after memreq
                    // service completed, RS hung between sa1-after and
                    // pre-restore with no output at all). Therefore use the
                    // inner form with the ambient BKL witness; finish uses
                    // the lock-holding variant (both bkl_unlock calls would
                    // drop the scheduler loop's lock).
                    let result = crate::syscall::kernel_call_dispatch_inner(
                        picked,
                        table,
                        &mut msg,
                        priv_table,
                        clock_state,
                        &section,
                    );
                    // C: kernel_call_resume tail kernel_call_finish
                    // (system.c:636) — it performs the completion half's
                    // reply copy / VmSuspend re-suspension bookkeeping /
                    // NoReply dequeue.
                    // Review fix (P1): the door ownership read from the
                    // suspended context drives finish — a call suspended on
                    // the IPC leg does not eagerly write its completion
                    // directly; when the re-dispatch suspends again
                    // (suspend_for_vm creates a new ctx and does not inherit
                    // the door flag), finish's VmSuspend arm re-sets it on the
                    // new ctx based on eager=false, closing multi-stage
                    // suspension paths.
                    let door_skip = table
                        .get(picked)
                        .and_then(|p| p.p_vm_suspend.as_ref())
                        .is_some_and(|c| c.resume_skip_eager_reply);
                    crate::syscall::kernel_call_finish_holding_bkl(
                        picked, table, &msg, result, priv_table, false, !door_skip,
                    );
                    match result.reply_wire() {
                        // Complete: deliver RAX + clear the suspension state,
                        // then the normal restore follows.
                        // This path is the SYSCALL leg's deferred completion;
                        // RAX uses the same sign as that leg's eager reply
                        // (`reply_wire()` already applies the error/data-code
                        // convention, so no negation here).
                        Some(wire) => {
                            if let Some(p) = table.get_mut(picked) {
                                crate::proc::set_ipc_return_code(p, wire as i64);
                            }
                            if let Some(p) = table.get_mut(picked) {
                                p.clear_vm_suspend();
                            }
                        }
                        // Re-dispatch suspended again (multiple pages/ranges):
                        // finish's VmSuspend arm has stored saved_msg, set
                        // KCALL_RESUME and queued the VM notification (the
                        // lock-holding variant, so the lock still belongs to
                        // the scheduler loop). Here we must not recursively
                        // re-enter scheduler_loop (each suspension level nests
                        // a whole scheduler frame and the kernel stack would
                        // run out) — `continue` re-picks at the same level:
                        // this process is now stopped and will not be picked,
                        // while VM is runnable and will get the CPU.
                        None => {
                            continue;
                        }
                    }
                }
                crate::vm::VmCheckResult::Fault => {
                    // Evidence probe P3a (riscv64, the SIGSEGV consuming leg,
                    // stage 3a): confirm the resume-Fault is consumed from
                    // stage 3a (not from proc_table.rs process_misc_flags'
                    // fallback leg), and print the caller ep + p_delivermsg_vir
                    // + the current page-table root on the spot, reconciling
                    // with P2 (mrr) + P1 (sf-*): whether the Fault's requestor
                    // sits on a console ecall and whether its delivermsg page
                    // has been self-filled. Do not remove yet.
                    #[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
                    {
                        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                        static RFS: AtomicUsize = AtomicUsize::new(0);
                        if RFS.fetch_add(1, AtomicOrd::Relaxed) < 16 {
                            let (ep, pdmv) = table
                                .get(picked)
                                .map_or((0i32, 0u64), |q| (q.p_endpoint.0, q.p_delivermsg_vir.0));
                            let root = crate::current_root_phys().map(|r| r.0).unwrap_or(0);
                        }
                    }
                    crate::syscall_signal::cause_signal(
                        picked,
                        crate::syscall_signal::SIGSEGV,
                        table,
                        priv_table,
                    );
                }
            }
        }

        // ── Stage 3: misc flags (check_misc_flags) ──
        // C: proc.c:351-415. Runs under the BKL (the contract documented
        // on process_misc_flags / arch_do_syscall). A `false` return is
        // C's `goto not_runnable_pick_new` (proc.c:413-414): the process
        // became non-runnable while being serviced.
        if !table.process_misc_flags(picked, &crate::ipc::KernelUserCopy, priv_table) {
            continue;
        }

        // ── Stage 4: quantum check ──
        // C: proc.c:421-428. `check_quantum` folds both C checks:
        // `proc_no_time` when the quantum is exhausted (with its
        // scheduler-notify policy split) and the post-quantum runnability
        // re-check (C:427-428) — one `false` exit back to the pick path.
        if !table.check_quantum(picked, priv_table, &section) {
            continue;
        }

        // ── Stage 5: finish + restore (never returns) ──
        finish_and_restore(table, smp, picked, tlb_must_refresh, &section);
    }
}

// ── Tests ──

#[cfg(test)]
mod identity_device_map_tests {
    //! SD-42: the Step-1 device carve-out, pinned through its pure helpers.
    //! The boot map itself only exists on real targets; what is testable here
    //! is the decision — which blocks split, and which pieces go Device.

    use super::identity_piece_is_device;

    const MIB_2: u64 = 1 << 21;

    #[test]
    fn the_uart_page_drives_its_whole_2mib_piece_to_device() {
        // The UART window is a single 4 KiB page; the piece that contains it
        // is Device (the surrounding 2 MiB is the QEMU virt device range, not
        // RAM), and the neighbouring pieces stay Normal.
        let riscv = [(0x0C00_0000u64, 0x0020_0000u64), (0x1000_0000, 0x1000)];
        assert!(identity_piece_is_device(0x1000_0000, MIB_2, &riscv));
        assert!(!identity_piece_is_device(0x1020_0000, MIB_2, &riscv));
        // PLIC: exactly one 2 MiB piece.
        assert!(identity_piece_is_device(0x0C00_0000, MIB_2, &riscv));
        assert!(!identity_piece_is_device(0x0C20_0000, MIB_2, &riscv));
        // aarch64's window spans its pieces fully.
        let aarch64 = [(0x0800_0000u64, 0x0200_0000u64)];
        assert!(identity_piece_is_device(0x0800_0000, MIB_2, &aarch64));
        assert!(identity_piece_is_device(0x09E0_0000, MIB_2, &aarch64));
        assert!(!identity_piece_is_device(0x0A00_0000, MIB_2, &aarch64));
        // No windows: no piece is Device.
        assert!(!identity_piece_is_device(0x1000_0000, MIB_2, &[]));
    }
}

#[cfg(all(test, feature = "mock"))]
mod tests {
    use super::*;
    // Test-only constant: the production face of this file does not
    // reference it, so the import lives here instead of the top-level use
    // list (where `clippy --workspace`, which builds without `--tests`,
    // would flag it unused).
    use minix_arch::paging::Paging;
    use minix_arch::paging::mock::MockPaging;
    use minix_types::USER_STACK_TOP;

    /// Negative control for D-6: an entry that skipped allocator registration
    /// must stop instead of handing out guessed conventional-memory pages.
    #[test]
    #[should_panic(expected = "boot allocator not registered")]
    fn missing_boot_allocator_registration_fails_closed() {
        let _ = require_boot_region::<minix_arch::arch::boot_arch::MockBootArch>(None);
    }

    /// The registered side of the same contract: once a region is present it
    /// is returned unchanged (the caller's coordinates are not re-derived).
    #[test]
    fn registered_boot_allocator_region_is_returned_unchanged() {
        assert_eq!(
            require_boot_region::<minix_arch::arch::boot_arch::MockBootArch>(Some((
                0x20_0000, 0x30_0000
            ))),
            (0x20_0000, 0x30_0000)
        );
    }

    /// The first VM allocation runs before any deduction is published, so the
    /// exclusion list is the only protection for the root page and the boot
    /// bump region (bootstrap page tables live in the bump).
    #[test]
    fn test_vm_boot_exclusions_cover_kernel_root_and_bump() {
        let (e, n) = vm_boot_exclusions(
            PhysBytes(0x10_0000),
            0x20_0000,
            &[],
            Some(PhysBytes(0x9000_0000)),
            Some((0x8F00_0000, 0x8F20_0000)),
        );
        assert_eq!(n, 3);
        assert_eq!((e[0].base.0, e[0].len), (0x10_0000, 0x20_0000));
        assert_eq!(
            (e[1].base.0, e[1].len),
            (0x9000_0000, crate::vm_handoff::PAGE_SIZE as usize)
        );
        assert_eq!((e[2].base.0, e[2].len), (0x8F00_0000, 0x20_0000));

        // Absent root/bump (mock and UEFI legs) contribute nothing.
        let (_, n) = vm_boot_exclusions(PhysBytes(0x10_0000), 0x20_0000, &[], None, None);
        assert_eq!(n, 1);
    }

    /// The candidate list is one thing; what the allocator actually hands out
    /// is another. Exhaust the allocator and prove no delivered frame comes
    /// from the bump region or the root page.
    #[test]
    fn test_first_vm_allocation_never_lands_in_bump_or_root() {
        use minix_arch::arch::frame::{VmBootAllocError, VmBootAllocator, VmBootRegion};

        let memmap = [MemoryRegion {
            base: PhysBytes(0x10_0000),
            len: 0x40_0000,
        }]; // [1 MiB, 5 MiB)
        let bump = (0x20_0000u64, 0x24_0000u64);
        let root = PhysBytes(0x26_0000);
        let (excl, n) =
            vm_boot_exclusions(PhysBytes(0x30_0000), 0x10_0000, &[], Some(root), Some(bump));
        let regions = VmBootRegion::select_multi::<32>(&memmap, &excl[..n])
            .expect("select_multi")
            .expect("some regions");
        let mut alloc = VmBootAllocator::new(regions);
        let mut pages = 0usize;
        loop {
            match alloc.alloc_page() {
                Ok(frame) => {
                    let a = frame.start().0;
                    assert!(
                        !(a < bump.1 && bump.0 < a + 0x1000),
                        "allocator delivered a bump page at {a:#x}"
                    );
                    assert_ne!(a, root.0, "allocator delivered the root page");
                    pages += 1;
                }
                Err(VmBootAllocError::OutOfMemory) => break,
            }
        }
        // 4 MiB = 1024 frames - kernel 256 - bump 64 - root 1 = 703 deliverable.
        assert_eq!(pages, 703);
    }

    /// Pin the bit semantics of the riscv lazy FPU ownership gating, so the
    /// old bug cannot recur — non-owner must be FS=Off(0b00<<13) (otherwise
    /// the first FP does not trap and the rotation leg is unreachable), owner
    /// must be FS=Dirty(0b11<<13). Cross-checks the gating predicate
    /// `riscv64_illegal_insn_is_fpu_trap` (trap_dispatch.rs): only a
    /// non-owner's Off is classified as a gating trap.
    #[test]
    fn test_riscv_lazy_fs_field_gating() {
        // non-owner → Off (sstatus bits13:14 == 0b00).
        assert_eq!(riscv_lazy_fs_field(false) & (0b11 << 13), 0b00 << 13);
        // owner → Dirty (0b11).
        assert_eq!(riscv_lazy_fs_field(true) & (0b11 << 13), 0b11 << 13);
        // Cross-check with the predicate: a non-owner's FS is classified as an
        // FP gating trap (rotation leg reachable), an owner's FS does not trap
        // (live registers continue, no rotation re-entry).
        assert!(crate::trap_dispatch::riscv64_illegal_insn_is_fpu_trap(
            riscv_lazy_fs_field(false)
        ));
        assert!(!crate::trap_dispatch::riscv64_illegal_insn_is_fpu_trap(
            riscv_lazy_fs_field(true)
        ));
    }

    /// Full boot-flow integration test with MockPaging.
    /// Verifies: identity mapping + kernel mapping + enable → no panic.
    /// C arch_clock.c:279-281: consuming the kbill marker
    /// attributes the whole context_stop delta to the in-flight call's
    /// process `p_cycles.kcall` and clears the marker; with no marker
    /// the consumption is a no-op.
    #[test]
    fn test_consume_kbill_kcall_attributes_delta() {
        let mut table = crate::test_helpers::test_proc_table();
        let nr = crate::proc::ProcNr(0);
        // SAFETY: single-threaded test.
        unsafe { *KBILL_KCALL.get() = Some(nr) };
        let section = crate::smp::bkl_lock_section();
        assert!(consume_kbill_kcall(&mut table, 500, &section));
        crate::smp::bkl_unlock();
        assert_eq!(
            table
                .get(nr)
                .unwrap()
                .p_cycles
                .kcall
                .load(core::sync::atomic::Ordering::Acquire),
            500
        );
        // Marker cleared → second consume reports false, no attribution.
        // SAFETY: single-threaded test.
        assert!(unsafe { kbill_kcall_raw() }.is_none());
        let section = crate::smp::bkl_lock_section();
        assert!(!consume_kbill_kcall(&mut table, 100, &section));
        crate::smp::bkl_unlock();
        assert_eq!(
            table
                .get(nr)
                .unwrap()
                .p_cycles
                .kcall
                .load(core::sync::atomic::Ordering::Acquire),
            500
        );
    }

    /// The kipc half is shaped like the kcall half — set the marker, record
    /// delta into `p_cycles.kipc`, clear the marker; a second consumption
    /// records nothing. C `arch_clock.c:274-277`.
    #[test]
    fn test_consume_kbill_ipc_attributes_delta() {
        let mut table = crate::test_helpers::test_proc_table();
        let nr = crate::proc::ProcNr(0);
        // SAFETY: single-threaded test.
        unsafe { *KBILL_IPC.get() = Some(nr) };
        let section = crate::smp::bkl_lock_section();
        assert!(consume_kbill_ipc(&mut table, 700, &section));
        crate::smp::bkl_unlock();
        assert_eq!(
            table
                .get(nr)
                .unwrap()
                .p_cycles
                .kipc
                .load(core::sync::atomic::Ordering::Acquire),
            700
        );
        // Marker cleared → the second consumption returns false and records
        // nothing.
        // SAFETY: single-threaded test.
        assert!(unsafe { kbill_ipc_raw() }.is_none());
        let section = crate::smp::bkl_lock_section();
        assert!(!consume_kbill_ipc(&mut table, 100, &section));
        crate::smp::bkl_unlock();
        assert_eq!(
            table
                .get(nr)
                .unwrap()
                .p_cycles
                .kipc
                .load(core::sync::atomic::Ordering::Acquire),
            700
        );
    }

    /// The two markers **do not interfere** — C's consumption block is two
    /// parallel `if`s, each recording into its own bucket (`kipc` vs `kcall`),
    /// adding the same delta once in each.
    #[test]
    fn test_kbill_ipc_and_kcall_are_independent() {
        let mut table = crate::test_helpers::test_proc_table();
        let nr = crate::proc::ProcNr(0);
        // SAFETY: single-threaded test.
        unsafe {
            *KBILL_IPC.get() = Some(nr);
            *KBILL_KCALL.get() = Some(nr);
        }
        let section = crate::smp::bkl_lock_section();
        assert!(consume_kbill_ipc(&mut table, 300, &section));
        assert!(consume_kbill_kcall(&mut table, 300, &section));
        crate::smp::bkl_unlock();
        let p = table.get(nr).unwrap();
        assert_eq!(
            p.p_cycles.kipc.load(core::sync::atomic::Ordering::Acquire),
            300
        );
        assert_eq!(
            p.p_cycles.kcall.load(core::sync::atomic::Ordering::Acquire),
            300
        );
    }

    /// Registration wires the kernel renderer into the minix-rt
    /// hook slot; the slot is cleared afterwards so a later test panic
    /// never routes through the EarlyConsole renderer (hosted tests
    /// would SIGSEGV on UART port I/O — the same gate as other port-I/O
    /// tests).
    #[test]
    fn test_register_panic_diagnostic_sets_hook() {
        assert!(minix_types::panic_diagnostic_hook().is_none());
        register_panic_diagnostic();
        assert!(minix_types::panic_diagnostic_hook().is_some());
        // Cleanup FIRST-order concern: no kernel renderer may survive a
        // test.
        minix_types::set_panic_diagnostic_hook(None);
        assert!(minix_types::panic_diagnostic_hook().is_none());
    }

    #[test]
    fn test_boot_flow_identity_and_kernel_map() {
        let memmap: &'static [minix_boot::MemoryRegion] = &[
            minix_boot::MemoryRegion {
                base: PhysBytes(0x200000),
                len: 0x1000000,
            }, // 16MB
        ];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000, // 2MB kernel
            boot_modules: &[],
            platform_sources: &[],
            param_buf: &[],
            reserved_regions: &[],
            source_records: &[],
            authority: minix_boot::RamAuthority::DTB,
        };

        let root_page = PhysBytes(0x1000);
        let mut paging = MockPaging::new_from_page(root_page, minix_arch::paging::PteChannel::Identity);

        let huge_size = MockPaging::FALLBACK_HUGE_PAGE_SIZE;

        // Test Step 1: Identity mapping. The production flags are
        // supervisor-only + executable; `read_write()` would include
        // USER_ACCESSIBLE and hide a flag regression.
        let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let region = &info.memmap[0];
        let mut addr = region.base.0;
        let end = addr + region.len as u64;
        let mut identity_pages = 0u64;
        while addr < end {
            paging
                .map_huge(VirBytes(addr), PhysBytes(addr), huge_size, id_flags)
                .unwrap();
            addr += huge_size.bytes();
            identity_pages += 1;
        }
        assert!(identity_pages > 0, "no identity pages mapped");

        // Verify identity mapping — query a mapped huge-page-aligned address
        let query_addr = region.base.0; // 2 MiB-aligned base, mapped at first iteration
        let result = paging.query(VirBytes(query_addr));
        let (_, flags) = result.expect("identity mapping not found");
        assert!(
            !flags.contains(PageFlags::USER_ACCESSIBLE),
            "identity mapping must be supervisor-only"
        );
        assert!(
            flags.contains(PageFlags::EXECUTABLE),
            "identity mapping must be executable (next fetch after the switch)"
        );

        // Test Step 2: Kernel high-address mapping
        let mut offset = 0u64;
        let mut kernel_pages = 0u64;
        while offset < info.kern_size {
            paging
                .map_huge(
                    VirBytes(info.kern_virt_base.0 + offset),
                    PhysBytes(info.kern_phys_base.0 + offset),
                    huge_size,
                    id_flags,
                )
                .unwrap();
            offset += huge_size.bytes();
            kernel_pages += 1;
        }
        assert!(kernel_pages > 0, "no kernel pages mapped");

        // Test Step 3: Enable paging
        // SAFETY: Test context — page tables were set up by the test above.
        // enable() loads CR3 with the test page table root. No concurrent
        // access since this is single-threaded test code.
        let root_phys = unsafe { paging.enable() };
        assert_eq!(root_phys, PhysBytes(0));
    }

    /// M6 (partial, host-side): Step 1 emits 2 MiB pieces and device-window
    /// pieces carry `DEVICE`; `IDENTITY_MAP_END` is a VA ceiling, not a
    /// coverage promise. This checks the policy without a page-table page
    /// allocator.
    #[test]
    fn boot_identity_windows_are_2mib_and_mark_device_pieces() {
        let device = &[(0x0800_0000u64, 0x0200_0000u64)]; // 128 MiB + 32 MiB
        let windows = &[
            (0x0800_0000u64, 0x0a00_0000u64),
            (0x0020_0000u64, 0x0060_0000u64),
            (0x4020_0000u64, 0x4060_0000u64),
        ];
        let mut mapped_pieces = 0usize;
        for &(raw_start, raw_end) in windows {
            let start = raw_start & !(DEV_PIECE - 1);
            let end = (raw_end + DEV_PIECE - 1) & !(DEV_PIECE - 1);
            assert!(end <= IDENTITY_MAP_END);
            let mut addr = start;
            while addr < end {
                let is_device = identity_piece_is_device(addr, DEV_PIECE, device);
                let overlaps = addr < 0x0A00_0000 && addr + DEV_PIECE > 0x0800_0000;
                assert_eq!(
                    is_device, overlaps,
                    "device flag mismatch at {addr:#x}"
                );
                addr += DEV_PIECE;
                mapped_pieces += 1;
            }
        }
        assert!(mapped_pieces > 0);
        let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        assert!(!id_flags.contains(PageFlags::USER_ACCESSIBLE));
        assert!(id_flags.contains(PageFlags::EXECUTABLE));
    }

    /// Legacy platform sources may have `len == 0`; the window builder then
    /// falls back to the 2 MiB piece containing the address. Producers that
    /// record a length use the full span (tested below).
    #[test]
    fn boot_window_push_2m_covers_the_descriptor_start_piece() {
        let mut windows = BootWindows::new();
        windows.push_2m(0x0800_1234);
        assert_eq!(
            windows.as_slice(),
            &[PhysRange::new(
                PhysBytes(0x0800_0000),
                PhysBytes(0x0800_0000 + DEV_PIECE)
            )
            .unwrap()]
        );
    }

    /// A platform source with an explicit length gets a window over
    /// the full blob, not just the address's 2 MiB piece.
    #[test]
    fn platform_window_uses_the_recorded_blob_length() {
        let source = minix_boot::PlatformDescSource::with_len(
            minix_boot::DTB,
            PhysBytes(0x0800_1234),
            0x4000,
        );
        let mut windows = BootWindows::new();
        push_platform_windows(&mut windows, &[source]);
        assert_eq!(
            windows.as_slice(),
            &[PhysRange::new(PhysBytes(0x0800_1234), PhysBytes(0x0800_5234)).unwrap()]
        );
    }

    #[test]
    #[should_panic(expected = "empty or reversed")]
    fn boot_windows_reject_reversed_ranges() {
        let mut windows = BootWindows::new();
        windows.push(0x2000, 0x1000);
    }

    #[test]
    #[should_panic(expected = "empty or reversed")]
    fn boot_windows_reject_empty_ranges() {
        let mut windows = BootWindows::new();
        windows.push(0x1000, 0x1000);
    }

    #[test]
    fn boot_windows_accept_ranges_below_the_phys_limit() {
        let mut windows = BootWindows::new();
        windows.push(0x1000, 0x2000);
        let _ = windows.check(0x2000);
    }

    #[test]
    #[should_panic(expected = "exceeds the platform physical-address bound")]
    fn boot_windows_reject_ranges_above_the_phys_limit() {
        let mut windows = BootWindows::new();
        windows.push(0x1000, 0x2001);
        let _ = windows.check(0x2000);
    }

    /// Regression: `store_kernel_info` must deep-land the
    /// firmware-heap payloads (`memmap`, `boot_modules`) into the kernel
    /// `.bss` pads — the published global copy may never keep pointing at
    /// boot-shim heap memory (real machine vh1: every conventional memmap
    /// entry read back length 0 at the `vm_handoff::classify` point,
    /// collapsing VM's free list to zero regions).
    #[test]
    fn test_store_kernel_info_lands_firmware_slices() {
        let _boot = crate::test_sync::lock_boot_globals();
        static MEMMAP: &[minix_boot::MemoryRegion] = &[
            minix_boot::MemoryRegion {
                base: PhysBytes(0x10_0000),
                len: 0x40_0000,
            },
            minix_boot::MemoryRegion {
                base: PhysBytes(0x1000_0000),
                len: 0x20_0000,
            },
        ];
        static MODULES: &[minix_boot::BootModule] = &[
            minix_boot::BootModule {
                name: "rs",
                start: PhysBytes(0x20_0000),
                len: 0x1000,
            },
            minix_boot::BootModule {
                name: "vm",
                start: PhysBytes(0x21_0000),
                len: 0x2000,
            },
        ];
        static PLATFORM: &[minix_boot::PlatformDescSource] = &[
            minix_boot::PlatformDescSource::new(minix_boot::DTB, PhysBytes(0x4000_0000)),
            minix_boot::PlatformDescSource::new(minix_boot::RSDP, PhysBytes(0xE0000)),
        ];
        static PARAM: &[u8] = b"ARCH=x86_64\0BOARD=qemu\0";
        let info = KernelInfo {
            memmap: MEMMAP,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000,
            boot_modules: MODULES,
            platform_sources: PLATFORM,
            param_buf: PARAM,
            reserved_regions: &[],
            source_records: &[],
            authority: minix_boot::RamAuthority::DTB,
        };
        store_kernel_info(&info);
        let stored = crate::kernel_info().expect("store_kernel_info published the global copy");
        assert_ne!(
            stored.memmap.as_ptr(),
            MEMMAP.as_ptr(),
            "memmap must be repointed to the .bss landing pad"
        );
        assert_eq!(stored.memmap.len(), 2);
        assert_eq!(stored.memmap[0].base, PhysBytes(0x10_0000));
        assert_eq!(stored.memmap[1].len, 0x20_0000);
        assert_ne!(
            stored.boot_modules.as_ptr(),
            MODULES.as_ptr(),
            "boot_modules must be repointed to the .bss landing pad"
        );
        assert_eq!(stored.boot_modules.len(), 2);
        assert_eq!(stored.boot_modules[0].name, "rs");
        assert_eq!(stored.boot_modules[1].start, PhysBytes(0x21_0000));
        assert_eq!(stored.boot_modules[1].len, 0x2000);
        // The platform-source list and the parameter bytes are landed
        // by the same store.
        assert_ne!(
            stored.platform_sources.as_ptr(),
            PLATFORM.as_ptr(),
            "platform_sources must be repointed to the .bss landing pad"
        );
        assert_eq!(stored.platform_sources.len(), 2);
        assert_eq!(
            stored.platform_sources[0].kind().raw(),
            minix_boot::DTB.raw()
        );
        assert_eq!(
            stored.platform_sources[0].phys_addr(),
            PhysBytes(0x4000_0000)
        );
        assert_ne!(
            stored.param_buf.as_ptr(),
            PARAM.as_ptr(),
            "param_buf must be repointed to the .bss landing pad"
        );
        assert_eq!(stored.param_buf, PARAM);
        // The re-store is a guard failure, not an idempotent no-op — this
        // negative control gives the one-shot contract its bite (deleting
        // the guard turns this test red).
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store_kernel_info(stored);
        }));
        assert!(
            result.is_err(),
            "a second store must fail closed on the I-35(a) one-shot guard"
        );
        // Release the one-shot slot for other boot-global tests.
        // SAFETY: boot-globals lock held; single writer.
        unsafe { *crate::globals::KERNEL_INFO.get() = None };
    }

    /// Minimal `KernelInfo` for the landing-pad fail-fast tests — only the
    /// three sliced payloads matter; everything else is inert filler.
    fn minimal_store_info() -> KernelInfo {
        KernelInfo {
            memmap: &[],
            reserved_regions: &[],
            source_records: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x20_0000),
            kern_size: 0x20_0000,
            boot_modules: &[],
            platform_sources: &[],
            param_buf: &[],
            authority: minix_boot::RamAuthority::DTB,
        }
    }

    /// Fail-fast: `reserved_regions` over the `.bss` landing pad aborts
    /// before any copy — it never truncates.
    #[test]
    #[should_panic(expected = "reserved_regions exceeds the .bss landing pad")]
    fn test_store_kernel_info_rejects_reserved_overflow() {
        let _boot = crate::test_sync::lock_boot_globals();
        static OVERFLOW: [minix_boot::MemoryRegion; crate::globals::RESERVED_REGION_STORE_LEN + 1] =
            [minix_boot::MemoryRegion {
                base: PhysBytes(0),
                len: 0,
            }; crate::globals::RESERVED_REGION_STORE_LEN + 1];
        let mut info = minimal_store_info();
        info.reserved_regions = &OVERFLOW;
        store_kernel_info(&info);
    }

    /// Fail-fast: `memmap` over `MEMMAP_REGION_STORE_LEN` aborts before
    /// the copy loop — a truncating store would publish a wrong free list.
    #[test]
    #[should_panic(expected = "memmap exceeds the .bss landing pad")]
    fn test_store_kernel_info_rejects_memmap_overflow() {
        let _boot = crate::test_sync::lock_boot_globals();
        static OVERFLOW: [minix_boot::MemoryRegion; crate::globals::MEMMAP_REGION_STORE_LEN + 1] =
            [minix_boot::MemoryRegion {
                base: PhysBytes(0),
                len: 0,
            }; crate::globals::MEMMAP_REGION_STORE_LEN + 1];
        let mut info = minimal_store_info();
        info.memmap = &OVERFLOW;
        store_kernel_info(&info);
    }

    /// Fail-fast: more boot modules than `NR_BOOT_MODULES` aborts before
    /// the name-pool copy.
    #[test]
    #[should_panic(expected = "boot_modules exceeds the .bss landing pad")]
    fn test_store_kernel_info_rejects_boot_module_overflow() {
        let _boot = crate::test_sync::lock_boot_globals();
        static OVERFLOW: [minix_boot::BootModule; minix_boot::NR_BOOT_MODULES + 1] =
            [minix_boot::BootModule {
                name: "",
                start: PhysBytes(0),
                len: 0,
            }; minix_boot::NR_BOOT_MODULES + 1];
        let mut info = minimal_store_info();
        info.boot_modules = &OVERFLOW;
        store_kernel_info(&info);
    }

    /// Fail-fast: more platform sources than the landing pad aborts
    /// before any copy.
    #[test]
    #[should_panic(expected = "platform_sources exceeds the .bss landing pad")]
    fn test_store_kernel_info_rejects_platform_overflow() {
        let _boot = crate::test_sync::lock_boot_globals();
        static OVERFLOW: [minix_boot::PlatformDescSource;
            crate::globals::PLATFORM_SOURCE_STORE_LEN + 1] = [minix_boot::PlatformDescSource::new(
            minix_boot::PlatformDescKind::new(0),
            PhysBytes(0),
        );
            crate::globals::PLATFORM_SOURCE_STORE_LEN + 1];
        let mut info = minimal_store_info();
        info.platform_sources = &OVERFLOW;
        store_kernel_info(&info);
    }

    /// Fail-fast: parameter bytes over the C-sized pad abort before
    /// the zero-and-copy step.
    #[test]
    #[should_panic(expected = "param_buf exceeds the .bss landing pad")]
    fn test_store_kernel_info_rejects_param_overflow() {
        let _boot = crate::test_sync::lock_boot_globals();
        static OVERFLOW: [u8; crate::globals::PARAM_BUF_STORE_LEN + 1] =
            [0u8; crate::globals::PARAM_BUF_STORE_LEN + 1];
        let mut info = minimal_store_info();
        info.param_buf = &OVERFLOW;
        store_kernel_info(&info);
    }

    /// Fail-fast: more provenance records than the source landing pad aborts
    /// before any copy.
    #[test]
    #[should_panic(expected = "source_records exceeds the .bss landing pad")]
    fn test_store_kernel_info_rejects_source_overflow() {
        let _boot = crate::test_sync::lock_boot_globals();
        static OVERFLOW: [minix_boot::SourceRecord; crate::globals::SOURCE_RECORD_STORE_LEN + 1] =
            [minix_boot::EMPTY_SOURCE_RECORD; crate::globals::SOURCE_RECORD_STORE_LEN + 1];
        let mut info = minimal_store_info();
        info.source_records = &OVERFLOW;
        store_kernel_info(&info);
    }

    /// Verify empty memmap is handled gracefully (identity pass is a no-op).
    #[test]
    fn test_boot_empty_memmap() {
        let info = KernelInfo {
            memmap: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000,
            boot_modules: &[],
            platform_sources: &[],
            param_buf: &[],
            reserved_regions: &[],
            source_records: &[],
            authority: minix_boot::RamAuthority::DTB,
        };
        let mut paging = MockPaging::new_from_page(PhysBytes(0x1000), minix_arch::paging::PteChannel::Identity);

        // Identity pass should be a no-op with zero regions
        for _region in info.memmap { /* empty */ }

        // Kernel map should still work
        let huge_size = MockPaging::FALLBACK_HUGE_PAGE_SIZE;
        let mut offset = 0u64;
        while offset < info.kern_size {
            paging
                .map_huge(
                    VirBytes(info.kern_virt_base.0 + offset),
                    PhysBytes(info.kern_phys_base.0 + offset),
                    huge_size,
                    PageFlags::kernel_read_write(),
                )
                .unwrap();
            offset += huge_size.bytes();
        }

        // SAFETY: Test context — page tables were set up by the test above.
        // enable() loads CR3 with the test page table root. No concurrent
        // access since this is single-threaded test code.
        unsafe { paging.enable() };
    }

    // ── HigherHalf trait unit tests ──

    use core::sync::atomic::Ordering;

    /// Mock HigherHalf: satisfies the trait without performing a real jump.
    /// The real trait has `fn jump_to_kmain(kinfo: &KernelInfo) -> !` (no
    /// `&self`), so there is no instance state to observe; the mock just
    /// spins.
    struct MockHigherHalf;

    impl boot::HigherHalf for MockHigherHalf {
        /// # Safety
        ///
        /// Test mock — does not actually perform a stack jump; it only enters
        /// an infinite spin loop.
        unsafe fn jump_to_kmain(_kinfo: &KernelInfo) -> ! {
            // Don't actually jump — spin for test purposes.
            loop {
                core::hint::spin_loop();
            }
        }
    }

    /// Verify HigherHalf trait can be implemented by a mock type
    /// (type-system check: associated fn with `-> !` return).
    #[test]
    fn test_higher_half_trait_is_implementable() {
        // Compile-time: MockHigherHalf must implement HigherHalf.
        // This test ensures the trait bounds and signature are correct.
        fn accept_higher_half<H: boot::HigherHalf>() {}
        accept_higher_half::<MockHigherHalf>();
    }

    /// Verify MockPaging queries work correctly after mapping.
    #[test]
    fn test_mock_paging_query_after_map() {
        let mut paging = MockPaging::new_from_page(PhysBytes(0x1000), minix_arch::paging::PteChannel::Identity);
        paging
            .map_huge(
                VirBytes(0x200000),
                PhysBytes(0x200000),
                MockPaging::FALLBACK_HUGE_PAGE_SIZE,
                PageFlags::read_write(),
            )
            .unwrap();
        assert!(
            paging.query(VirBytes(0x200000)).is_some(),
            "mapped page should be queryable at 0x100000"
        );
        assert!(
            paging.query(VirBytes(0x00_0001_0000_0000)).is_none(),
            "unmapped page should not be queryable"
        );
    }

    /// Verify that IDENTITY_MAP_END is aligned for huge-page mapping loops.
    #[test]
    fn test_identity_map_end_alignment() {
        // const assertions moved into `const {}` block so
        // they're checked at compile time, not at test runtime.
        const {
            assert!(
                MockPaging::HUGE_PAGE_SIZE.bytes() > 0,
                "HUGE_PAGE_SIZE must be positive"
            );
        }
        const {
            assert!(
                MockPaging::HUGE_PAGE_SIZE.bytes().is_power_of_two(),
                "HUGE_PAGE_SIZE must be a power of two"
            );
        }
        const {
            assert!(IDENTITY_MAP_END > 0, "IDENTITY_MAP_END must be positive");
        }
        let huge = MockPaging::HUGE_PAGE_SIZE.bytes();
        // IDENTITY_MAP_END / huge should be an integer (no partial pages at boundary)
        assert!(
            IDENTITY_MAP_END.is_multiple_of(huge),
            "IDENTITY_MAP_END (0x{:x}) must be a multiple of HUGE_PAGE_SIZE (0x{:x})",
            IDENTITY_MAP_END,
            huge
        );
    }

    // ── Three-architecture KernelInfo consistency tests ──

    /// Verify x86_64 KernelInfo: kern_phys_base must be 2MB-aligned for huge pages.
    #[test]
    fn test_kernel_info_x86_64_alignment() {
        let kern_phys_base: u64 = 0x200_000; // 2MB, as used in QEMU tests
        let kern_virt_base: u64 = 0xFFFF_8000_0000_0000;
        assert_eq!(
            kern_phys_base % (1 << 21),
            0,
            "x86_64 kern_phys_base must be 2MB-aligned"
        );
        assert_eq!(
            kern_virt_base % (1 << 30),
            0,
            "x86_64 kern_virt_base must be 1GB-aligned for 1GB huge pages"
        );
    }

    /// Verify aarch64 KernelInfo: kern_phys_base must be in QEMU virt RAM range.
    /// QEMU virt aarch64 RAM starts at 0x4000_0000.
    #[test]
    fn test_kernel_info_aarch64_phys_in_ram() {
        let ram_start: u64 = 0x4000_0000; // QEMU virt aarch64 RAM base
        let kern_phys_base: u64 = 0x4020_0000; // 2MB-aligned, within RAM
        assert!(
            kern_phys_base >= ram_start,
            "aarch64 kern_phys_base must be >= QEMU RAM start (0x4000_0000)"
        );
        assert_eq!(
            kern_phys_base % (1 << 21),
            0,
            "aarch64 kern_phys_base must be 2MB-aligned"
        );
    }

    /// Verify riscv64 KernelInfo: kern_virt_base == kern_phys_base (identity mapping in Sv39).
    /// QEMU virt riscv64 RAM starts at 0x8000_0000.
    #[test]
    fn test_kernel_info_riscv64_identity() {
        // riscv64.ld: KERNEL_VIRT_BASE is the Sv39 canonical high half,
        // KERNEL_PHYS_BASE is the QEMU virt DRAM load address. They are not
        // equal; the old fixture used the DRAM base for both and asserted a
        // 1 GiB alignment that the real phys base (0x8020_0000) does not have.
        let kern_phys_base: u64 = 0x8020_0000;
        let kern_virt_base: u64 = 0xFFFF_FFC0_0000_0000;
        assert_ne!(
            kern_virt_base, kern_phys_base,
            "riscv64 image is high-half linked, not identity-mapped"
        );
        assert_eq!(
            kern_phys_base % (1 << 21),
            0,
            "riscv64 kern_phys_base must be 2MiB-aligned for the fallback tier"
        );
        assert_eq!(
            kern_virt_base % (1 << 30),
            0,
            "riscv64 kern_virt_base must be 1GiB-aligned for Sv39 huge pages"
        );
    }

    // ── Linker script constraint validation tests ──
    // These tests verify that the constants in the three *production*
    // linker scripts (`os/kernel-image/*.ld`) satisfy arch_boot_impl's
    // constraints.
    //
    // **Per-architecture KERN_PHYS_BASE values** (from os/kernel-image/*.ld):
    // - x86_64:    0x0020_0000 (2 MB)
    // - aarch64:   0x4020_0000 (QEMU virt RAM base 0x4000_0000 + 2 MB)
    // - riscv64:   0x8020_0000 (QEMU virt DRAM base 0x8000_0000 + 2 MB)

    /// x86_64 linker script: KERN_VIRT_BASE = 0xFFFF800000000000, KERN_PHYS_BASE = 0x200000
    /// Verify these values satisfy arch_boot_impl constraints.
    /// Parse a `NAME = 0x...;` constant from a linker script.
    fn ld_const(script: &str, name: &str) -> u64 {
        let line = script
            .lines()
            .find(|l| l.trim_start().starts_with(name))
            .unwrap_or_else(|| panic!("linker script lacks {name}"));
        let rhs = line
            .split('=')
            .nth(1)
            .unwrap_or_else(|| panic!("bad {name} line"));
        let hex = rhs
            .split(';')
            .next()
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        u64::from_str_radix(hex.trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| panic!("bad {name} value: {hex}"))
    }

    /// The three *production* kernel linker scripts (`os/kernel-image/*.ld`,
    /// selected by `kernel-image/build.rs`) must satisfy the boot policy's
    /// alignment constraints.
    #[test]
    fn linker_scripts_satisfy_boot_alignment_constraints() {
        let cases: [(&str, &str, &str, &str); 3] = [
            (
                "x86_64",
                include_str!("../../kernel-image/x86_64.ld"),
                "KERNEL_HIGH_BASE",
                "KERNEL_PHYS_BASE",
            ),
            (
                "aarch64",
                include_str!("../../kernel-image/aarch64.ld"),
                "KERNEL_VIRT_BASE",
                "KERNEL_PHYS_BASE",
            ),
            (
                "riscv64",
                include_str!("../../kernel-image/riscv64.ld"),
                "KERNEL_VIRT_BASE",
                "KERNEL_PHYS_BASE",
            ),
        ];
        for (arch, script, virt_name, phys_name) in cases {
            let virt_symbol = ld_const(script, virt_name);
            let phys = ld_const(script, phys_name);
            // x86_64's kernel-image link places `.text` at
            // KERNEL_HIGH_BASE + KERNEL_PHYS_BASE (the AT() delta); the
            // policy sees min(p_vaddr), not the raw symbol.
            let virt = if arch == "x86_64" {
                virt_symbol + phys
            } else {
                virt_symbol
            };
            assert_eq!(virt % 0x1000, 0, "{arch}: virt base page-aligned");
            assert_eq!(phys % 0x1000, 0, "{arch}: phys base page-aligned");
            assert_eq!(phys % (1 << 21), 0, "{arch}: phys base 2MiB-aligned");
            assert_eq!(virt % (1 << 21), 0, "{arch}: policy virt base 2MiB-aligned");
            assert!(virt > phys, "{arch}: high-half virt base above phys base");
            assert!(
                script.contains("ALIGN(0x200000)"),
                "{arch}: image span must end on a 2MiB boundary"
            );
        }
    }

    /// Verify that arch_boot_impl rejects misaligned KernelInfo.
    #[test]
    #[should_panic(expected = "kern_phys_base must be page-aligned")]
    fn test_arch_boot_rejects_misaligned_phys() {
        let memmap: &'static [minix_boot::MemoryRegion] = &[];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x1001), // not page-aligned
            kern_size: 0x200_000,
            boot_modules: &[],
            platform_sources: &[],
            param_buf: &[],
            reserved_regions: &[],
            source_records: &[],
            authority: minix_boot::RamAuthority::DTB,
        };
        let root_page = PhysBytes(0x1000);
        let _ = arch_boot_impl::<minix_arch::arch::boot_arch::MockBootArch>(&info, root_page);
    }

    /// Verify that arch_boot_impl rejects zero kern_size.
    #[test]
    #[should_panic(expected = "kern_size must be > 0")]
    fn test_arch_boot_rejects_zero_kern_size() {
        let memmap: &'static [minix_boot::MemoryRegion] = &[];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0, // zero size
            boot_modules: &[],
            platform_sources: &[],
            param_buf: &[],
            reserved_regions: &[],
            source_records: &[],
            authority: minix_boot::RamAuthority::DTB,
        };
        let root_page = PhysBytes(0x1000);
        let _ = arch_boot_impl::<minix_arch::arch::boot_arch::MockBootArch>(&info, root_page);
    }

    /// Verify the bsp_finish_booting Step 5/7 side effects: cycles accounting
    /// init and fpu_presence are set on the BSP's CpuLocal. Since
    /// bsp_finish_booting is divergent (-> !, calls switch_to_user), we test
    /// the side-effect operations directly.
    #[test]
    fn test_bsp_finish_booting_step_5_7_side_effects() {
        use crate::clock::read_tsc;
        use crate::smp::SmpState;

        // Simulate Step 5 + Step 7 of bsp_finish_booting.
        let mut smp = SmpState::new_single_cpu();
        let tsc = read_tsc();
        let bsp = smp.bsp_cpu_id();
        {
            let bsp_local = smp.cpu_local_mut(bsp).unwrap();
            bsp_local.note_context_switch(tsc);
        }
        {
            let bsp_local = smp.cpu_local_mut(bsp).unwrap();
            bsp_local.fpu_presence = true;
        }

        // Verify side effects.
        let bsp_local = smp.cpu_local(bsp).unwrap();
        assert_eq!(
            bsp_local.cpu_last_tsc, tsc,
            "cycles_accounting_init must set cpu_last_tsc"
        );
        assert_eq!(
            bsp_local.cpu_last_idle, tsc,
            "cycles_accounting_init must set cpu_last_idle"
        );
        assert!(
            bsp_local.fpu_presence,
            "fpu_init must set fpu_presence = true"
        );
    }

    /// Global setup for the interrupt-stop tests:
    /// `account_interrupt_stop_with` reads four globals
    /// `SMP_STATE`/`CLOCK_STATE`/`PROC_TABLE`/`PRIV_TABLE` (`PROC_TABLE` is
    /// constructed inline; the other three must be installed) — same approach
    /// as clock.rs's `setup_globals`, installing only what this test group
    /// reads.
    fn setup_c26_globals() {
        crate::smp::bkl_lock_reset_for_test();
        // SAFETY: single-threaded test (workspace forces --test-threads=1);
        // each test re-installs the globals it reads.
        unsafe {
            *crate::globals::SMP_STATE.get() = Some(crate::smp::SmpState::new_single_cpu());
            *crate::globals::CLOCK_STATE.get() = Some(crate::clock::ClockState::new());
        }
    }

    /// Lay out the slot as an "interrupted ordinary user process":
    /// endpoint ≥ 0 (quantum not exempt), the given remaining quantum,
    /// `proc_ptr` pointing at it, and the baseline set to `baseline`.
    fn arm_interrupted_user(nr: crate::proc::ProcNr, quantum_left: u64, baseline: u64) {
        let section = crate::smp::bkl_lock_section();
        {
            let table = crate::proc_table_with(&section);
            let p = table.get_mut(nr).unwrap();
            p.p_nr = nr;
            p.p_endpoint = minix_types::Endpoint(7);
            p.p_rts_flags = crate::proc::RtsFlags::with(crate::proc::RtsFlagsBits::empty());
            // Give it USER privilege so the state bucket lands in CP_USER (C's
            // criterion is `p->p_priv != priv_addr(USER_PRIV_ID)` → CP_SYS; a
            // process with priv_id None also counts as "not USER", so this
            // must be set explicitly).
            p.priv_id = Some(crate::kpriv::USER_PRIV_ID);
            p.p_sched
                .quantum
                .cpu_time_left
                .store(quantum_left, core::sync::atomic::Ordering::Release);
        }
        {
            let smp = crate::smp_state_with(&section);
            let local = smp.cpu_local_mut(crate::proc::CpuId::BSP).unwrap();
            local.set_running(nr);
            local.tsc_ctr_switch = baseline;
        }
        crate::smp::bkl_unlock();
    }

    /// The interrupt entry settles the "interrupted segment" against the
    /// interrupted process — four things in one shot: quantum decrement (C
    /// `arch_clock.c:326-330`), state bucket (CP_USER), total cycles, and the
    /// cpuavg tick accumulation (`p_cycles.tick`).
    #[test]
    fn test_interrupt_stop_charges_interrupted_process() {
        setup_c26_globals();
        let nr = crate::proc::ProcNr(1);
        arm_interrupted_user(nr, 10_000, 1000);
        // The TSC read at the interrupt = 3000, baseline 1000 → delta 2000.
        let section = crate::smp::bkl_lock_section();
        super::account_interrupt_stop_with(&section, 3000);
        crate::smp::bkl_unlock();

        let section = crate::smp::bkl_lock_section();
        let table = crate::proc_table_with(&section);
        let p = table.get(nr).unwrap();
        assert_eq!(
            p.p_sched
                .quantum
                .cpu_time_left
                .load(core::sync::atomic::Ordering::Acquire),
            8_000,
            "quantum 被消费 2000（C 的 p_cpu_time_left -= tsc_delta）"
        );
        assert_eq!(
            p.p_cycles.total.load(core::sync::atomic::Ordering::Acquire),
            2_000,
            "总周期记到被打断的进程（此前只记 KERNEL）"
        );
        assert_eq!(
            p.p_cycles.tick.load(core::sync::atomic::Ordering::Acquire),
            2_000,
            "cpuavg 的滴答账先累加（未攒够 tpt 时不推进均值）"
        );
        let local = crate::smp_state_with(&section)
            .cpu_local(crate::proc::CpuId::BSP)
            .unwrap();
        assert_eq!(
            local.tsc_per_state[crate::clock::CP_USER],
            2_000,
            "状态桶按进程类别落 CP_USER"
        );
        assert_eq!(local.tsc_ctr_switch, 3000, "基线前移到本次读数");
        crate::smp::bkl_unlock();
    }

    /// quantum exhaustion → the next `switch_to_user`'s Stage 4 sees
    /// `check_quantum` return false and sets `RTS_NO_QUANTUM` (C
    /// `proc.c:418-428`) — only with the consuming half wired does this
    /// preemption chain actually trigger.
    #[test]
    fn test_interrupt_stop_exhausts_quantum_then_policy_applies() {
        setup_c26_globals();
        let nr = crate::proc::ProcNr(1);
        arm_interrupted_user(nr, 500, 1000);
        let section = crate::smp::bkl_lock_section();
        super::account_interrupt_stop_with(&section, 3000); // delta 2000 > 500
        crate::smp::bkl_unlock();
        {
            let section = crate::smp::bkl_lock_section();
            assert_eq!(
                crate::proc_table_with(&section)
                    .get(nr)
                    .unwrap()
                    .p_sched
                    .quantum
                    .cpu_time_left
                    .load(core::sync::atomic::Ordering::Acquire),
                0,
                "quantum 饱和到 0（不是回绕）"
            );
            crate::smp::bkl_unlock();
        }
        // Stage 4: `check_quantum` sees "exhausted" (cpu_time_left decreases
        // on the production path), and applies C `proc_no_time`'s **policy**:
        // this process has `p_sched.scheduler == None` (kernel scheduling) →
        // else arm tops the quantum back up and does not set NO_QUANTUM (C
        // proc.c:1893-1910).
        // The user-scheduled + preemptible arm (set NO_QUANTUM + notify the
        // scheduler) needs a live scheduler process and IPC, and is covered
        // by proc_table.rs's existing `sched_proc_no_time` tests.
        let section = crate::smp::bkl_lock_section();
        let priv_table = crate::priv_table_with(&section);
        let _ = crate::proc_table_with(&section).check_quantum(nr, priv_table, &section);
        crate::smp::bkl_unlock();
        let section = crate::smp::bkl_lock_section();
        let p = crate::proc_table_with(&section).get(nr).unwrap();
        assert!(
            p.p_sched
                .quantum
                .cpu_time_left
                .load(core::sync::atomic::Ordering::Acquire)
                > 0,
            "内核调度支：政策把 quantum 补满（证明用尽被看见）"
        );
        assert!(
            !p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::NO_QUANTUM),
            "内核调度支不置 NO_QUANTUM（C 的 else 支）"
        );
        crate::smp::bkl_unlock();
    }

    /// A missing `proc_ptr` (early-boot window) exits safely — nothing to
    /// settle, not even the baseline is touched.
    #[test]
    fn test_interrupt_stop_no_current_proc_is_noop() {
        setup_c26_globals();
        {
            let section = crate::smp::bkl_lock_section();
            let smp = crate::smp_state_with(&section);
            let local = smp.cpu_local_mut(crate::proc::CpuId::BSP).unwrap();
            local.proc_ptr = None;
            local.tsc_ctr_switch = 1000;
            crate::smp::bkl_unlock();
        }
        let section = crate::smp::bkl_lock_section();
        super::account_interrupt_stop_with(&section, 3000);
        crate::smp::bkl_unlock();
        let section = crate::smp::bkl_lock_section();
        assert_eq!(
            crate::smp_state_with(&section)
                .cpu_local(crate::proc::CpuId::BSP)
                .unwrap()
                .tsc_ctr_switch,
            1000,
            "没有当前进程时连基线都不动（无账可结）"
        );
        crate::smp::bkl_unlock();
    }

    /// Verify the bsp_finish_booting Step 5/7 are no-ops for non-BSP CPUs
    /// (single-CPU build: only BSP exists, AP CPU 1 is the empty default).
    #[test]
    fn test_bsp_finish_booting_single_cpu_only_bsp_initialized() {
        use crate::proc::CpuId;
        use crate::smp::SmpState;
        let smp = SmpState::new_single_cpu();
        // ncpus = 1, so only cpu 0 is initialized.
        assert_eq!(smp.ncpus(), 1);
        assert_eq!(smp.bsp_cpu_id(), CpuId::BSP);
        // AP CPU 1 exists in the array but is the default CpuLocal.
        let ap1 = smp.cpu_local(CpuId::new_unchecked(1)).unwrap();
        assert_eq!(ap1.cpu_last_tsc, 0, "AP CPU should be default-initialized");
        assert!(!ap1.fpu_presence, "AP CPU should have fpu_presence = false");
    }

    // ── ptproc tracking tests (P9-4: SetAddrSpace write_cr3 support) ──

    /// Install a fresh two-CPU global SmpState for ptproc tests (the
    /// accessors are per-CPU CpuLocal reads/writes now — the test drives the
    /// BSP slot, which `current_cpu_id()` names on hosted builds).
    fn fresh_ptproc_state() {
        // SAFETY: single-threaded test (workspace forces --test-threads=1).
        unsafe {
            *crate::globals::SMP_STATE.get() =
                Some(crate::smp::SmpState::with_ncpus(2, crate::proc::CpuId::BSP));
        }
    }

    /// Fresh kernel: `current_ptproc_nr()` returns `None` because
    /// `init_post_and_memory` has not run yet. This matches C behavior
    /// where `ptproc` is uninitialized until `arch_post_init()`.
    #[test]
    #[test]
    fn test_account_process_stop_accumulates_p_cycles() {
        // The helper is the single KERNEL-branch accounting point —
        // both context_stop equivalents route through it; the delta lands
        // in KERNEL.p_cycles (GET_PROC observable) and the CP_INTR bucket.
        fresh_ptproc_state();
        let mut table = crate::test_helpers::test_proc_table();
        let mut smp = crate::smp::SmpState::with_ncpus(1, crate::proc::CpuId::BSP);
        let section = crate::smp::bkl_lock_section();
        super::account_process_stop(
            &mut table,
            &mut smp,
            crate::proc::proc_nr::KERNEL,
            1000,
            &section,
        );
        super::account_process_stop(
            &mut table,
            &mut smp,
            crate::proc::proc_nr::KERNEL,
            250,
            &section,
        );
        crate::smp::bkl_unlock();
        let cycles = table
            .get(crate::proc::proc_nr::KERNEL)
            .unwrap()
            .p_cycles
            .total
            .load(core::sync::atomic::Ordering::Acquire);
        assert_eq!(cycles, 1250, "both deltas accumulate into KERNEL.p_cycles");
        assert_eq!(
            smp.cpu_local(crate::proc::CpuId::BSP)
                .unwrap()
                .tsc_per_state[crate::clock::CP_INTR],
            1250,
            "CP_INTR bucket mirrors the same delta"
        );
    }

    #[test]
    fn test_account_process_stop_zero_delta_is_noop() {
        fresh_ptproc_state();
        let mut table = crate::test_helpers::test_proc_table();
        let mut smp = crate::smp::SmpState::with_ncpus(1, crate::proc::CpuId::BSP);
        let section = crate::smp::bkl_lock_section();
        super::account_process_stop(
            &mut table,
            &mut smp,
            crate::proc::proc_nr::KERNEL,
            0,
            &section,
        );
        crate::smp::bkl_unlock();
        let cycles = table
            .get(crate::proc::proc_nr::KERNEL)
            .unwrap()
            .p_cycles
            .total
            .load(core::sync::atomic::Ordering::Acquire);
        assert_eq!(cycles, 0, "zero delta must not touch the counters");
    }

    #[test]
    fn test_ptproc_unset_returns_none_before_init() {
        fresh_ptproc_state();
        assert_eq!(
            current_ptproc_nr(),
            None,
            "ptproc must be None before init_post_and_memory runs"
        );
    }

    /// After `set_current_ptproc_nr(VM_PROC_NR)`, `current_ptproc_nr()`
    /// returns `Some(VM_PROC_NR)`. This mirrors C's
    /// `get_cpulocal_var(ptproc) = vm` in `arch_post_init()` (per-CPU
    /// CpuLocal slot, BSP's here).
    #[test]
    fn test_ptproc_set_returns_vm_proc_nr() {
        fresh_ptproc_state();
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        assert_eq!(
            current_ptproc_nr(),
            Some(crate::proc::proc_nr::VM_PROC_NR),
            "ptproc must be VM_PROC_NR after init_post_and_memory"
        );
    }

    /// The write lands in THIS CPU's CpuLocal slot only — a second
    /// CPU's slot stays untouched (C: `get_cpulocal_var(ptproc)` is
    /// per-CPU; the global atomic leaked the write across CPUs).
    #[test]
    fn test_ptproc_set_is_per_cpu() {
        fresh_ptproc_state();
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        let smp = unsafe { crate::smp_state_boot_unchecked() };
        let ap_slot = smp.cpu_local(crate::proc::CpuId::new_unchecked(1)).unwrap();
        assert!(
            ap_slot.ptproc.is_none(),
            "AP slot must not see the BSP's ptproc write"
        );
    }

    /// `set_current_ptproc_nr` is idempotent: setting twice to the same
    /// value produces the same observable state. (In normal operation,
    /// ptproc is set only once during boot, but the test guards against
    /// accidental state corruption.)
    #[test]
    fn test_ptproc_set_is_idempotent() {
        fresh_ptproc_state();
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        assert_eq!(current_ptproc_nr(), Some(crate::proc::proc_nr::VM_PROC_NR));
    }

    /// The `SetAddrSpace` handler's ptproc comparison uses `ProcNr` equality.
    /// Verify that `Some(VM_PROC_NR) == Some(VM_PROC_NR)` holds — this is
    /// the branch condition that triggers `TlbArch::set_active_root`.
    #[test]
    fn test_ptproc_comparison_branch_condition() {
        fresh_ptproc_state();
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        // Simulate the SetAddrSpace branch:
        //   if current_ptproc_nr() == Some(target.p_nr) { set_active_root(...) }
        let target_p_nr = crate::proc::proc_nr::VM_PROC_NR;
        let should_reload = current_ptproc_nr() == Some(target_p_nr);
        assert!(
            should_reload,
            "SetAddrSpace on VM (the current ptproc) must trigger set_active_root"
        );
        // A different proc-nr must NOT trigger the reload.
        let other_p_nr = crate::proc::ProcNr(crate::proc::proc_nr::VM_PROC_NR.0 + 1);
        let should_not_reload = current_ptproc_nr() == Some(other_p_nr);
        assert!(
            !should_not_reload,
            "SetAddrSpace on non-ptproc must NOT trigger set_active_root"
        );
    }

    // ── Bootstrap root tracking tests (P9-5: VM ELF loading at boot) ──

    /// Reset `CURRENT_ROOT_PHYS` to the unset sentinel.
    /// Helper for root-phys tests so they don't leak state across each other.
    fn reset_root_phys_for_test() {
        CURRENT_ROOT_PHYS.store(ROOT_PHYS_UNSET, Ordering::Release);
        // `current_root_phys()` now prefers the per-CPU slot,
        // so the reset must clear this CPU's slot too — otherwise a stale
        // value from a prior test would shadow the unset seed.
        let smp = unsafe { crate::try_smp_state() };
        if let Some(smp) = smp
            && let Some(local) = smp.cpu_local_mut(current_cpu_id())
        {
            local.root_phys = None;
        }
    }

    /// Fresh kernel: `current_root_phys()` returns `None` because
    /// `arch_boot_impl` has not yet run (paging not enabled).
    #[test]
    fn test_root_phys_unset_returns_none_before_boot() {
        reset_root_phys_for_test();
        assert_eq!(
            current_root_phys(),
            None,
            "root_phys must be None before arch_boot_impl runs"
        );
    }

    /// After `set_current_root_phys(0x200000)`, `current_root_phys()`
    /// returns `Some(PhysBytes(0x200000))`. This mirrors the boot flow
    /// where `arch_boot_impl` records the root after `enable()` succeeds.
    #[test]
    fn test_root_phys_set_returns_recorded_value() {
        reset_root_phys_for_test();
        set_current_root_phys(minix_types::PhysBytes(0x200000));
        assert_eq!(
            current_root_phys(),
            Some(minix_types::PhysBytes(0x200000)),
            "root_phys must be the value passed to set_current_root_phys"
        );
        reset_root_phys_for_test();
    }

    /// `set_current_root_phys` is idempotent: setting twice produces the
    /// same observable state.
    #[test]
    fn test_root_phys_set_is_idempotent() {
        reset_root_phys_for_test();
        set_current_root_phys(minix_types::PhysBytes(0x300000));
        set_current_root_phys(minix_types::PhysBytes(0x300000));
        assert_eq!(current_root_phys(), Some(minix_types::PhysBytes(0x300000)));
        reset_root_phys_for_test();
    }

    /// Verify the `ROOT_PHYS_UNSET` sentinel is distinct from any valid
    /// 4KB-aligned physical address. `u64::MAX` is not 4KB-aligned and
    /// cannot be a real root physical address.
    #[test]
    fn test_root_phys_sentinel_distinct_from_valid_addresses() {
        assert_eq!(
            ROOT_PHYS_UNSET,
            u64::MAX,
            "ROOT_PHYS_UNSET must be u64::MAX (sentinel value)"
        );
        // u64::MAX is not 4KB-aligned (low 12 bits != 0), so it can
        // never collide with a real page-table root physical address.
        assert_ne!(
            ROOT_PHYS_UNSET & 0xFFF,
            0,
            "ROOT_PHYS_UNSET must not be page-aligned"
        );
        // Common root addresses used in tests/boot must not collide.
        assert_ne!(ROOT_PHYS_UNSET, 0x1000);
        assert_ne!(ROOT_PHYS_UNSET, 0x200000);
    }

    /// Verify that `from_active_root` produces a `Paging` handle whose
    /// `root_paddr()` matches the value passed in. This is the contract
    /// `init_proc_and_boot` relies on when wrapping the bootstrap root
    /// to load the VM ELF.
    #[test]
    fn test_from_active_root_round_trip_root_paddr() {
        use minix_arch::paging::Paging;
        use minix_arch::paging::mock::MockPaging;

        let root_phys = minix_types::PhysBytes(0x10_0000);
        let paging = MockPaging::from_active_root(root_phys, minix_arch::paging::PteChannel::KernelDm);
        assert_eq!(
            paging.root_paddr(),
            root_phys,
            "from_active_root must produce a handle whose root_paddr matches"
        );
    }

    /// Verify `from_active_root` does NOT zero the root (unlike
    /// `new_from_page`). For the mock this is observable via the
    /// `mappings` map being empty in both cases, but the *intent*
    /// difference is documented: `from_active_root` assumes the root
    /// is already initialized. The test asserts the contract by
    /// checking that `from_active_root` returns a usable handle
    /// without calling any allocator.
    #[test]
    fn test_from_active_root_does_not_allocate_via_new_mock_path() {
        use minix_arch::paging::Paging;
        use minix_arch::paging::mock::MockPaging;

        // `from_active_root` should succeed for any root physical
        // address, even one that would be unusual for `new_mock`'s
        // internal counter-based scheme.
        let unusual_root = minix_types::PhysBytes(0xDEAD_BEEF_0000);
        let paging =
            MockPaging::from_active_root(unusual_root, minix_arch::paging::PteChannel::KernelDm);
        assert_eq!(
            paging.root_paddr(),
            unusual_root,
            "from_active_root must preserve the caller-supplied root"
        );
    }

    // ── 10-switch-to-user: scheduler loop tests ─────────────────────────
    //
    // The loop itself is divergent (`-> !`); these tests drive its stages
    // individually (pick/requeue/address-space/idle/finish) with injected
    // state, plus two end-to-end tests that run the real loop until the
    // mock restore diverges (should_panic — the divergence IS the
    // assertion that all five stages completed).
    //
    // Global-state hygiene: BKL/root-mirror/ptproc mirrors are reset at
    // each test's start (tests run with RUST_TEST_THREADS=1 — see
    // .cargo/config.toml — so no parallel interference).

    use crate::kpriv::PrivTable;
    use crate::proc::{CpuId, ProcNr, RtsFlagsBits, proc_nr};
    use crate::proc_table::ProcessTable;
    use crate::smp::SmpState;

    /// Reset the BKL to "held" — simulates the boot/trap-entry contract
    /// (`bsp_finish_booting` step 8.5 acquires it before `switch_to_user`).
    /// The scheduler loop and `idle`/`finish_and_restore` release it at
    /// C's release points, so leaving it cleanly unlocked after a test is
    /// the correct end state.
    fn bkl_acquire_for_test() {
        crate::smp::bkl_lock_reset_for_test();
        crate::smp::bkl_lock().transfer();
    }

    /// Make a user process runnable in `table` at `nr` with the given
    /// priority and remaining quantum, and link a BILLABLE privilege slot.
    fn make_runnable_billable(
        table: &mut ProcessTable,
        priv_table: &mut PrivTable,
        nr: ProcNr,
        prio: u8,
        quantum_cycles: u64,
    ) {
        let p = table.get_mut(nr).expect("process slot must exist");
        p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        p.p_endpoint = minix_types::Endpoint(nr.0); // endpoint >= 0 → quantum-eligible
        p.p_sched.priority.store(prio, Ordering::Release);
        p.p_sched
            .quantum
            .cpu_time_left
            .store(quantum_cycles, Ordering::Release);

        let priv_id = priv_table
            .assign_static(nr)
            .expect("priv slot for user process");
        let kpriv = priv_table.get_mut(priv_id).expect("priv entry");
        kpriv.flags.s_flags = crate::capability::ProcessCapability::USR_F; // BILLABLE | PREEMPTIBLE
        table.get_mut(nr).unwrap().priv_id = Some(priv_id);
    }

    // ── requeue_if_preempted (C: proc.c:322-330) ──

    #[test]
    fn test_requeue_preempted_with_quantum_reenters_queue_head() {
        // C: proc.c:324-327 — PREEMPTED + runnable + cpu_time_left > 0
        // → enqueue_head: a process preempted mid-quantum finishes its
        // slice before equal-priority peers run.
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(0),
            crate::proc::priority::USER_Q,
            1000,
        );
        // A peer already in the queue; the requeued process must land AHEAD of it.
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(1),
            crate::proc::priority::USER_Q,
            1000,
        );
        table.sched_enqueue_with(ProcNr(1), None, CpuId::BSP);
        // Preempt process 0 (rts_set with PREEMPTED also dequeues — it was not queued yet).
        table.rts_set(ProcNr(0), RtsFlagsBits::PREEMPTED);

        super::requeue_if_preempted(&mut table, ProcNr(0));

        let q = crate::proc::priority::USER_Q as usize;
        assert_eq!(
            table.scheduler().queue_head(q),
            Some(ProcNr(0)),
            "preempted process with quantum left must re-enter at the HEAD"
        );
        assert!(
            !table
                .get(ProcNr(0))
                .unwrap()
                .p_rts_flags
                .is_set(RtsFlagsBits::PREEMPTED),
            "PREEMPTED must be cleared by the requeue"
        );
    }

    #[test]
    fn test_requeue_preempted_without_quantum_reenters_queue_tail() {
        // C: proc.c:327-328 — PREEMPTED + runnable + no time left →
        // enqueue (tail): it must yield to peers that still have a slice.
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(0),
            crate::proc::priority::USER_Q,
            0,
        );
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(1),
            crate::proc::priority::USER_Q,
            1000,
        );
        table.sched_enqueue_with(ProcNr(1), None, CpuId::BSP);
        table.rts_set(ProcNr(0), RtsFlagsBits::PREEMPTED);

        super::requeue_if_preempted(&mut table, ProcNr(0));

        let q = crate::proc::priority::USER_Q as usize;
        assert_eq!(
            table.scheduler().queue_head(q),
            Some(ProcNr(1)),
            "peer keeps the head"
        );
        let second = table
            .get(ProcNr(1))
            .unwrap()
            .p_nextready
            .load(Ordering::Relaxed);
        assert_eq!(
            second,
            ProcNr(0).0,
            "exhausted preempted process goes to the TAIL (peer's nextready)"
        );
    }

    #[test]
    fn test_requeue_preempted_unrunnable_not_enqueued() {
        // C: proc.c:324 — the enqueue is guarded by proc_is_runnable; a
        // process blocked again by the preempting work stays off the queue.
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(0),
            crate::proc::priority::USER_Q,
            1000,
        );
        table.rts_set(ProcNr(0), RtsFlagsBits::PREEMPTED);
        table.rts_set(ProcNr(0), RtsFlagsBits::RECEIVING); // blocked again → not runnable

        super::requeue_if_preempted(&mut table, ProcNr(0));

        let q = crate::proc::priority::USER_Q as usize;
        assert_eq!(
            table.scheduler().queue_head(q),
            None,
            "non-runnable preempted process must not be enqueued"
        );
        assert!(
            !table
                .get(ProcNr(0))
                .unwrap()
                .p_rts_flags
                .is_set(RtsFlagsBits::PREEMPTED),
            "PREEMPTED is still cleared"
        );
    }

    #[test]
    fn test_requeue_not_preempted_is_noop() {
        // C: proc.c:322 — the whole block is guarded by proc_is_preempted.
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(0),
            crate::proc::priority::USER_Q,
            1000,
        );
        // Runnable but NOT preempted, and not in any queue.

        super::requeue_if_preempted(&mut table, ProcNr(0));

        let q = crate::proc::priority::USER_Q as usize;
        assert_eq!(
            table.scheduler().queue_head(q),
            None,
            "non-preempted process must not be enqueued by the requeue path"
        );
    }

    // ── pick_and_bill (C: pick_proc — proc.c:1785-1813) ──

    #[test]
    fn test_pick_and_bill_sets_bill_ptr_for_billable_process() {
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut smp = SmpState::new_single_cpu();
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(0),
            crate::proc::priority::USER_Q,
            1000,
        );
        table.sched_enqueue_with(ProcNr(0), None, CpuId::BSP);

        let picked =
            super::pick_and_bill(&mut table, &mut smp, &priv_table, crate::proc::CpuId::BSP);

        assert_eq!(picked, Some(ProcNr(0)));
        let bsp = smp.bsp_cpu_id();
        assert_eq!(
            smp.cpu_local(bsp).unwrap().bill_ptr,
            Some(ProcNr(0)),
            "BILLABLE picked process becomes the bill pointer (C: proc.c:1809)"
        );
    }

    #[test]
    fn test_pick_and_bill_empty_queues_returns_none() {
        let mut table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut smp = SmpState::new_single_cpu();

        assert_eq!(
            super::pick_and_bill(&mut table, &mut smp, &priv_table, crate::proc::CpuId::BSP),
            None,
            "empty queues → None (caller falls into idle)"
        );
    }

    // ── switch_address_space (C: __switch_address_space — klib.S:605-626) ──

    /// Reset the root + ptproc mirrors to the boot-unset state.
    fn reset_root_mirrors_for_test() {
        reset_root_phys_for_test();
        minix_arch::mock_reset_active_root();
        crate::set_current_ptproc_nr(ProcNr(i32::MIN)); // PTPROC_UNSET sentinel
    }

    #[test]
    fn test_switch_address_space_kernel_task_is_noop() {
        // C: klib.S:610-612 — p_cr3 == 0 → return; the kernel mapping
        // stays active and neither the root nor ptproc changes.
        reset_root_mirrors_for_test();
        fresh_ptproc_state();
        let table = crate::test_helpers::test_proc_table();

        super::switch_address_space(&table, proc_nr::KERNEL);

        assert_eq!(
            crate::current_root_phys(),
            None,
            "kernel task (no own root) must not touch the root mirror"
        );
        assert_eq!(
            crate::current_ptproc_nr(),
            None,
            "kernel task must not become ptproc"
        );
    }

    #[test]
    fn test_switch_address_space_installs_root_and_tracks_ptproc() {
        // C: klib.S:621-624 — write the root, then record ptproc.
        reset_root_mirrors_for_test();
        fresh_ptproc_state();
        let mut table = crate::test_helpers::test_proc_table();
        let root = minix_types::PhysBytes(0x5000); // page-aligned, non-zero
        table.get_mut(ProcNr(0)).unwrap().p_seg.phys_root = root;
        table
            .get_mut(ProcNr(0))
            .unwrap()
            .p_rts_flags
            .clear(RtsFlagsBits::SLOT_FREE);

        super::switch_address_space(&table, ProcNr(0));

        assert_eq!(
            crate::current_root_phys(),
            Some(root),
            "the mirror must reflect the installed root (Rust's read_cr3)"
        );
        assert_eq!(
            crate::current_ptproc_nr(),
            Some(ProcNr(0)),
            "the switched-to process becomes ptproc"
        );
    }

    #[test]
    fn test_switch_address_space_same_root_syncs_ptproc_and_mirror() {
        // When the new root equals the live HW register,
        // the switch skips the TLB flush but still syncs mirror and
        // ptproc (defensive consistency — C's asm skips both, but the
        // hardware-read comparison makes the skip safe; updating ptproc
        // is harmless and prevents a stale-pnumr edge).
        reset_root_mirrors_for_test();
        let mut table = crate::test_helpers::test_proc_table();
        let root = minix_types::PhysBytes(0x5000);
        for nr in [ProcNr(0), ProcNr(1)] {
            let p = table.get_mut(nr).unwrap();
            p.p_seg.phys_root = root;
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        super::switch_address_space(&table, ProcNr(0));
        assert_eq!(crate::current_ptproc_nr(), Some(ProcNr(0)));

        super::switch_address_space(&table, ProcNr(1));

        assert_eq!(
            crate::current_root_phys(),
            Some(root),
            "root unchanged (same-root switch syncs mirror)"
        );
        assert_eq!(
            crate::current_ptproc_nr(),
            Some(ProcNr(1)),
            "same-root switch updates ptproc to the newly-dispatched process"
        );
    }

    // ── idle (C: idle — proc.c:175-229) ──

    #[test]
    fn test_idle_marks_cpu_idle_and_bills_idle_proc() {
        // C: proc.c:185-187,192 — proc_ptr = idle_proc, bill_ptr = idle
        // when billable, cpu_is_idle = 1. The kernel-task billable check
        // needs a privilege slot; without one, bill_ptr keeps its old
        // value (C: the BILLABLE test fails and bill_ptr is untouched).
        crate::smp::bkl_lock_reset_for_test();
        bkl_acquire_for_test();
        reset_root_mirrors_for_test();
        let mut table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let mut smp = SmpState::new_single_cpu();
        let bsp = smp.bsp_cpu_id();

        let section = unsafe { crate::smp::BklSection::assume_held() };
        super::idle(
            &section,
            &mut table,
            &mut smp,
            &priv_table,
            crate::proc::CpuId::BSP,
        );

        let local = smp.cpu_local(bsp).unwrap();
        assert_eq!(
            local.proc_ptr,
            Some(proc_nr::IDLE),
            "idle must install the idle process as current (C:185)"
        );
        assert!(local.cpu_is_idle, "idle must set cpu_is_idle (C:192)");
        // The wake path re-acquired the BKL; release it for the next test.
        crate::smp::bkl_unlock();
    }

    // ── finish_and_restore + the full loop (divergence-based) ──

    #[test]
    #[should_panic(expected = "MockTrapReturn::restore_to_user")]
    fn test_finish_and_restore_reaches_mock_restore() {
        // Stage 5 in isolation: with a quantum-bearing picked process and
        // the BKL held (the loop's contract), the final act must be the
        // arch restore — the mock panics, which asserts every preceding
        // step (quantum debug_assert, context_stop, FPU select, flag
        // clear) completed.
        bkl_acquire_for_test();
        reset_root_mirrors_for_test();
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut smp = SmpState::new_single_cpu();
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(0),
            crate::proc::priority::USER_Q,
            5000,
        );
        table.get_mut(ProcNr(0)).unwrap().p_seg.phys_root = minix_types::PhysBytes(0x5000);
        // Simulate a prior kernel entry: the return gate refuses to dispatch
        // on NoEntry (C: arch_system.c:597-598), so the fixture records the
        // full-context style the entry path would stamp.
        table.get_mut(ProcNr(0)).unwrap().trap_style = TrapStyle::IntHard;

        let section = unsafe { crate::smp::BklSection::assume_held() };
        super::finish_and_restore(&mut table, &mut smp, ProcNr(0), false, &section);
    }

    #[test]
    #[should_panic(expected = "no entry trap style known")]
    fn test_finish_and_restore_refuses_dispatch_without_entry_style() {
        // C: arch_system.c:597-598 — restore_user_context panics on a
        // process with no recorded entry style: dispatching would restore
        // an arbitrary register file. A fresh slot has never entered the
        // kernel, so trap_style stays NoEntry and the gate must fire before
        // the mock restore (whose panic is the success signal of
        // test_finish_and_restore_reaches_mock_restore).
        bkl_acquire_for_test();
        reset_root_mirrors_for_test();
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut smp = SmpState::new_single_cpu();
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(0),
            crate::proc::priority::USER_Q,
            5000,
        );
        table.get_mut(ProcNr(0)).unwrap().p_seg.phys_root = minix_types::PhysBytes(0x5000);
        // trap_style intentionally left NoEntry.

        let section = unsafe { crate::smp::BklSection::assume_held() };
        super::finish_and_restore(&mut table, &mut smp, ProcNr(0), false, &section);
    }

    #[test]
    #[should_panic(expected = "fast-syscall return sequence lands with S-8")]
    fn test_finish_and_restore_fast_syscall_style_is_loud_placeholder() {
        // A skinny-frame fast entry pairs a sysret-class return; taking the
        // full-context path with a mismatched frame would be silent
        // corruption. The pairing return lands with the asm entry work —
        // until that lands, the recorded Syscall style must stop loudly,
        // which this pins.
        bkl_acquire_for_test();
        reset_root_mirrors_for_test();
        let mut table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut smp = SmpState::new_single_cpu();
        make_runnable_billable(
            &mut table,
            &mut priv_table,
            ProcNr(0),
            crate::proc::priority::USER_Q,
            5000,
        );
        table.get_mut(ProcNr(0)).unwrap().p_seg.phys_root = minix_types::PhysBytes(0x5000);
        table.get_mut(ProcNr(0)).unwrap().trap_style = TrapStyle::Syscall;

        let section = unsafe { crate::smp::BklSection::assume_held() };
        super::finish_and_restore(&mut table, &mut smp, ProcNr(0), false, &section);
    }

    // ── MF_FLUSH_TLB: the pick-point decision and the switch_to_user consume ──

    /// C: proc.c:345-347 — both halves are required: the flag must be set
    /// and the picked process must be this CPU's ptproc (translations
    /// resident) before a refresh is needed; if the flag is set but ptproc is
    /// another process, the refresh belongs to that CPU's scheduling point.
    #[test]
    fn test_needs_tlb_refresh_requires_flag_and_ptproc() {
        let mut p = crate::proc::KProcess::new(ProcNr(3), minix_types::Endpoint(3));
        p.p_misc_flags.set(crate::proc::MiscFlagsBits::FLUSH_TLB);

        assert!(
            p.needs_tlb_refresh(Some(ProcNr(3))),
            "旗标 + ptproc 匹配 → 刷新"
        );
        assert!(
            !p.needs_tlb_refresh(Some(ProcNr(4))),
            "旗标在但 ptproc 是别的进程 → 本 CPU 不刷"
        );
        assert!(!p.needs_tlb_refresh(None), "无 ptproc 记录 → 不刷");

        p.p_misc_flags.clear(crate::proc::MiscFlagsBits::FLUSH_TLB);
        assert!(!p.needs_tlb_refresh(Some(ProcNr(3))), "无旗标 → 不刷");
    }

    #[test]
    fn test_consume_flush_tlb_flag_flushes_and_clears() {
        // C: proc.c:459-463 — flag set + pick point says resident → flush
        // runs, flag cleared.
        let mut p = crate::proc::KProcess::new(ProcNr(3), minix_types::Endpoint(3));
        p.p_misc_flags.set(crate::proc::MiscFlagsBits::FLUSH_TLB);
        let mut flushed = false;
        super::consume_flush_tlb_flag(&mut p, true, || flushed = true);
        assert!(flushed);
        assert!(!p.p_misc_flags.is_set(crate::proc::MiscFlagsBits::FLUSH_TLB));
    }

    #[test]
    fn test_consume_flush_tlb_flag_clears_without_flush() {
        // Flag set but the translations are not resident on this CPU: skip
        // the flush here (the ptproc CPU flushes at its own scheduling
        // point), but still clear the flag — each CPU consuming it once is
        // part of C's semantics.
        let mut p = crate::proc::KProcess::new(ProcNr(3), minix_types::Endpoint(3));
        p.p_misc_flags.set(crate::proc::MiscFlagsBits::FLUSH_TLB);
        let mut flushed = false;
        super::consume_flush_tlb_flag(&mut p, false, || flushed = true);
        assert!(!flushed);
        assert!(!p.p_misc_flags.is_set(crate::proc::MiscFlagsBits::FLUSH_TLB));
    }

    #[test]
    fn test_consume_flush_tlb_flag_noop_when_unset() {
        // No flag: neither flush nor write the flag (C's `if` is only entered
        // when MF_FLUSH_TLB is set).
        let mut p = crate::proc::KProcess::new(ProcNr(3), minix_types::Endpoint(3));
        let mut flushed = false;
        super::consume_flush_tlb_flag(&mut p, true, || flushed = true);
        assert!(!flushed);
        assert!(!p.p_misc_flags.is_set(crate::proc::MiscFlagsBits::FLUSH_TLB));
    }

    #[test]
    #[should_panic(expected = "MockTrapReturn::restore_to_user")]
    fn test_switch_to_user_full_loop_dispatches_first_runnable_process() {
        // End-to-end: seed proc_ptr = IDLE → stage 1 sees a non-runnable
        // current → requeue (no-op) → pick finds process 0 → address-
        // space install → misc flags (none) → quantum (plenty) → restore
        // diverges. The panic is the success signal; reaching any other
        // outcome (spin, wrong process) would hang or mis-panic instead.
        bkl_acquire_for_test();
        reset_root_mirrors_for_test();

        // Global state the real loop reads: seed the process table with
        // one runnable BILLABLE process.
        // SAFETY: single-threaded test (RUST_TEST_THREADS=1); the global
        // statics are initialized lazily exactly as the boot path does.
        unsafe {
            *SMP_STATE.get() = Some(SmpState::new_single_cpu());
        }
        {
            let table = unsafe { crate::proc_table_boot_unchecked() };
            let priv_table = unsafe { crate::priv_table_boot_unchecked() };
            // Global-state hygiene: the shared scheduler run queue is
            // not part of the root-mirror reset. A prior test that drives a
            // real scheduler-aware primitive against the shared PROC_TABLE
            // (e.g. `clock_irq_handler` waking an expired-alarm owner through
            // the notify enqueue half) legitimately leaves a process
            // queued. Drain every run queue so this loop's pick sees only the
            // single runnable+queued process seeded below (the test's stated
            // precondition), immune to upstream ordering.
            table.drain_run_queues_for_test();
            make_runnable_billable(
                table,
                priv_table,
                ProcNr(0),
                crate::proc::priority::USER_Q,
                5000,
            );
            table.get_mut(ProcNr(0)).unwrap().p_seg.phys_root = minix_types::PhysBytes(0x6000);
            // Simulate a prior kernel entry (see test_finish_and_restore_
            // reaches_mock_restore): the return gate refuses NoEntry.
            table.get_mut(ProcNr(0)).unwrap().trap_style = TrapStyle::IntHard;
            // make_runnable_billable only clears SLOT_FREE; the run queue
            // must be populated explicitly (C's boot path reaches the same
            // state via RTS_UNSET(PROC_STOP) auto-enqueue — proc.h:216-224).
            table.sched_enqueue_with(ProcNr(0), None, CpuId::BSP);
        }

        super::switch_to_user();
    }
}

/// Semantic shutdown entry — C `minix_shutdown(status)` (utility.c; called
/// with 0 at main.c's normal end, hw-2). §3.8 two-layer rule: this is the
/// SEMANTIC layer (banner + backend dispatch); the MECHANISM is the arch
/// backend (`plat::shutdown_qemu` — QEMU test finisher per architecture).
/// The real-hardware backends (ACPI S5 / PSCI SYSTEM_OFF / SBI SRST) plug
/// into the same seam when their lanes land.
///
/// `!`: shutdown does not return (C: NOT_REACHABLE after the backend).
pub fn minix_shutdown(status: u32) -> ! {
    use crate::kmess::{EarlyConsole, KernelConsole as Console};
    Console::write_str("MINIX-RS: shutting down (status ");
    Console::write_hex(status as u64);
    Console::write_str(")\n");
    minix_plat::shutdown_qemu(status)
}

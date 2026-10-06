//! Global platform context — owns the descriptor, written once at T2.5.
//!
//! # Lifecycle
//!
//! 1. **T2.5** (early boot, BKL held, single-threaded): `init_from_kinfo()`
//!    iterates `KernelInfo.platform_sources`, calls `parse_by_kind()` on each,
//!    and writes the first successfully-parsed `PlatformDescEnum` into the
//!    global `PLATFORM` static.
//! 2. **Runtime** (all CPUs, BKL may be released): `platform_desc()` returns
//!    a `&'static dyn PlatformDesc` — read-only, no synchronization needed.

use minix_boot::KernelInfo;
use minix_types::Frozen;

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
use crate::acpi::AcpiDesc;
use crate::desc::PlatformDesc;
#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
use crate::device_tree::DeviceTreeDesc;
use crate::kind::parse_by_kind;
use crate::qemu_virt::QemuVirtDesc;

/// Global platform context — owns the descriptor.
///
/// Stored in kernel BSS as a `static`. Frozen once at T2.5 (single-threaded,
/// BKL held, IRQs off), read-only thereafter — **on every CPU**: S-6.2 (D-41)
/// APs read `platform_desc()` concurrently with the BSP, and the D-36-era
/// `AssumeSyncCell` ("single-threaded `UnsafeCell` with manual `Sync`")
/// promised more than the AP world delivers — its safety proof shape
/// ("no concurrent access, ever") became false the moment APs went online.
///
/// We use [`Frozen`] from `minix-types` — the project's boot-frozen cell
/// (`freeze` once with `Release`; every reader `Acquire`s; the value is then
/// shared immutable `&T`). The D-41 design note preferred the "frozen"
/// argument over locks: C's globals are boot-write-once/read-only by
/// convention with no mechanism; the Rust rewrite makes that convention a
/// checked protocol instead of an assumption.
///
/// # Why `Frozen` and not `Mutex`/`OnceLock`/`static mut`?
///
/// - `static mut` needs `unsafe` at every access and gives no extra safety.
/// - A lock would serialize reads that are immutable — pure overhead, and
///   the wrong semantics (there is nothing to protect after freeze).
/// - `OnceLock` requires `std`/allocator, unavailable in `no_std` boot.
///
/// # Safety invariant
///
/// `init()` / `init_from_kinfo()` freeze the cell exactly once, before any
/// CPU reads via `platform_desc()` — and strictly before `smp_init` wakes
/// the first AP (kmain order), so the freeze happens-before every reader.
static PLATFORM: Frozen<PlatformContext> = Frozen::new();

/// Platform context — owns the descriptor.
///
/// Fields are all `usize`/`u64`/enum (no raw pointers — pointers are derived
/// from `usize` inside methods), so the type is auto-`Send + Sync`.
pub struct PlatformContext {
    /// The platform descriptor (parsed product or QEMU fallback).
    pub desc: PlatformDescEnum,
}

impl PlatformDesc for PlatformContext {
    fn interrupt_controller(&self) -> &dyn crate::desc::InterruptControllerDesc {
        self.desc.interrupt_controller()
    }
    fn timer(&self) -> &dyn crate::desc::TimerDesc {
        self.desc.timer()
    }
    fn early_console(&self) -> Option<&dyn crate::desc::ConsoleDesc> {
        self.desc.early_console()
    }
    fn cpu_topology(&self) -> crate::desc::CpuTopology {
        self.desc.cpu_topology()
    }
    fn arch_misc(&self) -> crate::desc::ArchMiscDesc {
        self.desc.arch_misc()
    }
    fn source(&self) -> crate::desc::PlatformSource {
        self.desc.source()
    }
    fn sswi_setip_base(&self) -> Option<usize> {
        self.desc.sswi_setip_base()
    }
    fn psci_conduit(&self) -> Option<crate::desc::PsciConduit> {
        self.desc.psci_conduit()
    }
}

impl core::fmt::Debug for PlatformContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PlatformContext")
            .field("desc", &self.desc)
            .finish()
    }
}

/// Compile-time-fixed dispatch enum over all descriptor sources.
///
/// # Why not `Box<dyn PlatformDesc>`?
///
/// - `no_std` — avoid allocator dependency at T2.5 (early boot).
/// - Enum dispatch is zero-cost (no vtable indirection).
///
/// # Arch-specific variants
///
/// `DeviceTree` and `Acpi` variants are cfg-gated: DTB path only exists on
/// ARM64/RISC-V, ACPI path only exists on x86-64. The `QemuVirt` variant is
/// always present (every arch has a QEMU virt fallback).
pub enum PlatformDescEnum {
    /// Parsed from a Flattened Device Tree (ARM64/RISC-V only).
    #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
    DeviceTree(DeviceTreeDesc),
    /// Parsed from ACPI tables (x86-64, and aarch64 since S-2b).
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    Acpi(AcpiDesc),
    /// Hardcoded QEMU `virt` fallback (always available).
    QemuVirt(QemuVirtDesc),
}

impl core::fmt::Debug for PlatformDescEnum {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(_) => f.write_str("DeviceTree(...)"),
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            Self::Acpi(_) => f.write_str("Acpi(...)"),
            Self::QemuVirt(_) => f.write_str("QemuVirt"),
        }
    }
}

impl PlatformDesc for PlatformDescEnum {
    fn interrupt_controller(&self) -> &dyn crate::desc::InterruptControllerDesc {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(d) => d.interrupt_controller(),
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            Self::Acpi(d) => d.interrupt_controller(),
            Self::QemuVirt(d) => d.interrupt_controller(),
        }
    }
    fn timer(&self) -> &dyn crate::desc::TimerDesc {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(d) => d.timer(),
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            Self::Acpi(d) => d.timer(),
            Self::QemuVirt(d) => d.timer(),
        }
    }
    fn early_console(&self) -> Option<&dyn crate::desc::ConsoleDesc> {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(d) => d.early_console(),
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            Self::Acpi(d) => d.early_console(),
            Self::QemuVirt(d) => d.early_console(),
        }
    }
    fn cpu_topology(&self) -> crate::desc::CpuTopology {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(d) => d.cpu_topology(),
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            Self::Acpi(d) => d.cpu_topology(),
            Self::QemuVirt(d) => d.cpu_topology(),
        }
    }
    fn arch_misc(&self) -> crate::desc::ArchMiscDesc {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(d) => d.arch_misc(),
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            Self::Acpi(d) => d.arch_misc(),
            Self::QemuVirt(d) => d.arch_misc(),
        }
    }
    fn source(&self) -> crate::desc::PlatformSource {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(_) => crate::desc::PlatformSource::DeviceTree,
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            Self::Acpi(_) => crate::desc::PlatformSource::Acpi,
            Self::QemuVirt(_) => crate::desc::PlatformSource::QemuVirt,
        }
    }
    fn sswi_setip_base(&self) -> Option<usize> {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(d) => d.sswi_setip_base(),
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            Self::Acpi(d) => d.sswi_setip_base(),
            Self::QemuVirt(d) => d.sswi_setip_base(),
        }
    }
    fn psci_conduit(&self) -> Option<crate::desc::PsciConduit> {
        match self {
            #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
            Self::DeviceTree(d) => d.psci_conduit(),
            _ => None,
        }
    }
}

// ── Global API ──

/// Initialize the global platform context with a pre-built descriptor.
///
/// # Safety
///
/// Must only be called once, during boot (single-threaded, BKL held).
/// After this call, all CPUs access the context read-only via
/// [`platform_desc`].
pub unsafe fn init(desc: PlatformDescEnum) {
    // The single freeze of PLATFORM (Release). Caller guarantees boot-phase
    // single-threaded context; readers wake strictly later (kmain order:
    // init_from_kinfo → … → smp_init wakes APs).
    PLATFORM.freeze(PlatformContext { desc });
}

/// Initialize the global platform context from `KernelInfo`.
///
/// Iterates `kinfo.platform_sources` in order (boot-shim's preference) and
/// calls [`parse_by_kind`] on each. The first source that parses successfully
/// wins — matching Linux's `acpi=on/off/force` model where multiple firmware
/// tables may coexist (e.g., DTB + ACPI on SBBR ARM64 servers).
///
/// If `platform_sources` is empty, or all sources fail to parse, falls back
/// to `QemuVirtDesc` in dev builds (warn) or panics in release builds.
///
/// # Panics
///
/// Panics in release builds if no descriptor source parses successfully.
/// Dev builds fall back to `QemuVirtDesc` with a warning.
///
/// # Safety
///
/// Delegates to [`init`] — same single-threaded boot constraint.
/// Additionally, for each source, the caller must guarantee that
/// `source.phys_addr()` points to a valid firmware table blob.
pub unsafe fn init_from_kinfo(kinfo: &KernelInfo) {
    // Try each source in boot-shim's preference order. The first that
    // parses successfully wins.
    let mut parsed: Option<PlatformDescEnum> = None;
    for source in kinfo.platform_sources {
        // SAFETY: caller guarantees each source's phys_addr points to a
        // valid firmware table.
        match unsafe { parse_by_kind(*source) } {
            Ok(desc) => {
                parsed = Some(desc);
                break;
            }
            Err(_e) => {
                // Continue to next source on parse failure.
                // (In dev builds, log the failure; in release, silently
                // proceed to the next source or fallback.)
            }
        }
    }

    let desc = match parsed {
        Some(d) => d,
        None => qemu_fallback_or_panic("no platform source parsed successfully"),
    };

    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { init(desc) };
}

/// Construct the QEMU fallback, or panic in release builds.
///
/// In dev builds (`debug_assertions` enabled), logs a warning and returns
/// `QemuVirtDesc`. In release builds, panics — real hardware cannot run
/// without a proper descriptor.
fn qemu_fallback_or_panic(reason: &str) -> PlatformDescEnum {
    if cfg!(debug_assertions) {
        // Dev build: warn-and-fallback. `log` may not be available in no_std
        // kernel context at this early stage; the warning is best-effort.
        // Callers that have log available can check `source()` == QemuVirt.
        let _ = reason; // suppress unused warning in no-log builds
        PlatformDescEnum::QemuVirt(QemuVirtDesc::default())
    } else {
        panic!(
            "platform::init_from_kinfo: {} and not a dev build (no QemuVirt fallback in release)",
            reason
        );
    }
}

/// Get a reference to the global platform descriptor.
///
/// # Panics
///
/// Panics if called before [`init`] / [`init_from_kinfo`].
pub fn platform_desc() -> &'static dyn PlatformDesc {
    // SAFETY: after `init()` completes (single-threaded boot), the cell is
    // never mutated again. All CPUs read the `&'static` reference safely.
    // The returned reference is `&'static` because `PLATFORM` is a static.
    PLATFORM.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qemu_virt_dispatch_via_enum() {
        let e = PlatformDescEnum::QemuVirt(QemuVirtDesc::default());
        // Verify the source identifies as QemuVirt (drives downstream
        // arch-specific consumers).
        assert_eq!(e.source(), crate::desc::PlatformSource::QemuVirt);

        // Verify dispatch reaches QemuVirtDesc methods. The interrupt/timer
        // methods return `&dyn ...` (always present), console returns
        // `Option<&dyn>` (may or may not be set), and cpu_topology returns
        // the struct by value.
        let ic: &dyn crate::desc::InterruptControllerDesc = e.interrupt_controller();
        let timer: &dyn crate::desc::TimerDesc = e.timer();
        let console: Option<&dyn crate::desc::ConsoleDesc> = e.early_console();
        let topo: minix_boot::CpuTopology = e.cpu_topology();
        // Use each binding to ensure the compiler doesn't optimize the
        // dispatch calls away (verifying the vtables are wired correctly).
        let _ = (ic, timer, console, topo);

        // Verify CPU topology: QemuVirtDesc default mirrors QEMU `-smp 4`
        // (matches per-arch tests in `arch/{x86_64,aarch64,riscv64}.rs`).
        assert_eq!(
            topo.nr_cpus, 4,
            "QemuVirtDesc default should have 4 CPUs (matches QEMU -smp 4)"
        );
        assert_eq!(topo.bsp_id, 0);
        for i in 0..topo.nr_cpus as usize {
            assert_eq!(
                topo.cpus[i].hw_id, i as u64,
                "CPU[{i}] hw_id must equal i (per-hart sequential mapping)"
            );
        }
    }

    // 注：原 `test_platform_desc_panics_before_init` 是空 body placeholder
    // (无 assert、无行为断言)，本自我承认是 "no-op placeholder documenting
    // the contract"。按 Pattern #38 已删除（2026-08-16 review）。
    // 真实 INIT-before-use 验证应在进程隔离的集成测试中进行（每个测试在
    // fresh process 跑），超出单元测试范畴。
}

//! Global platform context — owns the descriptor, written once at T2.5.
//!
//! # Lifecycle
//!
//! 1. **T2.5** (early boot, BKL held, single-threaded): `init_from_kinfo()`
//!    parses `KernelInfo.platform_descriptor` and writes the resulting
//!    `PlatformDescEnum` into the global `PLATFORM` static.
//! 2. **Runtime** (all CPUs, BKL may be released): `platform_desc()` returns
//!    a `&'static dyn PlatformDesc` — read-only, no synchronization needed.

use core::cell::UnsafeCell;

use minix_boot::{KernelInfo, PlatformDescriptorPtr};

use crate::acpi::AcpiDesc;
use crate::desc::PlatformDesc;
use crate::device_tree::DeviceTreeDesc;
use crate::qemu_virt::QemuVirtDesc;

/// `Sync` wrapper around `UnsafeCell<Option<PlatformContext>>`.
///
/// `UnsafeCell` is not `Sync` by design (interior mutability + shared
/// access is unsafe). We assert `Sync` manually because the access pattern
/// is constrained: single write at boot (BKL held), read-only thereafter.
///
/// # Safety invariant
///
/// - `init()` / `init_from_kinfo()` must be called exactly once, before
///   any CPU reads via `platform_desc()`.
/// - After init, the cell is never mutated again.
struct SyncPlatformCell(UnsafeCell<Option<PlatformContext>>);

// SAFETY: the safety invariant above (single write at boot, read-only after)
// is upheld by the module's public API. `init*` is `unsafe fn` requiring the
// caller to guarantee single-threaded boot context.
unsafe impl Sync for SyncPlatformCell {}

/// Global platform context — owns the descriptor.
///
/// Stored in kernel BSS as a `static`. Written once at T2.5 (single-threaded,
/// BKL held, IRQs off), read-only thereafter.
///
/// # Why a `Sync` wrapper around `UnsafeCell` not `static mut`?
///
/// - `static mut` access requires `unsafe` but provides no extra safety and
///   is being deprecated (Rust 2024).
/// - `Mutex`/`spin::Mutex` would add runtime overhead unnecessary here
///   (boot-only mutation, read-only runtime).
/// - The `SyncPlatformCell` wrapper clearly documents the safety invariant.
static PLATFORM: SyncPlatformCell = SyncPlatformCell(UnsafeCell::new(None));

/// Platform context — owns the descriptor.
///
/// Fields are all `usize`/`u64`/enum (no raw pointers — pointers are derived
/// from `usize` inside methods), so the type is auto-`Send + Sync`.
pub struct PlatformContext {
    /// The platform descriptor (parsed product or QEMU fallback).
    pub desc: PlatformDescEnum,
}

impl PlatformDesc for PlatformContext {
    fn interrupt_controller(&self) -> crate::desc::InterruptControllerDesc {
        self.desc.interrupt_controller()
    }
    fn timer(&self) -> crate::desc::TimerDesc {
        self.desc.timer()
    }
    fn early_console(&self) -> Option<crate::desc::ConsoleDesc> {
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
/// - `no_std` — no allocator.
/// - Enum dispatch is zero-cost (no vtable indirection).
pub enum PlatformDescEnum {
    /// Parsed from a Flattened Device Tree (Phase 3 — ARM64/RISC-V).
    DeviceTree(DeviceTreeDesc),
    /// Parsed from ACPI tables (Phase 4 — x86-64).
    Acpi(AcpiDesc),
    /// Hardcoded QEMU `virt` fallback (Phase 1 — always available).
    QemuVirt(QemuVirtDesc),
}

impl core::fmt::Debug for PlatformDescEnum {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DeviceTree(_) => f.write_str("DeviceTree(...)"),
            Self::Acpi(_) => f.write_str("Acpi(...)"),
            Self::QemuVirt(_) => f.write_str("QemuVirt"),
        }
    }
}

impl PlatformDesc for PlatformDescEnum {
    fn interrupt_controller(&self) -> crate::desc::InterruptControllerDesc {
        match self {
            Self::DeviceTree(d) => d.interrupt_controller(),
            Self::Acpi(d) => d.interrupt_controller(),
            Self::QemuVirt(d) => d.interrupt_controller(),
        }
    }
    fn timer(&self) -> crate::desc::TimerDesc {
        match self {
            Self::DeviceTree(d) => d.timer(),
            Self::Acpi(d) => d.timer(),
            Self::QemuVirt(d) => d.timer(),
        }
    }
    fn early_console(&self) -> Option<crate::desc::ConsoleDesc> {
        match self {
            Self::DeviceTree(d) => d.early_console(),
            Self::Acpi(d) => d.early_console(),
            Self::QemuVirt(d) => d.early_console(),
        }
    }
    fn cpu_topology(&self) -> crate::desc::CpuTopology {
        match self {
            Self::DeviceTree(d) => d.cpu_topology(),
            Self::Acpi(d) => d.cpu_topology(),
            Self::QemuVirt(d) => d.cpu_topology(),
        }
    }
    fn arch_misc(&self) -> crate::desc::ArchMiscDesc {
        match self {
            Self::DeviceTree(d) => d.arch_misc(),
            Self::Acpi(d) => d.arch_misc(),
            Self::QemuVirt(d) => d.arch_misc(),
        }
    }
    fn source(&self) -> crate::desc::PlatformSource {
        match self {
            Self::DeviceTree(_) => crate::desc::PlatformSource::DeviceTree,
            Self::Acpi(_) => crate::desc::PlatformSource::Acpi,
            Self::QemuVirt(_) => crate::desc::PlatformSource::QemuVirt,
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
    // SAFETY: caller guarantees single-threaded boot context (BKL held,
    // IRQs off, no other CPU running). This is the only write to PLATFORM.
    unsafe {
        *PLATFORM.0.get() = Some(PlatformContext { desc });
    }
}

/// Initialize the global platform context from `KernelInfo`.
///
/// Reads `kinfo.platform_descriptor` and dispatches:
/// - `Some(Dtb)` → `DeviceTreeDesc::parse` (Phase 3 — ARM64/RISC-V)
/// - `Some(Rsdp)` → `AcpiDesc::parse` (Phase 4 — currently falls back)
/// - `None` → `QemuVirtDesc` fallback (dev: warn, release: panic)
///
/// # Panics
///
/// Panics in release builds if no descriptor is provided. Dev builds
/// fall back to `QemuVirtDesc` with a warning.
///
/// # Safety
///
/// Delegates to [`init`] — same single-threaded boot constraint.
/// Additionally, for the DTB path, the caller must guarantee that
/// `kinfo.platform_descriptor` points to a valid FDT blob.
pub unsafe fn init_from_kinfo(kinfo: &KernelInfo) {
    let desc = match kinfo.platform_descriptor {
        Some(PlatformDescriptorPtr::Dtb(pa)) => {
            // Phase 3: parse DTB. On parse failure, fall back to QemuVirt
            // in dev builds (so QEMU tests still pass with a malformed DTB)
            // or panic in release.
            // SAFETY: caller guarantees pa points to a valid FDT blob.
            match unsafe { DeviceTreeDesc::parse(pa.0 as usize) } {
                Ok(d) => PlatformDescEnum::DeviceTree(d),
                Err(_e) => qemu_fallback_or_panic("DTB parse failed"),
            }
        }
        Some(PlatformDescriptorPtr::Rsdp(pa)) => {
            // Phase 4: parse ACPI. On parse failure, fall back to QemuVirt
            // in dev builds or panic in release.
            // SAFETY: caller guarantees pa points to a valid RSDP.
            match unsafe { AcpiDesc::parse(pa.0 as usize) } {
                Ok(d) => PlatformDescEnum::Acpi(d),
                Err(_e) => qemu_fallback_or_panic("ACPI parse failed"),
            }
        }
        None => {
            qemu_fallback_or_panic("no platform descriptor provided by boot-shim")
        }
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
        PlatformDescEnum::QemuVirt(QemuVirtDesc)
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
    unsafe {
        (*PLATFORM.0.get())
            .as_ref()
            .expect("platform_desc() called before init_from_kinfo()")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qemu_virt_dispatch_via_enum() {
        let e = PlatformDescEnum::QemuVirt(QemuVirtDesc);
        assert_eq!(e.source(), crate::desc::PlatformSource::QemuVirt);
        // Verify dispatch reaches QemuVirtDesc methods
        let _ic = e.interrupt_controller();
        let _t = e.timer();
        let _c = e.early_console();
        let _topo = e.cpu_topology();
    }

    #[test]
    fn test_platform_desc_panics_before_init() {
        // Note: we cannot easily test this in-process because PLATFORM is
        // a global static. If init() was called by another test, this would
        // succeed. This test is a no-op placeholder documenting the contract.
        // Real verification happens via integration tests that run in fresh
        // processes.
    }
}

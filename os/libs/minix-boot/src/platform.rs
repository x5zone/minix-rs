//! Platform descriptor abstraction — the boot-handoff protocol layer.
//!
//! This module defines the **mechanism** (what platform descriptors can do),
//! not the **hardware** (what they are called). Brand names like `Apic`,
//! `Gicv3`, `Plic`, `DeviceTree`, `Acpi` never appear here — they live in
//! `minix-platform`'s `arch/` submodules and are invisible to upper layers.
//!
//! # Design (TODO-01-2 fix, 2026-07-16)
//!
//! Three layers:
//!
//! | Layer | Crate | Responsibility |
//! |-------|-------|----------------|
//! | Handoff | `minix-boot` (this file) | `PlatformDescKind` opaque tag + `PlatformDescSource` handle + sub-trait interfaces |
//! | Parsing | `minix-platform::kind` | Kind constants (DTB/RSDP) + `parse_by_kind()` dispatch |
//! | Implementation | `minix-platform::arch/` | Concrete structs (`ApicDesc`, `Gicv3Desc`, ...) implementing sub-traits |
//!
//! # Why an opaque tag instead of `fn(...)` pointer?
//!
//! A function pointer (`fn(PhysBytes) -> Result<...>`) would couple
//! `minix-boot` to `minix-platform` at the binary level — the pointer's
//! address is resolved in the boot-shim binary, but the function body lives
//! in `minix-platform`. Once TODO-02-3 makes the kernel an independent ELF,
//! the boot-shim's memory is reclaimed and the function pointer becomes a
//! dangling reference. An opaque `u32` tag + physical address is pure data
//! — safe to pass across binaries.
//!
//! # Why trait + `Any` downcast for sub-descriptors?
//!
//! Sub-descriptors (`InterruptControllerDesc`, `TimerDesc`, ...) carry
//! architecture-specific fields (`lapic_base` + `ioapic_base` for x86,
//! `gicd_base` + `gicr_base` for ARM, etc.). A trait with `Any` support
//! lets the upper layer call generic methods (`nr_irqs()`) while the arch
//! layer downcasts to the concrete type for architecture-specific fields —
//! zero vtable overhead on the arch hot path.

use core::any::Any;
use core::fmt;
use minix_types::PhysBytes;

// ── Opaque kind tag ──

/// Opaque platform descriptor kind tag.
///
/// A plain `u32` wrapper — the upper layer sees an opaque integer, not the
/// firmware table format name. `minix-platform::kind` defines the constants
/// (`DTB`, `RSDP`, ...) and the `parse_by_kind()` dispatch function.
///
/// # Cross-binary safety
///
/// `PlatformDescKind` is `Copy` and contains only a `u32` — safe to embed
/// in `KernelInfo` and pass from boot-shim to kernel even when they are
/// separate ELF binaries (TODO-02-3).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlatformDescKind(u32);

impl PlatformDescKind {
    /// Construct a kind tag. Only `minix-platform::kind` should call this.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
    /// Raw value (for diagnostics / `Display`).
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Debug for PlatformDescKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PlatformDescKind({})", self.0)
    }
}

// ── Known kind constants (boot handoff protocol) ──
//
// These constants are the **protocol contract** between boot-shim (which
// assigns tags when constructing `PlatformDescSource`) and `minix-platform::
// kind::parse_by_kind()` (which dispatches on tags to the correct parser).
//
// They live here — in the handoff layer — rather than in `minix-platform`,
// so that boot-shim can construct sources without depending on
// `minix-platform` (which pulls in arch-specific parsing code). The kernel
// never names these constants directly: it calls `parse_by_kind()` which
// dispatches internally.
//
// # Adding a new firmware table format
//
// 1. Add a new `pub const` here (e.g. `pub const SMBIOS: PlatformDescKind = ...`).
// 2. Add a match arm in `minix_platform::kind::parse_by_kind`.
// 3. Implement the parser in `minix_platform`.

/// DTB (Flattened Device Tree) kind tag.
///
/// Used by ARM64 and RISC-V when firmware passes a DTB blob. Boot-shim
/// constructs `PlatformDescSource::new(DTB, PhysBytes(dtb_pa))` and appends
/// it to `KernelInfo.platform_sources`.
pub const DTB: PlatformDescKind = PlatformDescKind::new(1);

/// RSDP (ACPI Root System Description Pointer) kind tag.
///
/// Used by x86-64 (and ARM64 SBBR servers that prefer ACPI). Boot-shim
/// constructs `PlatformDescSource::new(RSDP, PhysBytes(rsdp_pa))` and
/// appends it to `KernelInfo.platform_sources`.
pub const RSDP: PlatformDescKind = PlatformDescKind::new(2);

// ── Platform descriptor source handle ──

/// Platform descriptor source — an opaque handle carried in `KernelInfo`.
///
/// The upper layer sees only `(kind, phys_addr)` — it does not know whether
/// `kind` is DTB, RSDP, or something else. `minix-platform::parse_by_kind()`
/// dispatches on `kind` to the appropriate parser.
///
/// # Multiple sources (DTB + RSDP coexistence)
///
/// Real-world ARM64 servers (SBBR) may provide both DTB and ACPI tables.
/// Linux resolves this by preferring one (configurable via `acpi=on/off/force`)
/// and using only that one for device enumeration. minix-rs follows the same
/// model: `KernelInfo.platform_sources` is a list ordered by boot-shim's
/// preference; the kernel takes the first source that parses successfully.
#[derive(Clone, Copy)]
pub struct PlatformDescSource {
    kind: PlatformDescKind,
    phys_addr: PhysBytes,
}

impl PlatformDescSource {
    /// Construct a source handle. Called by boot-shim using constants from
    /// `minix_platform::kind`.
    pub const fn new(kind: PlatformDescKind, phys_addr: PhysBytes) -> Self {
        Self { kind, phys_addr }
    }
    /// Opaque kind tag (for `minix-platform::parse_by_kind()` dispatch).
    pub fn kind(&self) -> PlatformDescKind {
        self.kind
    }
    /// Physical address of the firmware table blob.
    pub fn phys_addr(&self) -> PhysBytes {
        self.phys_addr
    }
}

impl fmt::Debug for PlatformDescSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlatformDescSource")
            .field("kind", &self.kind)
            .field("phys_addr", &self.phys_addr)
            .finish()
    }
}

// ── Root platform descriptor trait ──

/// Where the platform description came from (for diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSource {
    /// Parsed from a Flattened Device Tree blob.
    DeviceTree,
    /// Parsed from ACPI tables (RSDP → XSDT → MADT).
    Acpi,
    /// Hardcoded QEMU `virt` fallback.
    QemuVirt,
}

/// Maximum number of CPUs supported by the platform descriptor.
///
/// Used to size the fixed `CpuTopology.cpus` array (`no_std` — no heap).
pub const MAX_CPUS: usize = 64;

/// CPU topology (SMP-ready; `QemuVirtDesc` pre-populates 4 cores for test).
#[derive(Debug, Clone, Copy)]
pub struct CpuTopology {
    /// Total CPU/hart count.
    pub nr_cpus: u32,
    /// Hardware ID of the BSP (boot CPU).
    pub bsp_id: u32,
    /// Per-CPU info. Only `cpus[0..nr_cpus]` is valid.
    pub cpus: [CpuInfo; MAX_CPUS],
}

impl Default for CpuTopology {
    fn default() -> Self {
        Self {
            nr_cpus: 1,
            bsp_id: 0,
            cpus: [CpuInfo::default(); MAX_CPUS],
        }
    }
}

/// Per-CPU descriptor entry.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuInfo {
    /// Architecture-specific hardware ID:
    /// - x86-64: APIC ID
    /// - ARM64: MPIDR_EL1.Aff0
    /// - RISC-V: hart ID
    pub hw_id: u64,
    /// ARM64 GICv3: this CPU's Redistributor base
    /// (`gicr_base + hw_id * gicr_stride`).
    pub gicr_base: Option<usize>,
    /// RISC-V CLINT: this hart's mtimecmp address
    /// (`mtimecmp_base + hart_id * mtimecmp_stride`).
    pub mtimecmp_addr: Option<usize>,
}

impl CpuInfo {
    /// Const constructor for use in `const fn` contexts (e.g. `QemuVirtDesc::new`).
    ///
    /// `#[derive(Default)]` generates a non-const `default()`, so `const fn`s
    /// cannot call `CpuInfo::default()`. This method provides the same value
    /// as a `const fn`.
    pub const fn zero() -> Self {
        Self {
            hw_id: 0,
            gicr_base: None,
            mtimecmp_addr: None,
        }
    }
}

/// Architecture miscellany consumed by `ArchInit`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArchMiscDesc {
    /// x86-64: ACPI tables physical address (if not via RSDP path).
    pub acpi_tables: Option<usize>,
    /// ARM64: enable PMU cycle counter.
    pub pmu_cycle_counter: bool,
}

/// Root platform descriptor trait — the unified abstraction.
///
/// One `PlatformDesc` instance answers: "what hardware am I running on?".
/// Upper layers (`ClockArch` / `InterruptController` / `ArchInit`) read
/// only this abstraction — they never touch raw FDT/ACPI bytes or brand names.
///
/// # Implementors
///
/// - `DeviceTreeDesc` (minix-platform): parsed from FDT (ARM64 / RISC-V).
/// - `AcpiDesc` (minix-platform): parsed from ACPI RSDP (x86-64).
/// - `QemuVirtDesc` (minix-platform): hardcoded QEMU `virt` fallback.
///
/// # Concurrency
///
/// All implementors must be `Send + Sync`: the descriptor is written once
/// at T2.5 (BKL held, single-threaded) and read by all CPUs thereafter.
pub trait PlatformDesc: Send + Sync + fmt::Debug {
    /// Interrupt controller descriptor (returns `&dyn` — see trait docs below).
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc;
    /// Timer descriptor.
    fn timer(&self) -> &dyn TimerDesc;
    /// Early console descriptor (optional — used before proper console init).
    fn early_console(&self) -> Option<&dyn ConsoleDesc>;
    /// CPU topology (core count, per-CPU info — SMP-ready, single-core for now).
    fn cpu_topology(&self) -> CpuTopology;
    /// Architecture miscellany (ACPI table pointer, PMU enable, ...).
    fn arch_misc(&self) -> ArchMiscDesc;
    /// Source identifier (for debugging / logging).
    fn source(&self) -> PlatformSource;
}

// ── Sub-descriptor traits (brand names hidden in arch/ submodules) ──
//
// Each sub-descriptor is a trait with `Any` support. The upper layer calls
// generic methods; the arch layer downcasts to the concrete type for
// architecture-specific fields. Brand names (`Apic`, `Gicv3`, `Plic`, ...)
// appear only in `minix-platform/src/arch/<arch>/` files.

/// Interrupt controller descriptor — describes the mechanism, not the brand.
///
/// Generic fields (like `nr_irqs`) are trait methods. Architecture-specific
/// fields (`lapic_base` + `ioapic_base` for x86, `gicd_base` + `gicr_base`
/// for ARM, `plic_base` + `context` for RISC-V) are accessed via `Any`
/// downcast in the arch layer.
///
/// # Implementors (all in `minix-platform/src/arch/`)
///
/// - `ApicDesc` (x86-64): LAPIC + IOAPIC
/// - `Gicv3Desc` (ARM64): GICv3 distributor + redistributor
/// - `PlicDesc` (RISC-V): PLIC
pub trait InterruptControllerDesc: Send + Sync + fmt::Debug + Any {
    /// Number of IRQ sources supported.
    fn nr_irqs(&self) -> u32;
    /// `Any` support for arch-layer downcast.
    ///
    /// Implementors write `fn as_any(&self) -> &dyn Any { self }` — this
    /// works because in the impl block `Self` is a concrete sized type.
    /// The default impl is omitted because `&Self` → `&dyn Any` requires
    /// `Self: Sized`, which is not guaranteed inside the trait definition.
    fn as_any(&self) -> &dyn Any;
}

/// Timer descriptor — describes the mechanism, not the brand.
///
/// # Implementors (all in `minix-platform/src/arch/`)
///
/// - `PitDesc` (x86-64): 8254 PIT + LAPIC Timer
/// - `ArmGenericTimerDesc` (ARM64): ARM Generic Timer
/// - `ClintDesc` (RISC-V): CLINT mtime
pub trait TimerDesc: Send + Sync + fmt::Debug + Any {
    /// Timer frequency in Hz (0 if read from hardware register at runtime).
    fn frequency(&self) -> u64;
    /// `Any` support for arch-layer downcast (see `InterruptControllerDesc`).
    fn as_any(&self) -> &dyn Any;
}

/// Early console descriptor — describes the mechanism, not the brand.
///
/// # Implementors (all in `minix-platform/src/arch/`)
///
/// - `IsaSerialDesc` (x86-64): ISA serial port I/O
/// - `MmioSerialDesc` (ARM64): MMIO UART (PL011)
/// - `SbiConsoleDesc` (RISC-V): SBI ecall console
pub trait ConsoleDesc: Send + Sync + fmt::Debug + Any {
    /// `Any` support for arch-layer downcast (see `InterruptControllerDesc`).
    fn as_any(&self) -> &dyn Any;
}

// ── Errors ──

/// Error returned when parsing a platform descriptor source fails.
#[derive(Debug, Clone, Copy)]
pub enum PlatformParseError {
    /// The kind tag is not recognized by any registered parser.
    UnknownKind(u32),
    /// DTB parsing failed (see `minix_platform::DtParseError` for details).
    DtbParse,
    /// ACPI parsing failed (see `minix_platform::AcpiParseError` for details).
    AcpiParse,
}

impl fmt::Display for PlatformParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKind(k) => write!(f, "unknown platform descriptor kind: {}", k),
            Self::DtbParse => write!(f, "DTB parse failed"),
            Self::AcpiParse => write!(f, "ACPI parse failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_platform_desc_kind_new_and_raw() {
        let k = PlatformDescKind::new(42);
        assert_eq!(k.raw(), 42);
    }

    #[test]
    fn test_platform_desc_kind_eq() {
        let a = PlatformDescKind::new(1);
        let b = PlatformDescKind::new(1);
        let c = PlatformDescKind::new(2);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_platform_desc_source_new() {
        let kind = PlatformDescKind::new(1);
        let src = PlatformDescSource::new(kind, PhysBytes(0x4000_0000));
        assert_eq!(src.kind(), kind);
        assert_eq!(src.phys_addr(), PhysBytes(0x4000_0000));
    }

    #[test]
    fn test_platform_desc_source_copy() {
        let kind = PlatformDescKind::new(1);
        let src = PlatformDescSource::new(kind, PhysBytes(0x4000_0000));
        let src_copy = src; // Copy
        assert_eq!(src.kind(), src_copy.kind());
        assert_eq!(src.phys_addr(), src_copy.phys_addr());
    }

    #[test]
    fn test_cpu_topology_default_single_core() {
        let t = CpuTopology::default();
        assert_eq!(t.nr_cpus, 1);
        assert_eq!(t.bsp_id, 0);
        assert_eq!(t.cpus.len(), MAX_CPUS);
    }

    #[test]
    fn test_cpu_info_default() {
        let c = CpuInfo::default();
        assert_eq!(c.hw_id, 0);
        assert_eq!(c.gicr_base, None);
        assert_eq!(c.mtimecmp_addr, None);
    }

    #[test]
    fn test_platform_source_variants() {
        assert_ne!(PlatformSource::DeviceTree, PlatformSource::Acpi);
        assert_ne!(PlatformSource::Acpi, PlatformSource::QemuVirt);
        assert_ne!(PlatformSource::DeviceTree, PlatformSource::QemuVirt);
    }

    #[test]
    fn test_platform_parse_error_display() {
        assert!(format!("{}", PlatformParseError::UnknownKind(99)).contains("99"));
        assert!(format!("{}", PlatformParseError::DtbParse).contains("DTB"));
        assert!(format!("{}", PlatformParseError::AcpiParse).contains("ACPI"));
    }
}

//! Minimal ACPI parser — extracts LAPIC/IOAPIC base addresses from MADT.
//!
//! Used by x86-64 when the boot-shim provides an RSDP pointer via
//! `KernelInfo.platform_sources`. Implements only the tables needed for
//! platform hardware discovery (RSDP, XSDT/RSDT, MADT). Full ACPI (power
//! management, AML, etc.) is out of scope — use the `acpi` crate for that.
//!
//! # ACPI table chain
//!
//! ```text
//! RSDP (Root System Description Pointer)
//!  ├── rsdt_addr → RSDT (32-bit)  ─┐
//!  └── xsdt_addr → XSDT (64-bit)  ─┤ (one of these)
//!                                    ↓
//!                                  XSDT/RSDT (array of table pointers)
//!                                    ├── "APIC" → MADT (Multiple APIC Description Table)
//!                                    ├── "HPET" → (skipped)
//!                                    └── ... (other tables skipped)
//! ```
//!
//! # MADT (APIC) table
//!
//! The MADT contains a header followed by a sequence of interrupt controller
//! structure records. We extract:
//! - **LAPIC base**: from the MADT header's `Local APIC Address` field.
//! - **IOAPIC base**: from the first `IOAPIC` structure record.
//! - **CPU topology**: from `LAPIC` structure records (one per CPU).
//!
//! # `no_std` constraints
//!
//! - No heap: all parsing is done via raw pointer reads with bounds checks.
//! - The parsed `AcpiDesc` owns only `usize`/`u32` values — no borrowed data.
//!
//! # Safety
//!
//! `parse` is `unsafe` because it dereferences raw physical pointers. The
//! caller (boot path) must guarantee:
//! - `rsdp_phys` points to a valid RSDP (signature "RSD PTR ").
//! - All referenced tables (XSDT, MADT) remain valid for the duration of `parse`.
//! - No other CPU is concurrently writing to the ACPI table memory.

#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::{ApicDesc, IsaSerialDesc, PitDesc};
#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::{ArmGenericTimerDesc, Gicv3Desc};
use crate::desc::*;
use core::fmt;

// ── ACPI constants ──

/// RSDP signature: "RSD PTR " (note trailing space).
const RSDP_SIGNATURE: [u8; 8] = *b"RSD PTR ";

/// RSDP revision 2.0+ (has XSDT pointer).
const RSDP_REV_2: u8 = 2;

/// MADT table signature: "APIC".
const MADT_SIGNATURE: [u8; 4] = *b"APIC";

/// IOAPIC structure type in MADT.
const MADT_TYPE_IOAPIC: u8 = 1;

/// GICC (Generic Interrupt Controller CPU interface) structure type — aarch64.
/// ACPI 6.x §5.2.12.5: 80-byte record, MPIDR at offset 56, GICR base at 48,
/// Flags at offset 12 (bit 0 = Processor Enabled).
const MADT_TYPE_GICC: u8 = 11;

/// GIC Distributor structure type — aarch64. ACPI 6.x §5.2.12.14 (24 bytes,
/// distributor base at offset 12).
const MADT_TYPE_GICD: u8 = 12;

/// LAPIC (processor local APIC) structure type in MADT.
const MADT_TYPE_LAPIC: u8 = 0;

/// LAPIC x2APIC structure type in MADT.
const MADT_TYPE_X2APIC: u8 = 9;

// ── ACPI table structures (packed, big-endian agnostic) ──
//
// We only use `size_of::<T>()` for these structs — all field access is via
// byte slices to avoid unaligned reads on packed fields. The structs are
// kept as documentation of the ACPI table layouts.

/// Common header for all ACPI tables (ACPI 6.4 §5.2.6).
#[repr(C, packed)]
struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: [u8; 4],
    creator_revision: u32,
}

/// MADT header (follows the common SDT header).
///
/// Per ACPI 6.4 §5.2.12.
#[repr(C, packed)]
struct MadtHeader {
    local_apic_addr: u32,
    flags: u32,
}

/// MADT interrupt controller structure header (common to all record types).
#[repr(C, packed)]
struct MadtEntryHeader {
    entry_type: u8,
    length: u8,
}

// ── Parsed ACPI descriptor ──

/// Parsed ACPI descriptor — owns all extracted hardware parameters.
///
/// Constructed via [`AcpiDesc::parse`] (production). After construction, the
/// ACPI tables are no longer needed — all values are stored as concrete x86-64
/// structs (`ApicDesc`, `PitDesc`, `IsaSerialDesc`) implementing the
/// sub-descriptor traits.
#[derive(Clone, Copy)]
pub struct AcpiDesc {
    #[cfg(target_arch = "x86_64")]
    ic: ApicDesc,
    #[cfg(target_arch = "aarch64")]
    ic: Gicv3Desc,
    #[cfg(target_arch = "x86_64")]
    timer: PitDesc,
    #[cfg(target_arch = "aarch64")]
    timer: ArmGenericTimerDesc,
    #[cfg(target_arch = "x86_64")]
    console: Option<IsaSerialDesc>,
    cpu_topology: CpuTopology,
    arch_misc: ArchMiscDesc,
}

impl fmt::Debug for AcpiDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dbg = f.debug_struct("AcpiDesc");
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        {
            dbg.field("ic", &self.ic).field("timer", &self.timer);
        }
        #[cfg(target_arch = "x86_64")]
        {
            dbg.field("console", &self.console);
        }
        dbg.field("cpu_topology", &self.cpu_topology).finish_non_exhaustive()
    }
}

impl AcpiDesc {
    /// Parse ACPI tables starting from the RSDP physical address.
    ///
    /// # Safety
    ///
    /// Caller must guarantee `rsdp_phys` points to a valid RSDP and all
    /// referenced tables remain readable for the duration of this call.
    pub unsafe fn parse(rsdp_phys: usize) -> Result<Self, AcpiParseError> {
        // SAFETY: caller guarantees rsdp_phys is a valid RSDP pointer.
        // Read fields via byte slices to avoid unaligned access on packed struct.
        let sig_bytes = unsafe { core::slice::from_raw_parts(rsdp_phys as *const u8, 8) };
        if sig_bytes != &RSDP_SIGNATURE[..] {
            return Err(AcpiParseError::BadRsdpSignature);
        }

        // RSDP layout (ACPI 6.4 §5.2.5):
        //   0: signature (8)
        //   8: checksum (1)
        //   9: oem_id (6)
        //  15: revision (1)
        //  16: rsdt_addr (4)
        //  20: length (4)        [rev 2+]
        //  24: xsdt_addr (8)     [rev 2+]
        //  32: extended_checksum  [rev 2+]
        //  33: reserved (3)       [rev 2+]
        let revision = unsafe { *((rsdp_phys + 15) as *const u8) };

        // Prefer XSDT (64-bit) on revision 2.0+; fall back to RSDT.
        let xsdt_phys = if revision >= RSDP_REV_2 {
            let xsdt_bytes = unsafe {
                core::slice::from_raw_parts((rsdp_phys + 24) as *const u8, 8)
            };
            let mut arr = [0u8; 8];
            arr.copy_from_slice(xsdt_bytes);
            u64::from_le_bytes(arr) as usize
        } else {
            let rsdt_bytes = unsafe {
                core::slice::from_raw_parts((rsdp_phys + 16) as *const u8, 4)
            };
            u32_le_from_slice(rsdt_bytes) as usize
        };

        if xsdt_phys == 0 {
            return Err(AcpiParseError::NoXsdtPointer);
        }

        // SAFETY: caller guarantees ACPI tables are valid.
        let madt_phys = unsafe { find_madt(xsdt_phys, revision >= RSDP_REV_2) }
            .ok_or(AcpiParseError::MadtNotFound)?;

        // SAFETY: madt_phys was validated by find_madt to point to a valid MADT.
        let entries = unsafe { parse_madt(madt_phys) }?;

        // CPU topology (shared accumulator: LAPIC on x86, GICC on aarch64).
        let mut cpu_topology = CpuTopology {
            nr_cpus: entries.nr_cpus,
            bsp_id: entries.bsp_id,
            ..Default::default()
        };
        // Copy parsed CPU info into the topology array.
        let copy_count = (entries.nr_cpus as usize).min(MAX_CPUS);
        cpu_topology.cpus[..copy_count].copy_from_slice(&entries.cpus[..copy_count]);

        // Arch misc: store the ACPI tables physical address for debugging.
        let arch_misc = ArchMiscDesc {
            acpi_tables: Some(rsdp_phys),
            pmu_cycle_counter: false,
        };

        // ── Per-architecture descriptor assembly ──
        #[cfg(target_arch = "x86_64")]
        {
            // Build the interrupt controller descriptor (concrete x86-64 type).
            let ic = ApicDesc {
                lapic_base: entries.lapic_base,
                ioapic_base: entries.ioapic_base,
                nr_irqs: entries.nr_irqs,
            };

            // x86-64 timer: PIT (boot) + LAPIC Timer (runtime).
            // PIT base frequency is a fixed hardware constant (1193182 Hz).
            let timer = PitDesc {
                pit_base_freq: 1_193_182,
                lapic_base: entries.lapic_base,
            };

            // Early console: COM1 (0x3F8) — standard PC AT serial port.
            let console = Some(IsaSerialDesc { port_base: 0x3F8 });

            Ok(Self { ic, timer, console, cpu_topology, arch_misc })
        }
        #[cfg(target_arch = "aarch64")]
        {
            // GICv3: distributor from the GICD entry; redistributor base from
            // the first GICC's GICR field; stride/nr_irqs mirror the DTB path
            // (device_tree.rs parse_gic) so both discovery routes produce
            // identical descriptor values on the same machine.
            let ic = Gicv3Desc {
                gicd_base: entries.gicd_base,
                gicr_base: entries.gicr_base,
                gicr_stride: 0x2_0000,
                nr_irqs: 64,
            };

            // ARM Generic Timer: frequency read from CNTFRQ_EL0 at runtime
            // (same rationale as the DTB path).
            let timer = ArmGenericTimerDesc;

            // aarch64 early console is a hardcoded PL011 in minix-plat; the
            // descriptor carries no console (see PlatformDesc::early_console).

            Ok(Self { ic, timer, cpu_topology, arch_misc })
        }
    }

    /// Construct from pre-parsed values (for tests).
    pub fn from_parsed(
        ic: ApicDesc,
        timer: PitDesc,
        console: Option<IsaSerialDesc>,
        cpu_topology: CpuTopology,
    ) -> Self {
        Self { ic, timer, console, cpu_topology, arch_misc: ArchMiscDesc::default() }
    }
}

impl PlatformDesc for AcpiDesc {
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc {
        &self.ic
    }
    fn timer(&self) -> &dyn TimerDesc {
        &self.timer
    }
    #[cfg(target_arch = "x86_64")]
    fn early_console(&self) -> Option<&dyn ConsoleDesc> {
        self.console.as_ref().map(|c| c as &dyn ConsoleDesc)
    }
    #[cfg(target_arch = "aarch64")]
    fn early_console(&self) -> Option<&dyn ConsoleDesc> {
        // aarch64: early console is a hardcoded PL011 in `minix-plat`
        // (0x0900_0000); it does not depend on platform discovery, so the
        // ACPI descriptor carries no console.
        None
    }
    fn cpu_topology(&self) -> CpuTopology {
        self.cpu_topology
    }
    fn arch_misc(&self) -> ArchMiscDesc {
        self.arch_misc
    }
    fn source(&self) -> PlatformSource {
        PlatformSource::Acpi
    }
}

// ── ACPI parsing helpers ──

/// Find the MADT (APIC) table in the XSDT/RSDT.
///
/// Returns the physical address of the MADT, or `None` if not found.
///
/// # Safety
///
/// Caller must guarantee `sdt_phys` points to a valid XSDT/RSDT.
unsafe fn find_madt(sdt_phys: usize, is_xsdt: bool) -> Option<usize> {
    // Read `length` via byte slice to avoid unaligned access on packed field.
    // SDT header layout: signature(4) + length(4) + ...
    let length_bytes = unsafe {
        core::slice::from_raw_parts((sdt_phys + 4) as *const u8, 4)
    };
    let sdt_len = u32_le_from_slice(length_bytes) as usize;

    // Number of table pointers: (length - header_size) / pointer_size.
    let header_size = core::mem::size_of::<SdtHeader>();
    if sdt_len < header_size {
        return None;
    }
    let ptr_size = if is_xsdt { 8 } else { 4 };
    let entry_count = (sdt_len - header_size) / ptr_size;

    let entries_start = sdt_phys + header_size;

    for i in 0..entry_count {
        let entry_addr = entries_start + i * ptr_size;
        // SAFETY: bounds checked via entry_count calculation above.
        let table_phys = if is_xsdt {
            // SAFETY: entry_addr is within the XSDT bounds.
            let bytes = unsafe { core::slice::from_raw_parts(entry_addr as *const u8, 8) };
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            u64::from_le_bytes(arr) as usize
        } else {
            // SAFETY: entry_addr is within the RSDT bounds.
            let bytes = unsafe { core::slice::from_raw_parts(entry_addr as *const u8, 4) };
            u32_le_from_slice(bytes) as usize
        };

        if table_phys == 0 {
            continue;
        }

        // SAFETY: ACPI tables referenced by XSDT/RSDT are valid per spec.
        let table_sig_bytes = unsafe { core::slice::from_raw_parts(table_phys as *const u8, 4) };
        if table_sig_bytes == &MADT_SIGNATURE[..] {
            return Some(table_phys);
        }
    }

    None
}

/// MADT parse result: `(lapic_base, ioapic_base, nr_irqs, cpus, nr_cpus, bsp_id)`.
/// MADT parse result — fields are per-architecture (the entry walk is shared;
/// x86 fills LAPIC/IOAPIC, aarch64 fills GICC/GICD).
struct MadtResult {
    /// x86: Local APIC base from the MADT header's `Local APIC Address` field.
    #[cfg(target_arch = "x86_64")]
    lapic_base: usize,
    /// x86: first IOAPIC base from the `IOAPIC` entry (default 0xFEC0_0000).
    #[cfg(target_arch = "x86_64")]
    ioapic_base: usize,
    /// Discovered CPU list (LAPIC on x86, GICC on aarch64) — shared accumulator.
    cpus: [CpuInfo; MAX_CPUS],
    nr_cpus: u32,
    /// First discovered CPU's hardware ID (x86 APIC ID / aarch64 MPIDR).
    /// Arm ACPI does not encode the boot CPU; see the aarch64 note in parse().
    bsp_id: u32,
    /// x86: GSI count (QEMU/PC constant 64).
    #[cfg(target_arch = "x86_64")]
    nr_irqs: u32,
    /// aarch64: GICD base from the `GIC Distributor` entry (0 = absent).
    #[cfg(target_arch = "aarch64")]
    gicd_base: usize,
    /// aarch64: GICR base from the first `GICC` entry (redistributor region).
    #[cfg(target_arch = "aarch64")]
    gicr_base: usize,
}

/// Parse the MADT (APIC) table.
///
/// Returns `(lapic_base, ioapic_base, nr_irqs, cpus, nr_cpus, bsp_id)`.
///
/// # Safety
///
/// Caller must guarantee `madt_phys` points to a valid MADT.
unsafe fn parse_madt(
    madt_phys: usize,
) -> Result<MadtResult, AcpiParseError> {
    // SAFETY: caller guarantees madt_phys is a valid MADT.
    // Read `length` via byte slice to avoid unaligned access on packed field.
    let length_bytes = unsafe {
        core::slice::from_raw_parts((madt_phys + 4) as *const u8, 4)
    };
    let madt_len = u32_le_from_slice(length_bytes) as usize;

    // MADT header follows the SDT header.
    let madt_header_size = core::mem::size_of::<SdtHeader>() + core::mem::size_of::<MadtHeader>();
    if madt_len < madt_header_size {
        return Err(AcpiParseError::MadtTooShort);
    }

    // Read the MADT header (Local APIC Address) via byte slice.
    // Layout: SDT header (36 bytes) + local_apic_addr (4 bytes) + flags (4 bytes).
    let lapic_addr_bytes = unsafe {
        core::slice::from_raw_parts(
            (madt_phys + core::mem::size_of::<SdtHeader>()) as *const u8,
            4,
        )
    };
    let lapic_base = u32_le_from_slice(lapic_addr_bytes) as usize;

    // Iterate over interrupt controller structures.
    #[cfg(target_arch = "x86_64")]
    let mut ioapic_base = 0xFEC0_0000usize; // Default IOAPIC base (QEMU/PC).
    #[cfg(target_arch = "aarch64")]
    let mut gicd_base = 0usize; // Filled by the GICD entry (0 = absent).
    #[cfg(target_arch = "aarch64")]
    let mut gicr_base = 0usize; // Filled by the first GICC entry.
    let mut nr_cpus = 0u32;
    let mut bsp_id = 0u32;
    let mut cpus = [CpuInfo::default(); MAX_CPUS];

    let mut offset = madt_header_size;
    while offset + core::mem::size_of::<MadtEntryHeader>() <= madt_len {
        // Read entry header (type + length) via byte slice.
        let entry_hdr_bytes = unsafe {
            core::slice::from_raw_parts(
                (madt_phys + offset) as *const u8,
                core::mem::size_of::<MadtEntryHeader>(),
            )
        };
        let entry_type = entry_hdr_bytes[0];
        let entry_len = entry_hdr_bytes[1] as usize;
        if entry_len < core::mem::size_of::<MadtEntryHeader>() || offset + entry_len > madt_len {
            break;
        }

        match entry_type {
            MADT_TYPE_IOAPIC => {
                // IOAPIC structure: extract base address.
                // Layout: type(1) + length(1) + ioapic_id(1) + reserved(1) + ioapic_addr(4) + gsi_base(4).
                if entry_len >= 12 {
                    let ioapic_addr_bytes = unsafe {
                        core::slice::from_raw_parts(
                            (madt_phys + offset + 4) as *const u8,
                            4,
                        )
                    };
                    ioapic_base = u32_le_from_slice(ioapic_addr_bytes) as usize;
                }
            }
            MADT_TYPE_LAPIC => {
                // Processor LAPIC: extract APIC ID if enabled.
                // Layout: type(1) + length(1) + processor_uid(1) + apic_id(1) + flags(4).
                if entry_len >= 8 {
                    let apic_id = unsafe {
                        *((madt_phys + offset + 3) as *const u8)
                    } as u32;
                    let flags_bytes = unsafe {
                        core::slice::from_raw_parts(
                            (madt_phys + offset + 4) as *const u8,
                            4,
                        )
                    };
                    let flags = u32_le_from_slice(flags_bytes);
                    // Bit 0 of flags = "Processor Enabled".
                    let enabled = (flags & 1) != 0;
                    if enabled && (nr_cpus as usize) < MAX_CPUS {
                        let hw_id = apic_id as u64;
                        if nr_cpus == 0 {
                            bsp_id = apic_id;
                        }
                        cpus[nr_cpus as usize] = CpuInfo {
                            hw_id,
                            gicr_base: None,
                            mtimecmp_addr: None,
                        };
                        nr_cpus += 1;
                    }
                }
            }
            MADT_TYPE_X2APIC => {
                // x2APIC: extract UID (32-bit) if enabled. Structure layout
                // differs from LAPIC; we read the relevant fields manually.
                // X2APIC structure (type 9): header(2) + reserved(2) + uid(4) + apic_id(4) + flags(4) + ...
                if entry_len >= 16 {
                    let base = (madt_phys + offset) as *const u8;
                    // SAFETY: entry_len >= 16, bounds checked.
                    let apic_id_bytes =
                        unsafe { core::slice::from_raw_parts(base.add(8), 4) };
                    let apic_id = u32_le_from_slice(apic_id_bytes);
                    let flags_bytes =
                        unsafe { core::slice::from_raw_parts(base.add(12), 4) };
                    let flags = u32_le_from_slice(flags_bytes);
                    let enabled = (flags & 1) != 0;
                    if enabled && (nr_cpus as usize) < MAX_CPUS {
                        let hw_id = apic_id as u64;
                        if nr_cpus == 0 {
                            bsp_id = apic_id;
                        }
                        cpus[nr_cpus as usize] = CpuInfo {
                            hw_id,
                            gicr_base: None,
                            mtimecmp_addr: None,
                        };
                        nr_cpus += 1;
                    }
                }
            }
            #[cfg(target_arch = "aarch64")]
            MADT_TYPE_GICC => {
                // GICC (80 bytes): MPIDR at offset 56, GICR base at 48, Flags
                // at offset 12 (bit 0 = Processor Enabled). Layout per ACPI
                // 6.x §5.2.12.5.
                if entry_len >= 64 {
                    let base = (madt_phys + offset) as *const u8;
                    // SAFETY: entry_len >= 64, bounds checked.
                    let read_u64 = |off: usize| -> u64 {
                        let bytes = unsafe {
                            core::slice::from_raw_parts(base.add(off), 8)
                        };
                        u64::from_le_bytes(bytes.try_into().unwrap())
                    };
                    let flags_bytes = unsafe {
                        core::slice::from_raw_parts(base.add(12), 4)
                    };
                    let flags = u32_le_from_slice(flags_bytes);
                    let enabled = (flags & 1) != 0;
                    if enabled && (nr_cpus as usize) < MAX_CPUS {
                        let hw_id = read_u64(56); // MPIDR
                        let cpu_gicr = read_u64(48);
                        if nr_cpus == 0 {
                            bsp_id = hw_id as u32;
                            gicr_base = cpu_gicr as usize;
                        }
                        cpus[nr_cpus as usize] = CpuInfo {
                            hw_id,
                            gicr_base: Some(cpu_gicr as usize),
                            mtimecmp_addr: None,
                        };
                        nr_cpus += 1;
                    }
                }
            }
            #[cfg(target_arch = "aarch64")]
            MADT_TYPE_GICD => {
                // GIC Distributor (24 bytes): base at offset 12.
                if entry_len >= 20 {
                    let base = (madt_phys + offset) as *const u8;
                    let bytes = unsafe {
                        core::slice::from_raw_parts(base.add(12), 8)
                    };
                    gicd_base =
                        u64::from_le_bytes(bytes.try_into().unwrap()) as usize;
                }
            }
            _ => {
                // Other entry types (Interrupt Source Override, etc.) skipped.
            }
        }

        offset += entry_len;
    }

    // aarch64: a MADT with CPUs but no GICD entry is unusable — the
    // interrupt controller base would be missing for every consumer.
    #[cfg(target_arch = "aarch64")]
    if nr_cpus > 0 && gicd_base == 0 {
        return Err(AcpiParseError::GicdNotFound);
    }

    // If no CPUs were found in the MADT, default to single-core.
    if nr_cpus == 0 {
        cpus[0] = CpuInfo { hw_id: 0, gicr_base: None, mtimecmp_addr: None };
        nr_cpus = 1;
    }

    Ok(MadtResult {
        #[cfg(target_arch = "x86_64")]
        lapic_base,
        #[cfg(target_arch = "x86_64")]
        ioapic_base,
        cpus,
        nr_cpus,
        bsp_id,
        #[cfg(target_arch = "x86_64")]
        nr_irqs: 64, // QEMU/PC GSI count (16 ISA + 8 PCI); real HW varies.
        #[cfg(target_arch = "aarch64")]
        gicd_base,
        #[cfg(target_arch = "aarch64")]
        gicr_base,
    })
}

// ── Little-endian byte helpers ──
//
// ACPI tables are little-endian (x86 origin). On x86-64 (the only ACPI
// target), native byte order is also little-endian, so packed struct
// fields read as `u32`/`u64` are already in the correct byte order.
// We use byte-slice reads to avoid unaligned access on packed fields.

/// Read a `u32` from a 4-byte slice (for unaligned byte-array access).
fn u32_le_from_slice(bytes: &[u8]) -> u32 {
    let mut arr = [0u8; 4];
    if bytes.len() >= 4 {
        arr.copy_from_slice(&bytes[..4]);
    }
    u32::from_le_bytes(arr)
}

/// Errors that can occur during ACPI parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiParseError {
    /// RSDP signature mismatch (not "RSD PTR ").
    BadRsdpSignature,
    /// RSDP has no valid XSDT/RSDT pointer.
    NoXsdtPointer,
    /// MADT (APIC) table not found in XSDT/RSDT.
    MadtNotFound,
    /// MADT table is too short to contain a header.
    MadtTooShort,
}

impl fmt::Display for AcpiParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRsdpSignature => write!(f, "RSDP signature mismatch"),
            Self::NoXsdtPointer => write!(f, "RSDP has no XSDT/RSDT pointer"),
            Self::MadtNotFound => write!(f, "MADT (APIC) table not found"),
            Self::MadtTooShort => write!(f, "MADT table too short"),
        }
    }
}

// (Helpers are private; tests access them via `use super::*`.)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acpi_parse_error_variants() {
        let errs = [
            AcpiParseError::BadRsdpSignature,
            AcpiParseError::NoXsdtPointer,
            AcpiParseError::MadtNotFound,
            AcpiParseError::MadtTooShort,
        ];
        for i in 0..errs.len() {
            for j in (i + 1)..errs.len() {
                assert_ne!(errs[i], errs[j], "duplicate error variant");
            }
        }
    }

    #[test]
    fn test_acpi_desc_from_parsed() {
        let ic = ApicDesc {
            lapic_base: 0xFEE0_0000,
            ioapic_base: 0xFEC0_0000,
            nr_irqs: 64,
        };
        let timer = PitDesc {
            pit_base_freq: 1_193_182,
            lapic_base: 0xFEE0_0000,
        };
        let console = Some(IsaSerialDesc { port_base: 0x3F8 });
        let desc = AcpiDesc::from_parsed(ic, timer, console, CpuTopology::default());
        assert_eq!(desc.source(), PlatformSource::Acpi);
        // Downcast via Any to verify the concrete type.
        let ic_ref = desc.interrupt_controller();
        let apic = ic_ref
            .as_any()
            .downcast_ref::<ApicDesc>()
            .expect("expected ApicDesc");
        assert_eq!(apic.lapic_base, 0xFEE0_0000);
        assert_eq!(apic.ioapic_base, 0xFEC0_0000);
    }

    #[test]
    fn test_u32_le_from_slice() {
        assert_eq!(u32_le_from_slice(&[0x78, 0x56, 0x34, 0x12]), 0x12345678);
    }

    /// Build a minimal synthetic ACPI table set in memory and parse it.
    ///
    /// This verifies the full RSDP → XSDT → MADT chain works correctly.
    #[test]
    fn test_parse_synthetic_acpi() {
        // Layout (all little-endian):
        //   0x0000: RSDP (36 bytes for rev 2)
        //   0x1000: XSDT (header + 1 entry pointing to MADT)
        //   0x2000: MADT (header + 1 IOAPIC + 1 LAPIC)
        // Use a heap-allocated buffer via `Box` (tests can use alloc).
        extern crate alloc;
        use alloc::boxed::Box;
        let mut buf = Box::new([0u8; 0x3000]);

        // Compute virtual addresses for each table within the buffer.
        let buf_base = buf.as_ptr() as usize;
        let xsdt_vaddr = buf_base + 0x1000;
        let madt_vaddr = buf_base + 0x2000;

        // ── RSDP at 0x0000 ──
        buf[0..8].copy_from_slice(b"RSD PTR ");
        buf[8] = 0; // checksum (ignored by parser)
        buf[9..15].copy_from_slice(b"MINIX ");
        buf[15] = RSDP_REV_2; // revision 2.0
        buf[16..20].copy_from_slice(&0x1000u32.to_le_bytes()); // rsdt_addr (unused)
        buf[20..24].copy_from_slice(&36u32.to_le_bytes()); // length
        buf[24..32].copy_from_slice(&(xsdt_vaddr as u64).to_le_bytes()); // xsdt_addr (virtual)
        buf[32] = 0; // extended_checksum
        buf[33..36].copy_from_slice(&[0, 0, 0]); // reserved

        // ── XSDT at 0x1000 ──
        let xsdt_offset = 0x1000usize;
        let xsdt_header_size = core::mem::size_of::<SdtHeader>();
        let xsdt_total = xsdt_header_size + 8; // header + 1 entry (8 bytes for XSDT)
        buf[xsdt_offset..xsdt_offset + 4].copy_from_slice(b"XSDT");
        buf[xsdt_offset + 4..xsdt_offset + 8].copy_from_slice(&(xsdt_total as u32).to_le_bytes());
        buf[xsdt_offset + 8] = 1; // revision
        buf[xsdt_offset + 9] = 0; // checksum
        buf[xsdt_offset + 10..xsdt_offset + 16].copy_from_slice(b"MINIX ");
        buf[xsdt_offset + 16..xsdt_offset + 24].copy_from_slice(b"XSDTMINI");
        buf[xsdt_offset + 24..xsdt_offset + 28].copy_from_slice(&0u32.to_le_bytes());
        buf[xsdt_offset + 28..xsdt_offset + 32].copy_from_slice(b"MINI");
        buf[xsdt_offset + 32..xsdt_offset + 36].copy_from_slice(&0u32.to_le_bytes());
        // XSDT entry 0: pointer to MADT (virtual address within buf).
        buf[xsdt_offset + xsdt_header_size..xsdt_offset + xsdt_header_size + 8]
            .copy_from_slice(&(madt_vaddr as u64).to_le_bytes());

        // ── MADT at 0x2000 ──
        let madt_offset = 0x2000usize;
        let madt_header_total =
            core::mem::size_of::<SdtHeader>() + core::mem::size_of::<MadtHeader>();
        // IOAPIC entry: 12 bytes. LAPIC entry: 8 bytes.
        let madt_total = madt_header_total + 12 + 8;
        buf[madt_offset..madt_offset + 4].copy_from_slice(b"APIC");
        buf[madt_offset + 4..madt_offset + 8].copy_from_slice(&(madt_total as u32).to_le_bytes());
        buf[madt_offset + 8] = 3; // revision
        buf[madt_offset + 9] = 0; // checksum
        buf[madt_offset + 10..madt_offset + 16].copy_from_slice(b"MINIX ");
        buf[madt_offset + 16..madt_offset + 24].copy_from_slice(b"MADTMINI");
        buf[madt_offset + 24..madt_offset + 28].copy_from_slice(&0u32.to_le_bytes());
        buf[madt_offset + 28..madt_offset + 32].copy_from_slice(b"MINI");
        buf[madt_offset + 32..madt_offset + 36].copy_from_slice(&0u32.to_le_bytes());
        // MADT header: local_apic_addr + flags.
        buf[madt_offset + 36..madt_offset + 40].copy_from_slice(&0xFEE0_0000u32.to_le_bytes());
        buf[madt_offset + 40..madt_offset + 44].copy_from_slice(&1u32.to_le_bytes());

        // IOAPIC entry at madt_offset + 44.
        let ioapic_off = madt_offset + 44;
        buf[ioapic_off] = MADT_TYPE_IOAPIC;
        buf[ioapic_off + 1] = 12;
        buf[ioapic_off + 2] = 0;
        buf[ioapic_off + 3] = 0;
        buf[ioapic_off + 4..ioapic_off + 8].copy_from_slice(&0xFEC0_0000u32.to_le_bytes());
        buf[ioapic_off + 8..ioapic_off + 12].copy_from_slice(&0u32.to_le_bytes());

        // LAPIC entry at madt_offset + 56.
        let lapic_off = madt_offset + 56;
        buf[lapic_off] = MADT_TYPE_LAPIC;
        buf[lapic_off + 1] = 8;
        buf[lapic_off + 2] = 0;
        buf[lapic_off + 3] = 0;
        buf[lapic_off + 4..lapic_off + 8].copy_from_slice(&1u32.to_le_bytes());

        // Parse from the heap buffer. All table pointers (RSDP→XSDT,
        // XSDT→MADT) use virtual addresses within `buf`, so `parse` can
        // safely dereference them in userspace.
        let rsdp_phys = buf_base;
        // SAFETY: we constructed a valid synthetic ACPI table set in `buf`.
        let desc = unsafe { AcpiDesc::parse(rsdp_phys) }
            .expect("ACPI parse should succeed on synthetic tables");

        // Verify extracted values.
        assert_eq!(desc.source(), PlatformSource::Acpi);
        let ic_ref = desc.interrupt_controller();
        let apic = ic_ref
            .as_any()
            .downcast_ref::<ApicDesc>()
            .expect("expected ApicDesc");
        assert_eq!(apic.lapic_base, 0xFEE0_0000, "LAPIC base mismatch");
        assert_eq!(apic.ioapic_base, 0xFEC0_0000, "IOAPIC base mismatch");
        assert_eq!(apic.nr_irqs, 64, "IRQ count mismatch");

        let timer_ref = desc.timer();
        let pit = timer_ref
            .as_any()
            .downcast_ref::<PitDesc>()
            .expect("expected PitDesc");
        assert_eq!(pit.pit_base_freq, 1_193_182);
        assert_eq!(pit.lapic_base, 0xFEE0_0000);

        let topo = desc.cpu_topology();
        assert_eq!(topo.nr_cpus, 1, "should find 1 CPU");
        assert_eq!(topo.bsp_id, 0, "BSP APIC ID should be 0");
        assert_eq!(topo.cpus[0].hw_id, 0, "CPU 0 hw_id should be 0");
    }
}

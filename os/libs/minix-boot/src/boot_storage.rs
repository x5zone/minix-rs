//! Caller-owned boot-record storage and its borrowed view (D-1).
//!
//! The producer side of the cross-image handoff used to fabricate `'static`
//! slices: the UEFI legs pointed them into the arena's semantic partition and
//! the loaders leaked their `Vec`s, so "the report lives forever" was asserted
//! instead of owned. This module makes the ownership explicit:
//!
//! - [`BootStorage`] is the **owner**: fixed-capacity arrays the caller keeps
//!   in its own static storage (`.data` when `new()` is not all-zero, or on its stack) and fills during boot preparation;
//! - [`BootReport<'a>`] is the **view**: borrowed slices into that storage,
//!   plus the scalar fields the kernel entry validates before paging setup.
//!
//! A report cannot be promoted to `'static` without a `'static` owner: the
//! lifetime parameter ties it to the storage. The kernel image keeps its
//! [`crate::KernelInfo`] with genuine `'static` slices because it deep-copies
//! the decoded payloads into its own `.bss` before first use
//! (`store_kernel_info`); the senders never need that type any more.
//!
//! Capacities are the wire partition capacities — for **memmap / reserved /
//! modules / platform / param** the storage is the single semantic source the
//! wire is encoded from, so the two cannot disagree about how many entries
//! fit. Source records are the exception on the UEFI legs: the
//! classifier writes them straight to the wire (its 296-record source budget
//! does not fit this storage's 40 slots), so `BootReport` is not a complete
//! view there. The DTB/U-Boot leg fills every slot and is the single-source
//! case the capacity invariant describes.

use minix_types::{PhysBytes, VirBytes};

use crate::boot_params::PARAM_BUF_CAP;
use crate::handoff_v1::{
    WIRE_MEMMAP_CAP, WIRE_MODULE_CAP, WIRE_PARAM_CAP, WIRE_PLATFORM_CAP, WIRE_RESERVED_CAP,
};
use crate::kernel_info::{BootModule, MemoryRegion};
use crate::memmap_source::{SourceRecord, DTB_SOURCE_CAP, EMPTY_SOURCE_RECORD};
use crate::platform::{DTB, PlatformDescSource};

const _: () = assert!(PARAM_BUF_CAP == WIRE_PARAM_CAP);

const ZERO_REGION: MemoryRegion = MemoryRegion {
    base: PhysBytes(0),
    len: 0,
};

const ZERO_MODULE: BootModule = BootModule {
    name: "",
    start: PhysBytes(0),
    len: 0,
};

/// The placeholder destination for an unused slot. Only ever read through the
/// live prefix (`*_len`), so its contents are irrelevant.
const ZERO_SOURCE: PlatformDescSource = PlatformDescSource::new(DTB, PhysBytes(0));

const ZERO_RECORD: SourceRecord = EMPTY_SOURCE_RECORD;

/// The two map-table counts, named so a swap is a struct-literal typo.
///
/// [`BootStorage::fill_map_tables`] takes this back from the classifier: both
/// tables come from one pass, so committing them must be one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapCounts {
    pub memmap: usize,
    pub reserved: usize,
}

/// Fixed-capacity owner of the boot records a sender hands to the kernel.
///
/// Every payload family is filled through its `fill_*` method: the closure
/// receives the slot array, returns how many entries it wrote, and the
/// method commits that count before returning. **Write and commit are one
/// step**, so a report can only ever see a committed prefix — there is no
/// "slots written, length not committed" half-state to intercept, and
/// therefore no dirty flag.
///
/// The map pair is the exception in shape only: [`Self::fill_map_tables`]
/// hands out both tables and takes back a [`MapCounts`], because the
/// classifier produces both from one pass and a caller that updated one
/// length alone would expose a report whose two tables came from different
/// rounds.
///
/// **Ordering rule**: a `fill_*` call borrows `&mut self` for its duration,
/// so two families cannot be interleaved ("write A, write B, then commit
/// both"). Every production call site is strictly "write one family, commit
/// it, move on", which is also what the retired two-step API relied on.
pub struct BootStorage {
    memmap: [MemoryRegion; WIRE_MEMMAP_CAP],
    memmap_len: usize,
    reserved: [MemoryRegion; WIRE_RESERVED_CAP],
    reserved_len: usize,
    modules: [BootModule; WIRE_MODULE_CAP],
    modules_len: usize,
    platform_sources: [PlatformDescSource; WIRE_PLATFORM_CAP],
    platform_len: usize,
    param_buf: [u8; PARAM_BUF_CAP],
    param_len: usize,
    /// Provenance records for the memmap/reserved tables (the DTB collector's
    /// own capacity, 40).
    source_records: [SourceRecord; DTB_SOURCE_CAP],
    source_records_len: usize,
    map_committed: bool,
    modules_committed: bool,
    platform_committed: bool,
    param_committed: bool,
    sources_committed: bool,
}

impl Default for BootStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl BootStorage {
    /// An empty storage. Const-constructible for a static; because `new()`
    /// contains non-zero defaults, such a static lands in `.data`, not `.bss`.
    pub const fn new() -> Self {
        Self {
            memmap: [ZERO_REGION; WIRE_MEMMAP_CAP],
            memmap_len: 0,
            reserved: [ZERO_REGION; WIRE_RESERVED_CAP],
            reserved_len: 0,
            modules: [ZERO_MODULE; WIRE_MODULE_CAP],
            modules_len: 0,
            platform_sources: [ZERO_SOURCE; WIRE_PLATFORM_CAP],
            platform_len: 0,
            param_buf: [0u8; PARAM_BUF_CAP],
            param_len: 0,
            source_records: [ZERO_RECORD; DTB_SOURCE_CAP],
            source_records_len: 0,
            map_committed: false,
            modules_committed: false,
            platform_committed: false,
            param_committed: false,
            sources_committed: false,
        }
    }

    /// Fill both map tables from one classification pass, then commit both
    /// counts.
    ///
    /// The two tables come from the same pass, so they are handed out and
    /// committed together: a caller that updated one count alone would expose
    /// a report whose tables disagreed. The closure returns a [`MapCounts`]
    /// with both counts written out by field name, so a swap of the two
    /// numbers is a struct-literal typo rather than a positional one.
    pub fn fill_map_tables<F>(&mut self, write: F)
    where
        F: FnOnce(&mut [MemoryRegion], &mut [MemoryRegion]) -> MapCounts,
    {
        assert!(!self.map_committed, "BootStorage: map tables already committed");
        let counts = write(&mut self.memmap, &mut self.reserved);
        assert!(
            counts.memmap <= WIRE_MEMMAP_CAP,
            "BootStorage: memmap length {} exceeds capacity {WIRE_MEMMAP_CAP}",
            counts.memmap
        );
        assert!(
            counts.reserved <= WIRE_RESERVED_CAP,
            "BootStorage: reserved length {} exceeds capacity {WIRE_RESERVED_CAP}",
            counts.reserved
        );
        self.memmap_len = counts.memmap;
        self.reserved_len = counts.reserved;
        self.map_committed = true;
    }

    /// Fill the boot-module slots, then commit the count.
    pub fn fill_modules<F>(&mut self, write: F)
    where
        F: FnOnce(&mut [BootModule]) -> usize,
    {
        assert!(!self.modules_committed, "BootStorage: module length already committed");
        let written = write(&mut self.modules);
        assert!(
            written <= WIRE_MODULE_CAP,
            "BootStorage: module length {written} exceeds capacity {WIRE_MODULE_CAP}"
        );
        self.modules_len = written;
        self.modules_committed = true;
    }

    /// Fill the platform-source slots, then commit the count.
    pub fn fill_platform<F>(&mut self, write: F)
    where
        F: FnOnce(&mut [PlatformDescSource]) -> usize,
    {
        assert!(!self.platform_committed, "BootStorage: platform length already committed");
        let written = write(&mut self.platform_sources);
        assert!(
            written <= WIRE_PLATFORM_CAP,
            "BootStorage: platform length {written} exceeds capacity {WIRE_PLATFORM_CAP}"
        );
        self.platform_len = written;
        self.platform_committed = true;
    }

    /// Fill the parameter buffer, then commit the byte count.
    ///
    /// The closure receives the whole fixed-size buffer and returns the
    /// number of bytes it used.
    pub fn fill_param<F>(&mut self, write: F)
    where
        F: FnOnce(&mut [u8; PARAM_BUF_CAP]) -> usize,
    {
        assert!(!self.param_committed, "BootStorage: parameter length already committed");
        let written = write(&mut self.param_buf);
        assert!(
            written <= PARAM_BUF_CAP,
            "BootStorage: param length {written} exceeds capacity {PARAM_BUF_CAP}"
        );
        self.param_len = written;
        self.param_committed = true;
    }

    /// Fill the provenance-record slots, then commit the count.
    pub fn fill_source_records<F>(&mut self, write: F)
    where
        F: FnOnce(&mut [SourceRecord]) -> usize,
    {
        assert!(!self.sources_committed, "BootStorage: source length already committed");
        let written = write(&mut self.source_records);
        assert!(
            written <= DTB_SOURCE_CAP,
            "BootStorage: memmap-source length {written} exceeds capacity {DTB_SOURCE_CAP}"
        );
        self.source_records_len = written;
        self.sources_committed = true;
    }

    /// Live provenance records.
    pub fn source_records(&self) -> &[SourceRecord] {
        &self.source_records[..self.source_records_len]
    }

    /// Live memmap entries.
    pub fn memmap(&self) -> &[MemoryRegion] {
        &self.memmap[..self.memmap_len]
    }

    /// Live reserved entries.
    pub fn reserved_regions(&self) -> &[MemoryRegion] {
        &self.reserved[..self.reserved_len]
    }

    /// Live boot modules.
    pub fn boot_modules(&self) -> &[BootModule] {
        &self.modules[..self.modules_len]
    }

    /// Live platform descriptor sources.
    pub fn platform_sources(&self) -> &[PlatformDescSource] {
        &self.platform_sources[..self.platform_len]
    }

    /// Live normalized parameter bytes.
    pub fn param_buf(&self) -> &[u8] {
        &self.param_buf[..self.param_len]
    }

    /// Build the borrowed report for the kernel entry.
    pub fn report(
        &self,
        kern_virt_base: VirBytes,
        kern_phys_base: PhysBytes,
        kern_size: u64,
    ) -> BootReport<'_> {
        // No dirty-flag assertion is needed: every family
        // is filled and committed in one `fill_*` call, so "slots written,
        // length not committed" is not a representable state.
        BootReport {
            memmap: self.memmap(),
            reserved_regions: self.reserved_regions(),
            boot_modules: self.boot_modules(),
            platform_sources: self.platform_sources(),
            param_buf: self.param_buf(),
            source_records: self.source_records(),

            kern_virt_base,
            kern_phys_base,
            kern_size,
        }
    }
}


/// Borrowed semantic view of a [`BootStorage`] plus the kernel scalars.
///
/// This is what a v1 sender passes to the kernel entry. The kernel image's
/// [`crate::KernelInfo`] is the `'static` receiver-side form (backed by its
/// own storage), not a sender type.
#[derive(Clone, Copy)]
pub struct BootReport<'a> {
    pub memmap: &'a [MemoryRegion],
    pub reserved_regions: &'a [MemoryRegion],
    pub boot_modules: &'a [BootModule],
    pub platform_sources: &'a [PlatformDescSource],
    pub param_buf: &'a [u8],
    /// Provenance records for the two map tables. On the DTB/U-Boot/direct
    /// legs these are the complete slice owned by `BootStorage`; on the UEFI
    /// leg the classifier writes the records straight into the wire, so this
    /// slice is empty and the wire is the only authority.
    pub source_records: &'a [SourceRecord],
    // No startup-object slice: every startup-object value is a pure
    // restatement the kernel derives from the compile-time constants.

    pub kern_virt_base: VirBytes,
    pub kern_phys_base: PhysBytes,
    pub kern_size: u64,
    // No `user_sp` / `kern_stack_top` / `syscall_entry` here: these are pure
    // restatements the wire does not carry. Nothing in production reads
    // them: `user_sp` is the `minix_types::USER_STACK_TOP` constant,
    // `syscall_entry` has `syscall_entry_va()`, and `kern_stack_top` has no
    // shim-side meaning.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::PlatformDescSource;

    fn region(base: u64, len: usize) -> MemoryRegion {
        MemoryRegion {
            base: PhysBytes(base),
            len,
        }
    }

    fn module(name: &'static str, start: u64, len: usize) -> BootModule {
        BootModule {
            name,
            start: PhysBytes(start),
            len,
        }
    }

    #[test]
    fn report_exposes_only_the_committed_prefixes() {
        let mut storage = BootStorage::new();
        storage.fill_map_tables(|memmap, reserved| {
            memmap[0] = region(0x1000, 0x2000);
            reserved[0] = region(0x9000, 0x1000);
            MapCounts {
                memmap: 1,
                reserved: 1,
            }
        });
        storage.fill_modules(|slots| {
            slots[0] = module("vm", 0x30_0000, 0x1000);
            1
        });
        storage.fill_platform(|slots| {
            slots[0] = PlatformDescSource::new(DTB, PhysBytes(0x4000_0000));
            1
        });
        storage.fill_param(|buf| {
            buf[..3].copy_from_slice(b"a=b");
            3
        });

        let report = storage.report(
            VirBytes(0xFFFF_8000_0000_0000),
            PhysBytes(0x20_0000),
            0x40_0000,
        );
        assert_eq!(report.memmap.len(), 1);
        assert_eq!(
            (report.memmap[0].base.0, report.memmap[0].len),
            (0x1000, 0x2000)
        );
        assert_eq!(report.reserved_regions.len(), 1);
        assert_eq!(
            (
                report.reserved_regions[0].base.0,
                report.reserved_regions[0].len
            ),
            (0x9000, 0x1000)
        );
        assert_eq!(report.boot_modules.len(), 1);
        assert_eq!(report.boot_modules[0].name, "vm");
        assert_eq!(report.platform_sources.len(), 1);
        assert_eq!(report.param_buf, b"a=b");
    }

    #[test]
    fn a_fresh_storage_reports_empty_lists_and_zero_param_bytes() {
        let storage = BootStorage::new();
        let report = storage.report(VirBytes(0), PhysBytes(0), 0);
        assert!(report.memmap.is_empty());
        assert!(report.reserved_regions.is_empty());
        assert!(report.boot_modules.is_empty());
        assert!(report.platform_sources.is_empty());
        assert!(report.param_buf.is_empty());
    }

    /// A second fill of the same family is refused: the one-shot commit
    /// flags survive (`fill_*` replaced the old "slots now, length later"
    /// pair, but the commit is still once per boot face).
    #[test]
    #[should_panic(expected = "module length already committed")]
    fn fill_modules_twice_is_refused() {
        let mut storage = BootStorage::new();
        storage.fill_modules(|_| 0);
        storage.fill_modules(|_| 0);
    }

    #[test]
    fn fill_map_tables_commits_both_prefixes_together() {
        let mut storage = BootStorage::new();
        storage.fill_map_tables(|memmap, reserved| {
            memmap[0] = region(0x1000, 0x2000);
            reserved[0] = region(0x9000, 0x1000);
            MapCounts {
                memmap: 1,
                reserved: 1,
            }
        });
        let report = storage.report(
            VirBytes(0xFFFF_8000_0000_0000),
            PhysBytes(0x20_0000),
            0x40_0000,
        );
        assert_eq!(report.memmap.len(), 1);
        assert_eq!(report.reserved_regions.len(), 1);
    }

    #[test]
    #[should_panic(expected = "map tables already committed")]
    fn fill_map_tables_twice_is_refused() {
        let mut storage = BootStorage::new();
        storage.fill_map_tables(|_, _| MapCounts { memmap: 0, reserved: 0 });
        storage.fill_map_tables(|_, _| MapCounts { memmap: 0, reserved: 0 });
    }

    #[test]
    #[should_panic(expected = "exceeds capacity")]
    fn fill_map_tables_past_capacity_panics() {
        let mut storage = BootStorage::new();
        storage.fill_map_tables(|_, _| MapCounts {
            memmap: WIRE_MEMMAP_CAP + 1,
            reserved: 0,
        });
    }

}
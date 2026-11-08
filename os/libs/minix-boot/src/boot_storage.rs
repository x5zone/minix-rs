//! Caller-owned boot-record storage and its borrowed view (D-1).
//!
//! The producer side of the cross-image handoff used to fabricate `'static`
//! slices: the UEFI legs pointed them into the arena's semantic partition and
//! the loaders leaked their `Vec`s, so "the report lives forever" was asserted
//! instead of owned. This module makes the ownership explicit:
//!
//! - [`BootStorage`] is the **owner**: fixed-capacity arrays the caller keeps
//!   in its own `.bss` (or on its stack) and fills during boot preparation;
//! - [`BootReport<'a>`] is the **view**: borrowed slices into that storage,
//!   plus the scalar fields the kernel entry validates before paging setup.
//!
//! A report cannot be promoted to `'static` without a `'static` owner: the
//! lifetime parameter ties it to the storage. The kernel image keeps its
//! [`crate::KernelInfo`] with genuine `'static` slices because it deep-copies
//! the decoded payloads into its own `.bss` before first use
//! (`store_kernel_info`); the senders never need that type any more.
//!
//! Capacities are the wire partition capacities — the storage is the single
//! semantic source the wire is encoded from, so the two cannot disagree about
//! how many entries fit.

use minix_types::{PhysBytes, VirBytes};

use crate::boot_params::PARAM_BUF_CAP;
use crate::handoff_v1::{
    WIRE_MEMMAP_CAP, WIRE_MODULE_CAP, WIRE_PLATFORM_CAP, WIRE_RESERVED_CAP, WIRE_STARTUP_CAP,
};
use crate::kernel_info::{BootModule, MemoryRegion};
use crate::memmap_source::{startup_record, SourceRecord, DTB_SOURCE_CAP};
use crate::platform::{DTB, PlatformDescSource};

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

const ZERO_RECORD: SourceRecord = startup_record(0, 0, 0);

/// Fixed-capacity owner of the boot records a sender hands to the kernel.
///
/// Fill the map slots through [`Self::memmap_slots_mut`] /
/// [`Self::reserved_slots_mut`] (then commit the live prefix with the `set_*`
/// methods), and the counted lists through the `push_*` methods. Every setter
/// checks its bound, so a report can never expose an unwritten slot.
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
    memmap_sources: [SourceRecord; DTB_SOURCE_CAP],
    memmap_sources_len: usize,
    /// Startup-object records (kernel + modules + bootstrap objects), in the
    /// wire partition's capacity.
    startup_objects: [SourceRecord; WIRE_STARTUP_CAP],
    startup_len: usize,
}

impl BootStorage {
    /// An empty storage. Const-constructible for a `.bss` static.
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
            memmap_sources: [ZERO_RECORD; DTB_SOURCE_CAP],
            memmap_sources_len: 0,
            startup_objects: [ZERO_RECORD; WIRE_STARTUP_CAP],
            startup_len: 0,
        }
    }

    /// Commit both map-table lengths as one step.
    ///
    /// The classifier fills the two tables from one classification pass; a
    /// caller that updates only one length would expose a report whose two
    /// tables come from different rounds. This commit makes that half-state
    /// unrepresentable at the call site.
    pub fn commit_map_tables(&mut self, memmap_len: usize, reserved_len: usize) {
        assert!(
            memmap_len <= WIRE_MEMMAP_CAP,
            "BootStorage: memmap length {memmap_len} exceeds capacity {WIRE_MEMMAP_CAP}"
        );
        assert!(
            reserved_len <= WIRE_RESERVED_CAP,
            "BootStorage: reserved length {reserved_len} exceeds capacity {WIRE_RESERVED_CAP}"
        );
        self.memmap_len = memmap_len;
        self.reserved_len = reserved_len;
    }

    /// The full memmap slot array. The classifier writes entries starting at
    /// index 0 and then commits both table counts with
    /// [`Self::commit_map_tables`].
    pub fn memmap_slots_mut(&mut self) -> &mut [MemoryRegion] {
        &mut self.memmap
    }

    /// Both map tables as one disjoint pair of mutable slices.
    ///
    /// A caller that needs to fill both in one call (the classifier) cannot
    /// take two `&mut self` borrows; this method splits the fields inside the
    /// storage.
    pub fn map_tables_mut(&mut self) -> (&mut [MemoryRegion], &mut [MemoryRegion]) {
        (&mut self.memmap, &mut self.reserved)
    }

    /// Commit the live memmap prefix length.
    ///
    /// Production code must use [`Self::commit_map_tables`]; this single-table
    /// form exists only for tests that check one side at a time.
    #[cfg(test)]
    pub(crate) fn set_memmap_len(&mut self, len: usize) {
        assert!(
            len <= WIRE_MEMMAP_CAP,
            "BootStorage: memmap length {len} exceeds capacity {WIRE_MEMMAP_CAP}"
        );
        self.memmap_len = len;
    }

    /// The full reserved slot array (same protocol as the memmap slots).
    pub fn reserved_slots_mut(&mut self) -> &mut [MemoryRegion] {
        &mut self.reserved
    }

    /// Commit the live reserved prefix length.
    ///
    /// Production code must use [`Self::commit_map_tables`]; this single-table
    /// form exists only for tests that check one side at a time.
    #[cfg(test)]
    pub(crate) fn set_reserved_len(&mut self, len: usize) {
        assert!(
            len <= WIRE_RESERVED_CAP,
            "BootStorage: reserved length {len} exceeds capacity {WIRE_RESERVED_CAP}"
        );
        self.reserved_len = len;
    }

    /// The full boot-module slot array. The loader writes entries starting at
    /// index 0 and then commits the count with [`Self::set_modules_len`].
    pub fn module_slots_mut(&mut self) -> &mut [BootModule] {
        &mut self.modules
    }

    /// Commit the live boot-module prefix length.
    pub fn set_modules_len(&mut self, len: usize) {
        assert!(
            len <= WIRE_MODULE_CAP,
            "BootStorage: module length {len} exceeds capacity {WIRE_MODULE_CAP}"
        );
        self.modules_len = len;
    }

    /// Append one boot module. Fails when the fixed list is full.
    ///
    /// Production loaders write the slot array directly; this append form is
    /// used by tests that build a storage record by record.
    #[cfg(test)]
    pub fn push_module(&mut self, module: BootModule) -> Result<(), BootStorageFull> {
        if self.modules_len >= WIRE_MODULE_CAP {
            return Err(BootStorageFull);
        }
        self.modules[self.modules_len] = module;
        self.modules_len += 1;
        Ok(())
    }

    /// The full platform-source slot array (same protocol as the module slots).
    pub fn platform_slots_mut(&mut self) -> &mut [PlatformDescSource] {
        &mut self.platform_sources
    }

    /// Commit the live platform-source prefix length.
    pub fn set_platform_len(&mut self, len: usize) {
        assert!(
            len <= WIRE_PLATFORM_CAP,
            "BootStorage: platform length {len} exceeds capacity {WIRE_PLATFORM_CAP}"
        );
        self.platform_len = len;
    }

    /// Append one platform descriptor source. Fails when the list is full.
    pub fn push_platform_source(
        &mut self,
        source: PlatformDescSource,
    ) -> Result<(), BootStorageFull> {
        if self.platform_len >= WIRE_PLATFORM_CAP {
            return Err(BootStorageFull);
        }
        self.platform_sources[self.platform_len] = source;
        self.platform_len += 1;
        Ok(())
    }

    /// The parameter landing buffer. The caller normalizes into it and then
    /// commits the live prefix with [`Self::set_param_len`].
    pub fn param_buf_mut(&mut self) -> &mut [u8; PARAM_BUF_CAP] {
        &mut self.param_buf
    }

    /// Commit the live parameter-buffer prefix length.
    pub fn set_param_len(&mut self, len: usize) {
        assert!(
            len <= PARAM_BUF_CAP,
            "BootStorage: param length {len} exceeds capacity {PARAM_BUF_CAP}"
        );
        self.param_len = len;
    }

    /// The full provenance-record slot array (the DTB collector writes here,
    /// then commits the count with [`Self::set_memmap_sources_len`]).
    pub fn memmap_source_slots_mut(&mut self) -> &mut [SourceRecord] {
        &mut self.memmap_sources
    }

    /// Commit the live provenance-record prefix length.
    pub fn set_memmap_sources_len(&mut self, len: usize) {
        assert!(
            len <= DTB_SOURCE_CAP,
            "BootStorage: memmap-source length {len} exceeds capacity {DTB_SOURCE_CAP}"
        );
        self.memmap_sources_len = len;
    }

    /// The full startup-object slot array.
    pub fn startup_slots_mut(&mut self) -> &mut [SourceRecord] {
        &mut self.startup_objects
    }

    /// Commit the live startup-object prefix length.
    pub fn set_startup_len(&mut self, len: usize) {
        assert!(
            len <= WIRE_STARTUP_CAP,
            "BootStorage: startup length {len} exceeds capacity {WIRE_STARTUP_CAP}"
        );
        self.startup_len = len;
    }

    /// Append one startup-object record. Fails when the partition is full.
    pub fn push_startup(&mut self, record: SourceRecord) -> Result<(), BootStorageFull> {
        if self.startup_len >= WIRE_STARTUP_CAP {
            return Err(BootStorageFull);
        }
        self.startup_objects[self.startup_len] = record;
        self.startup_len += 1;
        Ok(())
    }

    /// Live provenance records.
    pub fn memmap_sources(&self) -> &[SourceRecord] {
        &self.memmap_sources[..self.memmap_sources_len]
    }

    /// Live startup-object records.
    pub fn startup_objects(&self) -> &[SourceRecord] {
        &self.startup_objects[..self.startup_len]
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
    ///
    /// The derived fields follow the boot layout contract:
    /// `kern_stack_top = kern_virt_base + kern_size` and
    /// `syscall_entry = kern_virt_base`; `user_sp` comes from the single
    /// per-arch authority `minix_types::USER_STACK_TOP`.
    pub fn report(
        &self,
        kern_virt_base: VirBytes,
        kern_phys_base: PhysBytes,
        kern_size: u64,
    ) -> BootReport<'_> {
        BootReport {
            memmap: self.memmap(),
            reserved_regions: self.reserved_regions(),
            boot_modules: self.boot_modules(),
            platform_sources: self.platform_sources(),
            param_buf: self.param_buf(),
            memmap_sources: self.memmap_sources(),
            startup_objects: self.startup_objects(),
            kern_virt_base,
            kern_phys_base,
            kern_size,
            user_sp: VirBytes(minix_types::USER_STACK_TOP),
            kern_stack_top: VirBytes(kern_virt_base.0 + kern_size),
            syscall_entry: VirBytes(kern_virt_base.0),
        }
    }
}

impl Default for BootStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// The fixed list a `push_*` call targeted is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootStorageFull;

/// Borrowed semantic view of a [`BootStorage`] plus the kernel scalars.
///
/// This is what a v1 sender passes to the kernel entry. The kernel image's
/// [`crate::KernelInfo`] is the `'static` receiver-side form (backed by its
/// own `.bss`), not a sender type.
#[derive(Clone, Copy)]
pub struct BootReport<'a> {
    pub memmap: &'a [MemoryRegion],
    pub reserved_regions: &'a [MemoryRegion],
    pub boot_modules: &'a [BootModule],
    pub platform_sources: &'a [PlatformDescSource],
    pub param_buf: &'a [u8],
    /// Provenance records for the two map tables (the DTB leg carries them
    /// across the hop; the UEFI leg writes them during classification).
    pub memmap_sources: &'a [SourceRecord],
    /// Startup-object records (kernel + modules + bootstrap allocations).
    pub startup_objects: &'a [SourceRecord],
    pub kern_virt_base: VirBytes,
    pub kern_phys_base: PhysBytes,
    pub kern_size: u64,
    pub user_sp: VirBytes,
    pub kern_stack_top: VirBytes,
    pub syscall_entry: VirBytes,
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
        storage.memmap_slots_mut()[0] = region(0x1000, 0x2000);
        storage.set_memmap_len(1);
        storage.reserved_slots_mut()[0] = region(0x9000, 0x1000);
        storage.set_reserved_len(1);
        storage
            .push_module(module("vm", 0x30_0000, 0x1000))
            .unwrap();
        storage
            .push_platform_source(PlatformDescSource::new(DTB, PhysBytes(0x4000_0000)))
            .unwrap();
        storage.param_buf_mut()[..3].copy_from_slice(b"a=b");
        storage.set_param_len(3);

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
        assert_eq!(report.kern_stack_top.0, 0xFFFF_8000_0040_0000);
        assert_eq!(report.syscall_entry.0, 0xFFFF_8000_0000_0000);
        assert_eq!(report.user_sp.0, minix_types::USER_STACK_TOP);
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

    #[test]
    fn fixed_lists_reject_the_entry_past_capacity() {
        let mut storage = BootStorage::new();
        for i in 0..WIRE_MODULE_CAP {
            storage.push_module(module("vm", i as u64, 1)).unwrap();
        }
        assert_eq!(
            storage.push_module(module("vm", 0, 1)),
            Err(BootStorageFull),
            "module list must stop at its capacity"
        );
        for i in 0..WIRE_PLATFORM_CAP {
            storage
                .push_platform_source(PlatformDescSource::new(DTB, PhysBytes(i as u64)))
                .unwrap();
        }
        assert_eq!(
            storage.push_platform_source(PlatformDescSource::new(DTB, PhysBytes(0))),
            Err(BootStorageFull),
            "platform list must stop at its capacity"
        );
    }

    #[test]
    fn commit_map_tables_commits_both_prefixes_together() {
        let mut storage = BootStorage::new();
        storage.memmap_slots_mut()[0] = region(0x1000, 0x2000);
        storage.reserved_slots_mut()[0] = region(0x9000, 0x1000);
        storage.commit_map_tables(1, 1);
        let report = storage.report(
            VirBytes(0xFFFF_8000_0000_0000),
            PhysBytes(0x20_0000),
            0x40_0000,
        );
        assert_eq!(report.memmap.len(), 1);
        assert_eq!(report.reserved_regions.len(), 1);
    }

    #[test]
    #[should_panic(expected = "exceeds capacity")]
    fn committing_a_memmap_length_past_capacity_panics() {
        let mut storage = BootStorage::new();
        storage.set_memmap_len(WIRE_MEMMAP_CAP + 1);
    }

}
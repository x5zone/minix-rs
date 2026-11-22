//! KernelInfo — shared struct passed from boot-loader to kernel.
//!
//! Corresponds to Minix3's `kinfo_t` (minix/param.h, pre_init.c).
//! Stripped of multiboot-specific fields — UEFI provides memory map directly.

use minix_types::{PhysBytes, VirBytes};

use crate::memmap_source::{InputObjectId, RamAuthority, SourceAuditError, SourceRecord};
use crate::boot_storage::BootReport;
use crate::platform::PlatformDescSource;

// ── Data structures ──

// `KernelInfo` is an in-process value, not a cross-image ABI: after handoff
// v1 the sender writes `BootReport` into the wire and the receiver
// reconstructs this struct in its own `.bss`; neither image reads this struct
// from the other's memory. Deliberately no `#[repr(C)]`: field order is not a
// contract, and the `&'static [T]` fields' internal (ptr, len) order is
// toolchain-defined anyway.
#[derive(Clone, Copy)]
pub struct KernelInfo {
    /// Free physical memory regions — **not** pre-filtered.
    /// What lands here per leg: the kernel image is in this list on the
    /// self-boot legs (its LMAs are carved from conventional RAM) and absent
    /// on UEFI legs, where `reserve_kernel_span` claims its span as
    /// `LOADER_DATA` before any other allocation. Module blobs are
    /// in it on the U-Boot / direct-core legs (bump pages) but absent on UEFI
    /// legs (`LOADER_DATA`). Each consumer deducts the occupied ranges it must
    /// not touch (see `vm_handoff::classify` and `vm_boot_exclusions` for the
    /// per-consumer split).
    /// C: kinfo.memmap[] — pg_utils.c add_memmap/cut_memmap
    pub memmap: &'static [MemoryRegion],

    /// Memory the classifier marked `ReservedMapped` — non-conventional,
    /// firmware-retained, usable through the Direct Map but not allocatable.
    /// Uses:
    ///
    /// 1. Firmware allocations the kernel may reach through the Direct Map
    ///    (loader data, kernel stack, ACPI tables, LOADER_DATA allocations
    ///    including the bump region and module blobs). Consumed as DM coverage
    ///    source 4 (`dm_coverage.rs`).
    ///
    /// What each production leg passes here:
    ///
    /// - UEFI legs: non-empty — the pairing loop's five-step classification
    ///   routes `ReservedMapped` descriptors (LOADER_*/BOOT_SERVICES_*/
    ///   ACPI_RECLAIM with write-back) into this list; it is serialized as the
    ///   arena's reserved partition and decoded on the receive side.
    /// - U-Boot shim leg and riscv64 direct-core leg: non-empty — the DTB
    ///   collector is built with `firmware_reserve_end = KERN_PHYS_BASE`, so
    ///   the firmware prefix `[DRAM_BASE, KERN_PHYS_BASE)` is carved out here
    ///   (plus any DTB `/reserved-memory` entries).
    /// - aarch64 direct-core leg: built with `firmware_reserve_end = 0`, so
    ///   the prefix is empty; DTB `/reserved-memory` entries would still
    ///   appear, and the current test DTB has none.
    /// - Host tests: `&[]`.
    pub reserved_regions: &'static [MemoryRegion],

    /// Provenance records behind [`Self::memmap`] / [`Self::reserved_regions`]:
    /// one record per input declaration (or derived firmware prefix), naming
    /// the object and the offset inside it. All five production legs carry
    /// them across four writer sites (the two UEFI legs share the pairing
    /// loop's per-descriptor records, the U-Boot shim has one site, and the
    /// two direct-core faces have one each). A non-empty list is re-audited by
    /// [`Self::validate`], so the consumer boundary refuses to boot on a
    /// projected span that no record explains; surfaces that know the wire
    /// contract was in force call [`Self::validate_requiring_sources`], which
    /// also refuses an empty partition. Hand-built test kernels construct
    /// this field as `&[]` and use the lenient gate.
    pub source_records: &'static [SourceRecord],

    /// Kernel virtual base address.
    /// C: kinfo.vir_kern_start = &_kern_vir_base — pre_init.c:113
    pub kern_virt_base: VirBytes,

    /// Kernel physical base address.
    /// C: kern_phys_start = &_kern_phys_base — pg_utils.c linker symbol
    pub kern_phys_base: PhysBytes,

    /// Kernel total size in bytes.
    /// C: kern_kernlen = &_kern_size — pg_utils.c linker symbol
    /// Uses u64 instead of usize to avoid truncation on 32-bit targets.
    pub kern_size: u64,

    /// Boot process images (PM, VM, VFS, RS etc.).
    /// C: kinfo.module_list[] — pre_init.c memcpy from GRUB
    pub boot_modules: &'static [BootModule],

    /// Platform descriptor sources — opaque handles carrying firmware table
    /// pointers (DTB, RSDP, or both). Ordered by boot-shim's preference;
    /// the kernel takes the first source that parses successfully.
    ///
    /// Empty slice means boot-shim did not provide any — kernel falls back
    /// to `QemuVirtDesc` (dev) or panics (release). See 04-platform-discovery.md §4.6.
    ///
    /// # Cross-binary safety
    ///
    /// Each `PlatformDescSource` is pure data `(u32 kind, u32 len, u64 phys_addr)`
    /// — safe to pass from boot-shim to kernel even when they are separate
    /// ELF binaries. No function pointers cross the boundary. `len = 0`
    /// means the producer did not record the blob extent; the window builder
    /// then falls back to one 2 MiB piece.
    ///
    /// # DTB + RSDP coexistence
    ///
    /// Real-world ARM64 servers (SBBR) may provide both DTB and ACPI. The
    /// list supports this: boot-shim passes `[dtb_source, rsdp_source]` (or
    /// `[rsdp_source, dtb_source]` if ACPI preferred). The kernel tries
    /// each in order, using the first that parses successfully — matching
    /// Linux's `acpi=on/off/force` model.
    pub platform_sources: &'static [PlatformDescSource],

    /// Boot monitor parameter buffer — `key=value\0` pairs passed by the
    /// boot loader (multiboot command line, UEFI load options, or SBI
    /// boot hart argument). Returned to user space by `GET_INFO` with
    /// `info_type = GET_MONPARAMS` (`do_getinfo.c:143-146`).
    ///
    /// C: `char kinfo.param_buf[MULTIBOOT_PARAM_BUF_SIZE]` (param.h:28,
    /// `MULTIBOOT_PARAM_BUF_SIZE = 1024` — earm/multiboot.h:240).
    ///
    /// In Minix3 the buffer is populated by `mb_set_param()` in
    /// `arch/i386/pre_init.c:144` / `arch/earm/pre_init.c:269`, parsing the
    /// multiboot info structure's `cmdline` field into `key=value` pairs
    /// (e.g. `bootopts=...`, `memory=...`, `ht=...`).
    ///
    /// The Rust boot legs fill this slice from their real parameter source
    /// through [`crate::normalize_param_buf`]: the UEFI leg reads
    /// `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions`, the OpenSBI/direct legs read
    /// DTB `/chosen/bootargs`. A leg with no source passes `&[]`, which the
    /// kernel's `do_getinfo` MonParams handler treats as "no boot parameters"
    /// and copies zero bytes to the caller (matching C's behavior when
    /// `param_buf[0] == '\0'`).
    ///
    /// # Validation
    ///
    /// `KernelInfo::validate` asserts `param_buf.len() <= 1024` to match
    /// C's `MULTIBOOT_PARAM_BUF_SIZE`. Boot-shims that pass larger buffers
    /// violate the contract and trigger a fail-fast panic.
    pub param_buf: &'static [u8],

    /// The input object declared as this boot's RAM authority.
    ///
    /// Bound at construction, not at validation: the wire decoder copies
    /// `BootHandoffV1::ram_authority`, the direct-core legs pass
    /// [`RamAuthority::DTB`], and the UEFI legs pass [`RamAuthority::UEFI`]. [`Self::validate_requiring_sources`]
    /// reads this field instead of taking a caller-supplied parameter,
    /// so the projection and its authority cannot be paired by a
    /// separate argument at the consumer boundary.
    pub authority: RamAuthority,
}

/// Validation strictness for [`KernelInfo::validate_impl_inner`].
///
    /// Replaces the two-`bool` encoding that allowed four combinations
    /// but only three were semantically valid.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ValidationLevel {
    /// QEMU bootstrap test kernels: no boot modules, no source records.
    BeforeTables,
    /// Production gate: requires boot modules to be non-empty.
    Standard,
    /// Production + provenance records (strict gate). The authority is not
    /// carried here: it is already a field of `KernelInfo`, and keeping a
    /// second copy in the level would recreate the pair the no-argument
    /// entry point removed.
    Strict,
}

impl ValidationLevel {
    fn requires_boot_modules(self) -> bool {
        !matches!(self, Self::BeforeTables)
    }
    fn requires_kernel_span_coverage(self) -> bool {
        matches!(self, Self::Strict)
    }
    fn is_strict(self) -> bool {
        matches!(self, Self::Strict)
    }
}

/// Read-only view of the fields every `KernelInfo` validation step needs.
///
/// `KernelInfo` is the receiver-owned `'static` form; `BootReport<'_>` is the
/// sender-side borrowed form. Both feed the same validation code so the two
/// forms cannot drift.
pub trait KernelInfoView {
    fn memmap(&self) -> &[MemoryRegion];
    fn reserved_regions(&self) -> &[MemoryRegion];
    fn boot_modules(&self) -> &[BootModule];
    fn platform_sources(&self) -> &[PlatformDescSource];
    fn param_buf(&self) -> &[u8];
    fn source_records(&self) -> &[SourceRecord];
    fn kern_virt_base(&self) -> VirBytes;
    fn kern_phys_base(&self) -> PhysBytes;
    fn kern_size(&self) -> u64;
}

impl KernelInfoView for KernelInfo {
    fn memmap(&self) -> &[MemoryRegion] { self.memmap }
    fn reserved_regions(&self) -> &[MemoryRegion] { self.reserved_regions }
    fn boot_modules(&self) -> &[BootModule] { self.boot_modules }
    fn platform_sources(&self) -> &[PlatformDescSource] { self.platform_sources }
    fn param_buf(&self) -> &[u8] { self.param_buf }
    fn source_records(&self) -> &[SourceRecord] { self.source_records }
    fn kern_virt_base(&self) -> VirBytes { self.kern_virt_base }
    fn kern_phys_base(&self) -> PhysBytes { self.kern_phys_base }
    fn kern_size(&self) -> u64 { self.kern_size }
}

impl KernelInfoView for BootReport<'_> {
    fn memmap(&self) -> &[MemoryRegion] { self.memmap }
    fn reserved_regions(&self) -> &[MemoryRegion] { self.reserved_regions }
    fn boot_modules(&self) -> &[BootModule] { self.boot_modules }
    fn platform_sources(&self) -> &[PlatformDescSource] { self.platform_sources }
    fn param_buf(&self) -> &[u8] { self.param_buf }
    fn source_records(&self) -> &[SourceRecord] { self.source_records }
    fn kern_virt_base(&self) -> VirBytes { self.kern_virt_base }
    fn kern_phys_base(&self) -> PhysBytes { self.kern_phys_base }
    fn kern_size(&self) -> u64 { self.kern_size }
}

impl KernelInfo {
    /// The kernel's initial stack top, derived from the image span.
    ///
    /// Neither the wire nor this struct carries `kern_stack_top`: every
    /// producing leg computes it as `kern_virt_base + kern_size`
    /// (the loader places the boot stack at the image tail; `bootface`'s `.ld`
    /// pins `KERN_STACK_TOP` to the same expression), so carrying it across
    /// the hop would restate an arithmetic identity on both ends.
    ///
    /// # Panics
    ///
    /// Panics on overflow — a `kern_virt_base + kern_size` that wraps u64
    /// means the image span itself is corrupt, and every consumer below
    /// would otherwise work with a bogus stack pointer.
    pub fn kern_stack_top(&self) -> VirBytes {
        VirBytes(
            self.kern_virt_base
                .0
                .checked_add(self.kern_size)
                .expect("KernelInfo: kern_virt_base + kern_size overflows u64"),
        )
    }

    /// Validate kernel-info invariants. Called by `kmain` at entry.
    ///
    /// `KernelInfo` fields remain `pub` for cross-crate
    /// access (boot-shim constructs, kernel reads, tests assert), but this
    /// method enforces runtime invariants that the type system cannot:
    ///
    /// 1. `kern_size > 0` — boot-shim must have loaded a non-empty kernel.
    /// 2. `!memmap.is_empty()` — at least one free memory region required.
    /// 3. `!boot_modules.is_empty()` — at least PM/VM/VFS/RS required.
    ///
    /// # Panics
    ///
    /// Panics with a descriptive message if any invariant is violated.
    /// This is a fail-fast contract: boot-shim bugs that produce invalid
    /// `KernelInfo` are unrecoverable.
    pub fn validate(&self) {
        KernelInfo::validate_view(self, self.authority, &ValidationLevel::Standard);
    }

    /// Structural gate for entries that run before the first page-table
    /// allocation. Same invariants as [`Self::validate`] except the
    /// `boot_modules` non-empty check: QEMU bootstrap test kernels legally
    /// hand over an empty module list (they exercise the mapping machinery,
    /// not the init-server image). Production still runs the full
    /// [`Self::validate`] at `kmain`.
    pub fn validate_before_tables(&self) {
        KernelInfo::validate_view(self, self.authority, &ValidationLevel::BeforeTables);
    }

    /// Production handoff gate: exactly [`Self::validate`], plus the
    /// requirement that the boot leg actually carried provenance records.
    ///
    /// Every production leg writes one record per projected input unit, so an
    /// empty partition means a producer regression. Hand-built test kernels
    /// (`os/qemu-tests` fixtures) use [`Self::validate_before_tables`] or call
    /// `arch_boot_impl` directly; the strict gate is for surfaces that know
    /// the wire contract was in force.
    ///
    /// Strict production gate: the leg's declared RAM authority must have
    /// records, and those records must explain the projection. A missing
    /// authority is a fail-stop — never a fallback to the other input.
    pub fn validate_requiring_sources(&self) {
        KernelInfo::validate_view(self, self.authority, &ValidationLevel::Strict);
    }

    fn validate_view<V: KernelInfoView>(view: &V, authority: RamAuthority, level: &ValidationLevel) {
        // 1. Kernel size must be non-zero.
        assert!(
            view.kern_size() > 0,
            "KernelInfo: kern_size must be non-zero (boot-shim did not load kernel?)"
        );

        // 4. At least one free memory region.
        assert!(
            !view.memmap().is_empty(),
            "KernelInfo: memmap must not be empty (no free memory?)"
        );

        // Platform sources are physical pointers the kernel will dereference
        // before the Direct Map exists: reject null addresses, a zero kind
        // tag, and a blob span that overflows instead of letting the parser
        // fault on forged boot input (RD4B-DESIGN-D2-05).
        for (i, source) in view.platform_sources().iter().enumerate() {
            assert!(
                source.phys_addr().0 != 0,
                "KernelInfo: platform_sources[{i}] has a null physical address"
            );
            assert!(
                source.kind().raw() != 0,
                "KernelInfo: platform_sources[{i}] has kind 0 (not a registered source)"
            );
            source
                .phys_addr()
                .0
                .checked_add(source.len().map_or(0, |n| n.get()) as u64)
                .unwrap_or_else(|| {
                    panic!("KernelInfo: platform_sources[{i}] span overflows u64")
                });
        }

        // Ordered & pairwise disjoint (D01-R1 D2-05). The input assumption
        // is explicit (D01-R4 D2-03): UEFI GetMemoryMap publishes descriptors
        // in address order; DTB /memory's ordering is *not* spec-guaranteed,
        // so this repository accepts only the sorted subset — an unordered
        // (legal but pathological) DTB fails closed here rather than being
        // silently absorbed by one consumer and not another.
        for (list_name, list) in [("memmap", view.memmap()), ("reserved_regions", view.reserved_regions())] {
            for w in list.windows(2) {
                assert!(
                    w[0].base.0 < w[1].base.0,
                    "KernelInfo: {list_name} 未按基址升序（0x{:x} ≥ 0x{:x}）",
                    w[0].base.0,
                    w[1].base.0
                );
                let w_end = w[0]
                    .base
                    .0
                    .checked_add(w[0].len as u64)
                    .expect("KernelInfo: memory region span overflows u64");
                assert!(
                    w_end <= w[1].base.0,
                    "KernelInfo: {list_name} 区间相交（[0x{:x}, 0x{:x}) 与 0x{:x} 起）",
                    w[0].base.0,
                    w_end,
                    w[1].base.0
                );
            }
        }

        // Cross-table disjointness: a page that appears in
        // both the allocatable list and the never-reallocate list is a
        // producer bug that would let the PMM hand out a reserved frame.
        // Both lists are sorted (checked above), so a merge-walk suffices.
        {
            let (mut mi, mut ri) = (0usize, 0usize);
            while mi < view.memmap().len() && ri < view.reserved_regions().len() {
                let m_end = view.memmap()[mi]
                    .base
                    .0
                    .checked_add(view.memmap()[mi].len as u64)
                    .expect("KernelInfo: memmap span overflows u64");
                let r_end = view.reserved_regions()[ri]
                    .base
                    .0
                    .checked_add(view.reserved_regions()[ri].len as u64)
                    .expect("KernelInfo: reserved span overflows u64");
                if m_end <= view.reserved_regions()[ri].base.0 {
                    mi += 1;
                } else if r_end <= view.memmap()[mi].base.0 {
                    ri += 1;
                } else {
                    panic!(
                        "KernelInfo: memmap[{}] ([0x{:x},+0x{:x})) overlaps reserved_regions[{}] ([0x{:x},+0x{:x})) — producer bug",
                        mi, view.memmap()[mi].base.0, view.memmap()[mi].len,
                        ri, view.reserved_regions()[ri].base.0, view.reserved_regions()[ri].len,
                    );
                }
            }
        }

        // Kernel image vs module spans, and module-vs-module spans.
        // The producer asserts this before copying, but the
        // receive side re-checks the same half-open predicate: a hand-crafted
        // wire payload must not be able to place a module over the kernel it is
        // about to run. Touching is legal (`[a,b)` and `[b,c)`).
        {
            let kbase = view.kern_phys_base().0;
            // Release builds do not enable overflow checks; a forged
            // `kern_size` must fail here rather than wrap the end below the
            // base and turn the predicate into a tautology.
            let kend = kbase
                .checked_add(view.kern_size())
                .expect("KernelInfo: kernel span overflows u64");
            for (i, m) in view.boot_modules().iter().enumerate() {
                let mb = m.start.0;
                let me = mb
                    .checked_add(m.len as u64)
                    .expect("KernelInfo: boot module span overflows u64");
                assert!(
                    me <= kbase || mb >= kend,
                    "KernelInfo: boot_modules[{}] ({}) [0x{:x},0x{:x}) 与内核镜像 [0x{:x},0x{:x}) 相交",
                    i, m.name, mb, me, kbase, kend
                );
                for (j, prev) in view.boot_modules()[..i].iter().enumerate() {
                    let pb = prev.start.0;
                    let pe = pb
                        .checked_add(prev.len as u64)
                        .expect("KernelInfo: boot module span overflows u64");
                    assert!(
                        me <= pb || mb >= pe,
                        "KernelInfo: boot_modules[{}] ({}) [0x{:x},0x{:x}) 与 boot_modules[{}] ({}) [0x{:x},0x{:x}) 相交",
                        i, m.name, mb, me, j, prev.name, pb, pe
                    );
                }
            }
        }

        // 4a. All memory regions must be non-empty and page-aligned.
        //     DESIGN-09 (D01): zero-length or unaligned entries would be
        //     silently skipped by consumers, hiding boot-shim bugs.
        for (i, r) in view.memmap().iter().enumerate() {
            r.base.0
                .checked_add(r.len as u64)
                .unwrap_or_else(|| panic!("KernelInfo: memmap[{}] span overflows u64", i));
            assert!(r.len > 0, "KernelInfo: memmap[{}] has zero length", i);
            assert!(
                r.base.0 % 0x1000 == 0,
                "KernelInfo: memmap[{}] base {:#x} is not page-aligned",
                i, r.base.0
            );
            assert!(
                r.len % 0x1000 == 0,
                "KernelInfo: memmap[{}] length {:#x} is not page-aligned",
                i, r.len
            );
        }
        for (i, r) in view.reserved_regions().iter().enumerate() {
            r.base.0
                .checked_add(r.len as u64)
                .unwrap_or_else(|| panic!("KernelInfo: reserved_regions[{}] span overflows u64", i));
            assert!(r.len > 0, "KernelInfo: reserved_regions[{}] has zero length", i);
            assert!(
                r.base.0 % 0x1000 == 0,
                "KernelInfo: reserved_regions[{}] base {:#x} is not page-aligned",
                i, r.base.0
            );
            assert!(
                r.len % 0x1000 == 0,
                "KernelInfo: reserved_regions[{}] length {:#x} is not page-aligned",
                i, r.len
            );
        }

        // 5. At least one boot module (PM/VM/VFS/RS). Production-only:
        //    bootstrap test kernels hand over no init-server image.
        if level.requires_boot_modules() {
            assert!(
                !view.boot_modules().is_empty(),
                "KernelInfo: boot_modules must not be empty (no init servers?)"
            );
        }

        // 6. Boot parameter buffer must fit in MULTIBOOT_PARAM_BUF_SIZE.
        //    C: `char kinfo.param_buf[MULTIBOOT_PARAM_BUF_SIZE]` where
        //    MULTIBOOT_PARAM_BUF_SIZE = 1024 (earm/multiboot.h:240).
        //    The Rust variant uses a borrowed slice (`&'static [u8]`) so
        //    we enforce the same upper bound at validation time.
        const MULTIBOOT_PARAM_BUF_SIZE: usize = 1024;
        assert!(
            view.param_buf().len() <= MULTIBOOT_PARAM_BUF_SIZE,
            "KernelInfo: param_buf length {} exceeds MULTIBOOT_PARAM_BUF_SIZE ({})",
            view.param_buf().len(),
            MULTIBOOT_PARAM_BUF_SIZE
        );

        // 6. Source records: a strict (production) gate declares the leg's
        //    RAM authority and requires records from *that* input; a missing
        //    authority is a fail-stop, never a fallback to the other input.
        //    The lenient gate audits whatever records are present.
        if level.is_strict() {
            let authority = authority;
            // The wire carries no derived-value equalities to check: the
            // receiver derives `user_sp`, `syscall_entry` and
            // `kern_stack_top` from `USER_STACK_TOP` / `kern_virt_base` /
            // `kern_virt_base + kern_size`, so there is no wire value left to
            // forge.
            // The module name is positional identity, not a display label:
            // a forged wire must not pair position 6's image with position 7's
            // name.
            for (i, module) in view.boot_modules().iter().enumerate() {
                assert!(
                    i < MODULE_NAMES.len() && module.name == MODULE_NAMES[i],
                    "KernelInfo: boot_modules[{i}] name {:?} does not match the canonical position {:?}",
                    module.name,
                    MODULE_NAMES.get(i)
                );
            }
            assert!(
                !view.source_records().is_empty(),
                "KernelInfo: production handoff carries no memory-map source records"
            );
            assert!(
                view.source_records()
                    .iter()
                    .any(|r| r.input_object == authority.input_object()),
                "KernelInfo: declared RAM authority {authority:?} has no source records — refusing to fall back to another input"
            );
            match crate::memmap_source::audit_source_coverage_for(
                view.memmap(),
                view.reserved_regions(),
                view.source_records(),
                Some(authority.input_object()),
            ) {
                Ok(()) => {}
                Err(error) => panic!(
                    "KernelInfo: declared RAM authority {authority:?} does not cover the projected map: {error:?}"
                ),
            }
        } else if !view.source_records().is_empty() {
            match crate::memmap_source::audit_source_coverage(
                view.memmap(),
                view.reserved_regions(),
                view.source_records(),
            ) {
                Ok(()) => {}
                Err(error) => {
                    panic!("KernelInfo: source records do not cover the projected map: {error:?}")
                }
            }
        }

        // Kernel-image coverage: at least one record must cover the kernel
        // image's physical span. Every production leg writes
        // it; a missing record means a producer regression, not an empty-list
        // corner case.
        if level.requires_kernel_span_coverage() {
            // The kernel span must be covered by a `memmap` or
            // `reserved_regions` span. All five legs satisfy this (UEFI legs
            // claim the span as `ReservedMapped`; the U-Boot shim and the two
            // direct-core legs get it from the DTB `/memory` projection).
            let kern_lo = view.kern_phys_base().0;
            let kern_hi = kern_lo
                .checked_add(view.kern_size())
                .expect("KernelInfo: kernel span overflows u64");
            let covered = |spans: &[MemoryRegion]| {
                spans.iter().any(|r| {
                    let end = r.base.0.saturating_add(r.len as u64);
                    r.base.0 <= kern_lo && kern_hi <= end
                })
            };
            assert!(
                covered(view.memmap()) || covered(view.reserved_regions()),
                "KernelInfo: the kernel image span [{kern_lo:#x}, {kern_hi:#x}) must lie in a memmap or reserved_regions span (v10 derives this from the map tables)"
            );
        }
    }

    /// One-line description of the audit failure for diagnostics, or `None`
    /// when the records cover both projections.
    pub fn source_audit(&self) -> Option<SourceAuditError> {
        if self.source_records.is_empty() {
            return None;
        }
        crate::memmap_source::audit_source_coverage(
            self.memmap,
            self.reserved_regions,
            self.source_records,
        )
        .err()
    }

    /// **Boot-postcondition check shared by every boot form.**
    ///
    /// The five boot forms each produce `KernelInfo` from a
    /// different firmware surface, so their absolute addresses differ; what
    /// can be checked in one place is the relation every form must satisfy.
    /// That relation is the coarse lower bound `kern_virt_base > u32::MAX`
    /// (the kernel must not sit below 4 GiB). The derived scalars `user_sp` /
    /// `syscall_entry` / `kern_stack_top` are not wire fields, so there is no
    /// equality to check; the per-architecture user/kernel split is carried
    /// by the layout constants, not by this gate.
    ///
    /// Complementary to [`Self::validate`]: `validate` checks non-address
    /// invariants (kern_size/alignment/non-empty/param_buf bound), while
    /// this method checks the image's address lower bound, so every form's
    /// construction site calls both in order.
    ///
    /// # Panics
    ///
    /// Any failing postcondition fails fast (there is no recovery surface
    /// during boot; same weight as `validate`).
    pub fn check_boot_postconditions(&self) {
        check_boot_postconditions_view(self);
    }
}

fn check_boot_postconditions_view<V: KernelInfoView>(view: &V) {
    // Only the lower bound is checked here. A check on
    // `kern_virt_base + kern_size` would recompute the same expression as
    // a tautology, and a `USER_STACK_TOP` check would compare a
    // compile-time constant with itself. The kernel image must sit above
    // 4 GiB (u32::MAX): a coarse lower bound, not the
    // per-architecture user/kernel split (the layout constants carry
    // the latter).
    //
    // The syscall-entry placement check has one authority:
    // `syscall_entry_va()`.
    assert!(
        view.kern_virt_base().0 > u64::from(u32::MAX),
        "KernelInfo postcond: kern_virt_base (0x{:x}) 不大于 u32::MAX（4 GiB 下限）",
        view.kern_virt_base().0
    );
}

impl BootReport<'_> {
    /// Validate the sender's borrowed view with the same invariants as the
    /// receiver-owned `KernelInfo`. `authority` is supplied by the leg because
    /// the report itself carries no authority field.
    pub fn validate(&self, authority: RamAuthority) {
        KernelInfo::validate_view(self, authority, &ValidationLevel::Standard);
    }

    pub fn validate_before_tables(&self, authority: RamAuthority) {
        KernelInfo::validate_view(self, authority, &ValidationLevel::BeforeTables);
    }

    pub fn validate_requiring_sources(&self, authority: RamAuthority) {
        KernelInfo::validate_view(self, authority, &ValidationLevel::Strict);
    }

    pub fn check_boot_postconditions(&self) {
        check_boot_postconditions_view(self);
    }
}

/// Boot wire / semantic-table entry for one physical region.
///
/// This is the external representation shared by `KernelInfo`, the v1
/// handoff wire, VM classification and DM coverage. Its `base + len`
/// shape, wire width and zero-length/overflow handling are protocol
/// contract; do not replace it with the internal `PhysRange` type.
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: PhysBytes,
    pub len: usize,
}

/// Number of user-space boot modules the kernel expects the boot-shim to
/// deliver, in the `KernelInfo::boot_modules` field.
///
/// This is the **shared boot-module count contract** between the two
/// independent consumers, both of which depend on this crate:
/// - boot-shim `MODULE_NAMES` must have exactly this many entries
///   (`os/boot-shim/src/loader.rs` asserts it at compile time).
/// - the kernel's `BOOT_MODULE_PROC_NRS` map is typed
///   `&[ProcNr; NR_BOOT_MODULES]`, so its length is structurally pinned here
///   (`os/kernel/src/proc.rs`).
///
/// Before this constant lived here, both sides carried their own literal `12`
/// and a change on one side would not fail to compile on the other. Housing
/// the count in this crate makes such a drift a compile error in *both* crates
/// at once. It locks the **count** only — the *order* remains a positional
/// contract that boot-shim's `MODULE_NAMES` must match against the kernel's
/// `BOOT_MODULE_PROC_NRS` (authoritative C source:
/// `minix3/minix/kernel/table.c:44-64` `image[]` user-space section).
///
/// C: `NR_BOOT_MODULES` — the user-module tail of `image[NR_BOOT_PROCS]`.
pub const NR_BOOT_MODULES: usize = 12;

/// Canonical boot-module order and names (DESIGN-02).
///
/// This is the shared positional contract: boot-shim loads files in this
/// order, the kernel binds positions to `BOOT_MODULE_PROC_NRS`, and the strict
/// gate compares each `BootModule.name` against this table. The array length
/// pins the count at compile time, so a count change fails both sides.
pub const MODULE_NAMES: [&str; NR_BOOT_MODULES] = [
    "ds", "rs", "pm", "sched", "vfs", "memory", "tty", "mib", "vm", "pfs", "mfs", "init",
];

#[derive(Debug, Clone, Copy)]
pub struct BootModule {
    pub name: &'static str,
    pub start: PhysBytes,
    pub len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a valid KernelInfo for testing.
    fn make_valid_info() -> KernelInfo {
        KernelInfo {
            memmap: &[MemoryRegion {
                base: PhysBytes(0x100000),
                len: 0x1000000,
            }],
            reserved_regions: &[MemoryRegion {
                base: PhysBytes(0x1da00000),
                len: 0x1000,
            }],
            source_records: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200000),
            kern_size: 0x200000,
            boot_modules: &[BootModule {
                name: "ds",
                start: PhysBytes(0x400000),
                len: 0x100000,
            }],
            platform_sources: &[],
            param_buf: &[],
            authority: RamAuthority::DTB,
        }
    }

    static NULL_PLATFORM_SOURCE: [PlatformDescSource; 1] = [PlatformDescSource::new(
        crate::platform::RSDP,
        PhysBytes(0),
    )];
    static ZERO_KIND_PLATFORM_SOURCE: [PlatformDescSource; 1] = [PlatformDescSource::new(
        crate::platform::PlatformDescKind::new(0),
        PhysBytes(0x1000),
    )];

    #[test]
    #[should_panic(expected = "null physical address")]
    fn platform_source_null_address_rejected() {
        let mut info = make_valid_info();
        info.platform_sources = &NULL_PLATFORM_SOURCE;
        info.validate();
    }

    #[test]
    #[should_panic(expected = "kind 0")]
    fn platform_source_zero_kind_rejected() {
        let mut info = make_valid_info();
        info.platform_sources = &ZERO_KIND_PLATFORM_SOURCE;
        info.validate();
    }

    /// Records that explain both projections: one `/memory` record for the
    /// conventional span and one `reserved-memory` record for the reserved
    /// span.
    static COVERING_RECORDS: [SourceRecord; 2] = [
        SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: crate::memmap_source::SourceKind::Memory,
            origin: crate::memmap_source::SourceOrigin::MemoryNode { node: 0, reg: 0 },
            range: crate::memmap_source::SourceRange::new(0x100000, 0x1100000),
        },
        SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 1,
            raw_type: 0,
            kind: crate::memmap_source::SourceKind::ReservedOrdinary,
            origin: crate::memmap_source::SourceOrigin::ReservedMemoryChild { node: 0, reg: 0 },
            range: crate::memmap_source::SourceRange::new(0x1da00000, 0x1da01000),
        },
    ];

    #[test]
    fn test_validate_accepts_covering_source_records() {
        let mut info = make_valid_info();
        info.source_records = &COVERING_RECORDS;
        info.validate();
        assert!(info.source_audit().is_none());
    }

    /// The consumer-boundary audit is fail-closed: dropping the only record
    /// of the reserved span leaves it unexplained, and validation refuses to
    /// proceed instead of carrying a span with no provenance.
    #[test]
    #[should_panic(expected = "source records do not cover the projected map")]
    fn test_validate_rejects_span_without_source_record() {
        let mut info = make_valid_info();
        info.source_records = &COVERING_RECORDS[..1];
        info.validate();
    }

    /// The strict gate turns "no records" into a boot refusal: an empty
    /// partition would otherwise skip the coverage audit silently.
    #[test]
    #[should_panic(expected = "carries no memory-map source records")]
    fn test_validate_requiring_sources_rejects_an_empty_partition() {
        make_valid_info().validate_requiring_sources();
    }

    #[test]
    fn test_validate_requiring_sources_accepts_covering_records() {
        let mut info = make_valid_info();
        info.source_records = &COVERING_RECORDS;
        // v10 (D-062): the strict gate no longer needs a startup record; it
        // now requires the kernel span to lie in a memmap or reserved span.
        // `make_valid_info`'s map already covers [0x100000, 0x1100000) +
        // [0x1da00000, +0x1000), and kern_phys_base 0x200000 + 0x200000 lands
        // inside the first one, so the call below passes on the map tables
        // alone.
        info.validate_requiring_sources();
    }

    // v10 (D-062) deleted the derived `kern_stack_top` field and its
    // "stack top past the image end" relation; the remaining postcondition
    // negative control is `test_boot_postconditions_reject_low_virtual_base`
    // below.



    /// D01-R1 D2-05: an out-of-order memmap violates the firmware delivery
    /// contract and must fail fast at validate.
    #[test]
    #[should_panic(expected = "未按基址升序")]
    fn test_validate_rejects_unordered_memmap() {
        static UNORDERED: [MemoryRegion; 2] = [
            MemoryRegion { base: PhysBytes(0x200_000), len: 0x10_000 },
            MemoryRegion { base: PhysBytes(0x100_000), len: 0x10_000 },
        ];
        let mut info = make_valid_info();
        info.memmap = &UNORDERED;
        info.validate();
    }

    /// D01-R1 D2-05: overlapping memmap entries are equally impossible.
    #[test]
    #[should_panic(expected = "区间相交")]
    fn test_validate_rejects_overlapping_memmap() {
        static OVERLAP: [MemoryRegion; 2] = [
            MemoryRegion { base: PhysBytes(0x100_000), len: 0x20_000 },
            MemoryRegion { base: PhysBytes(0x110_000), len: 0x10_000 },
        ];
        let mut info = make_valid_info();
        info.memmap = &OVERLAP;
        info.validate();
    }

    /// R16 DESIGN-10: a page that appears in both memmap and reserved_regions
    /// would let the PMM hand out a reserved frame. The cross-table check
    /// must catch it.
    #[test]
    #[should_panic(expected = "overlaps reserved_regions")]
    fn test_validate_rejects_memmap_reserved_overlap() {
        static CONV: [MemoryRegion; 1] = [
            MemoryRegion { base: PhysBytes(0x100_000), len: 0x20_000 },
        ];
        static RES: [MemoryRegion; 1] = [
            MemoryRegion { base: PhysBytes(0x110_000), len: 0x4_000 },
        ];
        let mut info = make_valid_info();
        info.memmap = &CONV;
        info.reserved_regions = &RES;
        info.validate();
    }

    /// R18 DESIGN-D2-10: the receive side re-checks kernel-vs-module spans so
    /// a forged payload cannot hand the kernel a module over its own image.
    #[test]
    #[should_panic(expected = "与内核镜像")]
    fn test_validate_rejects_module_overlapping_kernel_image() {
        static OVER_KERNEL: [BootModule; 1] = [BootModule {
            name: "vm",
            start: PhysBytes(0x300_000),
            len: 0x20_000,
        }];
        let mut info = make_valid_info();
        info.boot_modules = &OVER_KERNEL;
        info.validate();
    }

    #[test]
    #[should_panic(expected = "相交")]
    fn test_validate_rejects_modules_overlapping_each_other() {
        static OVERLAP: [BootModule; 2] = [
            BootModule { name: "vm", start: PhysBytes(0x400_000), len: 0x20_000 },
            BootModule { name: "pm", start: PhysBytes(0x410_000), len: 0x20_000 },
        ];
        let mut info = make_valid_info();
        info.boot_modules = &OVERLAP;
        info.validate();
    }

    #[test]
    #[should_panic(expected = "length 0x1001 is not page-aligned")]
    fn test_validate_rejects_unaligned_memmap_len() {
        static UNALIGNED: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(0x1000),
            len: 0x1001,
        }];
        let mut info = make_valid_info();
        info.memmap = &UNALIGNED;
        info.reserved_regions = &[];
        info.validate();
    }

    #[test]
    #[should_panic(expected = "reserved span overflows")]
    fn test_validate_rejects_reserved_span_overflow() {
        static OVERFLOW: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(u64::MAX - 1),
            len: 4,
        }];
        let mut info = make_valid_info();
        info.reserved_regions = &OVERFLOW;
        info.validate();
    }

    #[test]
    #[should_panic(expected = "memmap[0] span overflows")]
    fn test_validate_rejects_memmap_span_overflow() {
        static OVERFLOW: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(u64::MAX - 1),
            len: 4,
        }];
        let mut info = make_valid_info();
        info.memmap = &OVERFLOW;
        info.reserved_regions = &[];
        info.validate();
    }

    #[test]
    #[should_panic(expected = "kernel span overflows")]
    fn test_validate_rejects_kernel_span_overflow() {
        let mut info = make_valid_info();
        info.kern_size = u64::MAX;
        info.validate();
    }

    #[test]
    #[should_panic(expected = "boot module span overflows")]
    fn test_validate_rejects_module_span_overflow() {
        static OVERFLOW: [BootModule; 1] = [BootModule {
            name: "vm",
            start: PhysBytes(u64::MAX - 1),
            len: 4,
        }];
        let mut info = make_valid_info();
        info.boot_modules = &OVERFLOW;
        info.validate();
    }

    /// D01-F07: without a startup record covering the kernel image, the
    /// strict production gate fails closed even when source records cover
    /// the memory projections.

    /// v10 (D-062) replacement for the deleted startup-record test: the
    /// kernel span must lie in a memmap or reserved span, so a map that
    /// covers neither must fail the strict gate.
    #[test]
    #[should_panic(expected = "must lie in a memmap or reserved_regions span")]
    fn test_validate_requiring_sources_rejects_kernel_span_outside_map() {
        let mut info = make_valid_info();
        info.source_records = &COVERING_RECORDS;
        // Point the kernel outside both tables (base 0x40000000, size 0x200000).
        info.kern_phys_base = PhysBytes(0x4000_0000);
        info.validate_requiring_sources();
    }

    /// A declared authority with no records is a fail-stop; it must not fall
    /// back to the other input even when that input's records are present.
    #[test]
    #[should_panic(expected = "declared RAM authority")]
    fn test_validate_requiring_sources_refuses_missing_authority() {
        let mut info = make_valid_info();
        info.source_records = &COVERING_RECORDS; // all DtbBootInput
        info.authority = RamAuthority::UEFI;
        info.validate_requiring_sources();
    }




    #[test]
    #[should_panic(expected = "does not match the canonical position")]
    fn test_validate_requiring_sources_rejects_misnamed_module_position() {
        static WRONG_NAME: [BootModule; 1] = [BootModule {
            name: "mib",
            start: PhysBytes(0x400000),
            len: 0x100000,
        }];
        let mut info = make_valid_info();
        info.boot_modules = &WRONG_NAME;
        info.validate_requiring_sources();
    }

    #[test]
    fn test_validate_accepts_valid_info() {
        // A valid KernelInfo should pass validate() without panic.
        make_valid_info().validate();
    }

    #[test]
    #[should_panic(expected = "kern_size must be non-zero")]
    fn test_validate_rejects_zero_kern_size() {
        let mut info = make_valid_info();
        info.kern_size = 0;
        info.validate();
    }


    #[test]
    #[should_panic(expected = "memmap must not be empty")]
    fn test_validate_rejects_empty_memmap() {
        let mut info = make_valid_info();
        info.memmap = &[];
        info.validate();
    }

    #[test]
    #[should_panic(expected = "boot_modules must not be empty")]
    fn test_validate_rejects_empty_boot_modules() {
        let mut info = make_valid_info();
        info.boot_modules = &[];
        info.validate();
    }

    #[test]
    #[should_panic(expected = "exceeds MULTIBOOT_PARAM_BUF_SIZE")]
    fn test_validate_rejects_oversized_param_buf() {
        // P9-1: param_buf length must not exceed MULTIBOOT_PARAM_BUF_SIZE (1024).
        // We cannot construct a `&'static [u8]` of length 1025 at compile time
        // without a `const` slice; instead we use a static buffer and slice it.
        static OVERFLOW_BUF: [u8; 1025] = [0; 1025];
        let mut info = make_valid_info();
        info.param_buf = &OVERFLOW_BUF[..];
        info.validate();
    }

    #[test]
    fn test_validate_accepts_max_param_buf() {
        // P9-1: param_buf length == 1024 (exactly MULTIBOOT_PARAM_BUF_SIZE) is OK.
        static MAX_BUF: [u8; 1024] = [0; 1024];
        let mut info = make_valid_info();
        info.param_buf = &MAX_BUF[..];
        info.validate();
    }

    #[test]
    #[should_panic(expected = "不大于 u32::MAX")]
    fn test_boot_postconditions_reject_low_virtual_base() {
        // v10 (D-062) removed the three derived-scalar relations this gate used
        // to read; the remaining assertion is `kern_virt_base > u32::MAX`.
        // This is its negative control (the positive side is exercised by
        // every boot path and by `make_valid_info`).
        let mut info = make_valid_info();
        info.kern_virt_base = VirBytes(0x1000_0000);
        info.check_boot_postconditions();
    }
}

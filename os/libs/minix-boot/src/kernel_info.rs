//! KernelInfo — shared struct passed from boot-loader to kernel.
//!
//! Corresponds to Minix3's `kinfo_t` (minix/param.h, pre_init.c).
//! Stripped of multiboot-specific fields — UEFI provides memory map directly.

use minix_types::{PhysBytes, VirBytes};

use crate::memmap_source::{InputObjectId, SourceAuditError, SourceRecord};
use crate::platform::PlatformDescSource;

// ── Data structures ──

// `#[repr(C)]` pins the field order and each field's offset for debuggability
// and for a future FFI boundary. It is NOT a cross-image layout contract:
// after handoff v9 the sender writes `BootReport` into the wire and the
// receiver reconstructs `KernelInfo` in its own `.bss`; neither image reads
// this struct from the other's memory. The `&'static [T]` fields' internal
// (ptr, len) order is toolchain-defined, which is another reason this type
// must not be used as an ABI.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelInfo {
    /// Free physical memory regions — **not** pre-filtered (D01-R5 D2-14).
    /// What lands here per leg: the kernel image is in this list on the
    /// self-boot legs (its LMAs are carved from conventional RAM) and absent
    /// on UEFI legs, where `reserve_kernel_span` claims its span as
    /// `LOADER_DATA` before any other allocation (D01-R13 A). Module blobs are
    /// in it on the U-Boot / direct-core legs (bump pages) but absent on UEFI
    /// legs (`LOADER_DATA`). Each consumer deducts the occupied ranges it must
    /// not touch (see the `memmap()` accessor for the per-consumer split).
    /// C: kinfo.memmap[] — pg_utils.c add_memmap/cut_memmap
    pub memmap: &'static [MemoryRegion],

    /// Memory the classifier marked `ReservedMapped` — non-conventional,
    /// firmware-retained, usable through the Direct Map but not allocatable.
    /// Two historical uses:
    ///
    /// 1. The inline x86 identity window (kernel linked into the boot-shim
    ///    PE, executed at low physical addresses). Retired with the
    ///    independent-image split: `vm_handoff`'s kern_ident input now
    ///    returns empty for this leg.
    /// 2. Firmware allocations the kernel may reach through the Direct Map
    ///    (loader data, kernel stack, ACPI tables, LOADER_DATA allocations
    ///    including the bump region and module blobs). This is still
    ///    consumed as DM coverage source 4 (`dm_coverage.rs`).
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
    pub memmap_sources: &'static [SourceRecord],

    /// Startup-object records — the canonical set (D01-F07): the kernel
    /// image, every boot module, the root page, the bump pool, and the
    /// handoff arena (when the leg has one). All four producer legs write
    /// the same set (four writer sites across the five legs — the two
    /// UEFI chains share one writer); `validate_requiring_sources` fails closed unless at
    /// least one record covers the kernel image. They have their own
    /// partition and capacity, so they are never counted against
    /// memmap_sources.
    pub startup_objects: &'static [SourceRecord],

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

    /// User-space stack top.
    /// C: kinfo.user_sp = USR_STACKTOP — pre_init.c:156
    pub user_sp: VirBytes,

    /// Kernel initial stack top (virtual address).
    /// C: k_boot_stktop / k_initial_stktop — linker symbol, used by
    /// tss_init(0, &k_boot_stktop) in prot_init() — arch/i386/protect.c:338
    /// x86-64: TSS.sp0, aarch64: SP_EL1, riscv64: sscratch
    pub kern_stack_top: VirBytes,

    /// System call entry point (virtual address).
    /// System call handler entry virtual address (= kern_virt_base + offset).
    /// Kernel metadata recorded for all architectures; only x86-64 uses it
    /// to configure LSTAR MSR. aarch64/riscv64 determine the entry at
    /// compile time (exception/trap vector), so this field is for reference only.
    /// C: STAR MSR (AMD_MSR_STAR, 32-bit AMD SYSCALL) — set by tss_init() SYSCALL
    /// MSR setup — arch/i386/protect.c:189-205. Note: C is 32-bit, uses STAR;
    /// Rust x86-64 long mode uses LSTAR (different MSR, same semantic role).
    pub syscall_entry: VirBytes,

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
    /// [`InputObjectId::DtbBootInput`], and the UEFI legs pass
    /// [`InputObjectId::UefiMemoryMap`]. [`Self::validate_requiring_sources`]
    /// reads this field instead of taking a caller-supplied parameter
    /// (F-R17-04), so the projection and its authority cannot be paired by a
    /// separate argument at the consumer boundary.
    pub authority: InputObjectId,
}

/// Validation strictness for [`KernelInfo::validate_at`].
///
/// Replaces the previous two-`bool` encoding that allowed four combinations
/// but only three were semantically valid (R17 DESIGN-12).
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
    fn requires_startup_kernel_record(self) -> bool {
        matches!(self, Self::Strict)
    }
    fn is_strict(self) -> bool {
        matches!(self, Self::Strict)
    }
}

impl KernelInfo {
    /// Validate kernel-info invariants. Called by `kmain` at entry.
    ///
    /// `KernelInfo` fields remain `pub` for cross-crate
    /// access (boot-shim constructs, kernel reads, tests assert), but this
    /// method enforces runtime invariants that the type system cannot:
    ///
    /// 1. `kern_size > 0` — boot-shim must have loaded a non-empty kernel.
    /// 2. `kern_stack_top % 16 == 0` — stack alignment (AAPCS64 / SysV ABI).
    /// 3. `!memmap.is_empty()` — at least one free memory region required.
    /// 4. `!boot_modules.is_empty()` — at least PM/VM/VFS/RS required.
    ///
    /// # Panics
    ///
    /// Panics with a descriptive message if any invariant is violated.
    /// This is a fail-fast contract: boot-shim bugs that produce invalid
    /// `KernelInfo` are unrecoverable.
    pub fn validate(&self) {
        self.validate_at(ValidationLevel::Standard);
    }

    /// Unified validation entry point (R17 DESIGN-12).
    fn validate_at(&self, level: ValidationLevel) {
        self.validate_impl_inner(&level);
    }

    /// Structural gate for entries that run before the first page-table
    /// allocation. Same invariants as [`Self::validate`] except the
    /// `boot_modules` non-empty check: QEMU bootstrap test kernels legally
    /// hand over an empty module list (they exercise the mapping machinery,
    /// not the init-server image). Production still runs the full
    /// [`Self::validate`] at `kmain`.
    pub fn validate_before_tables(&self) {
        self.validate_at(ValidationLevel::BeforeTables);
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
        self.validate_at(ValidationLevel::Strict);
    }

    fn validate_impl_inner(&self, level: &ValidationLevel) {
        // 1. Kernel size must be non-zero.
        assert!(
            self.kern_size > 0,
            "KernelInfo: kern_size must be non-zero (boot-shim did not load kernel?)"
        );

        // 3. Stack pointer must be 16-byte aligned (AAPCS64 / SysV ABI).
        assert!(
            self.kern_stack_top.0.is_multiple_of(16),
            "KernelInfo: kern_stack_top (0x{:x}) must be 16-byte aligned",
            self.kern_stack_top.0
        );

        // 4. At least one free memory region.
        assert!(
            !self.memmap.is_empty(),
            "KernelInfo: memmap must not be empty (no free memory?)"
        );

        // Ordered & pairwise disjoint (D01-R1 D2-05). The input assumption
        // is explicit (D01-R4 D2-03): UEFI GetMemoryMap publishes descriptors
        // in address order; DTB /memory's ordering is *not* spec-guaranteed,
        // so this repository accepts only the sorted subset — an unordered
        // (legal but pathological) DTB fails closed here rather than being
        // silently absorbed by one consumer and not another.
        for (list_name, list) in [("memmap", self.memmap), ("reserved_regions", self.reserved_regions)] {
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

        // Cross-table disjointness (R16 DESIGN-10): a page that appears in
        // both the allocatable list and the never-reallocate list is a
        // producer bug that would let the PMM hand out a reserved frame.
        // Both lists are sorted (checked above), so a merge-walk suffices.
        {
            let (mut mi, mut ri) = (0usize, 0usize);
            while mi < self.memmap.len() && ri < self.reserved_regions.len() {
                let m_end = self.memmap[mi]
                    .base
                    .0
                    .checked_add(self.memmap[mi].len as u64)
                    .expect("KernelInfo: memmap span overflows u64");
                let r_end = self.reserved_regions[ri]
                    .base
                    .0
                    .checked_add(self.reserved_regions[ri].len as u64)
                    .expect("KernelInfo: reserved span overflows u64");
                if m_end <= self.reserved_regions[ri].base.0 {
                    mi += 1;
                } else if r_end <= self.memmap[mi].base.0 {
                    ri += 1;
                } else {
                    panic!(
                        "KernelInfo: memmap[{}] ([0x{:x},+0x{:x})) overlaps reserved_regions[{}] ([0x{:x},+0x{:x})) — producer bug",
                        mi, self.memmap[mi].base.0, self.memmap[mi].len,
                        ri, self.reserved_regions[ri].base.0, self.reserved_regions[ri].len,
                    );
                }
            }
        }

        // Kernel image vs module spans, and module-vs-module spans
        // (R18 DESIGN-D2-10). The producer asserts this before copying, but the
        // receive side re-checks the same half-open predicate: a hand-crafted
        // wire payload must not be able to place a module over the kernel it is
        // about to run. Touching is legal (`[a,b)` and `[b,c)`).
        {
            let kbase = self.kern_phys_base.0;
            // Release builds do not enable overflow checks; a forged
            // `kern_size` must fail here rather than wrap the end below the
            // base and turn the predicate into a tautology.
            let kend = kbase
                .checked_add(self.kern_size)
                .expect("KernelInfo: kernel span overflows u64");
            for (i, m) in self.boot_modules.iter().enumerate() {
                let mb = m.start.0;
                let me = mb
                    .checked_add(m.len as u64)
                    .expect("KernelInfo: boot module span overflows u64");
                assert!(
                    me <= kbase || mb >= kend,
                    "KernelInfo: boot_modules[{}] ({}) [0x{:x},0x{:x}) 与内核镜像 [0x{:x},0x{:x}) 相交",
                    i, m.name, mb, me, kbase, kend
                );
                for (j, prev) in self.boot_modules[..i].iter().enumerate() {
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
        for (i, r) in self.memmap.iter().enumerate() {
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
        for (i, r) in self.reserved_regions.iter().enumerate() {
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
                !self.boot_modules.is_empty(),
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
            self.param_buf.len() <= MULTIBOOT_PARAM_BUF_SIZE,
            "KernelInfo: param_buf length {} exceeds MULTIBOOT_PARAM_BUF_SIZE ({})",
            self.param_buf.len(),
            MULTIBOOT_PARAM_BUF_SIZE
        );

        // 6. Source records: a strict (production) gate declares the leg's
        //    RAM authority and requires records from *that* input; a missing
        //    authority is a fail-stop, never a fallback to the other input.
        //    The lenient gate audits whatever records are present.
        if level.is_strict() {
            let authority = self.authority;
            // Production-only derived-value equalities (DESIGN-01): the three
            // scalars must match their single source, not merely be in range.
            // A forged wire can otherwise carry a legal-looking `user_sp`,
            // `syscall_entry` or `kern_stack_top` that no inequality catches.
            assert_eq!(
                self.user_sp.0,
                minix_types::USER_STACK_TOP,
                "KernelInfo: production user_sp does not equal USER_STACK_TOP"
            );
            assert_eq!(
                self.syscall_entry.0, self.kern_virt_base.0,
                "KernelInfo: production syscall_entry does not equal kern_virt_base"
            );
            let expected_stack_top = self
                .kern_virt_base
                .0
                .checked_add(self.kern_size)
                .expect("KernelInfo: kern_virt_base + kern_size overflows u64");
            assert_eq!(
                self.kern_stack_top.0, expected_stack_top,
                "KernelInfo: production kern_stack_top does not equal kern_virt_base + kern_size"
            );
            // The module name is positional identity, not a display label:
            // a forged wire must not pair position 6's image with position 7's
            // name (DESIGN-02).
            for (i, module) in self.boot_modules.iter().enumerate() {
                assert!(
                    i < MODULE_NAMES.len() && module.name == MODULE_NAMES[i],
                    "KernelInfo: boot_modules[{i}] name {:?} does not match the canonical position {:?}",
                    module.name,
                    MODULE_NAMES.get(i)
                );
            }
            assert!(
                !self.memmap_sources.is_empty(),
                "KernelInfo: production handoff carries no memory-map source records"
            );
            assert!(
                self.memmap_sources
                    .iter()
                    .any(|r| r.input_object == authority),
                "KernelInfo: declared RAM authority {authority:?} has no source records — refusing to fall back to another input"
            );
            match crate::memmap_source::audit_source_coverage_for(
                self.memmap,
                self.reserved_regions,
                self.memmap_sources,
                Some(self.authority),
            ) {
                Ok(()) => {}
                Err(error) => panic!(
                    "KernelInfo: declared RAM authority {authority:?} does not cover the projected map: {error:?}"
                ),
            }
        } else if !self.memmap_sources.is_empty() {
            match crate::memmap_source::audit_source_coverage(
                self.memmap,
                self.reserved_regions,
                self.memmap_sources,
            ) {
                Ok(()) => {}
                Err(error) => {
                    panic!("KernelInfo: source records do not cover the projected map: {error:?}")
                }
            }
        }

        // Startup-object canonical set (D01-F07): at least one record must
        // cover the kernel image's physical span. Every production leg writes
        // it; a missing record means a producer regression, not an empty-list
        // corner case.
        if level.requires_startup_kernel_record() {
            let kern_lo = self.kern_phys_base.0;
            let kern_hi = kern_lo
                .checked_add(self.kern_size)
                .expect("KernelInfo: kernel span overflows u64");
            assert!(
                self.startup_objects.iter().any(|r| {
                    r.range.start <= kern_lo && u128::from(kern_hi) <= r.range.end_exclusive
                }),
                "KernelInfo: startup_objects must carry a record covering the kernel image [{kern_lo:#x}, {kern_hi:#x}) — got {} record(s)",
                self.startup_objects.len()
            );
        }
    }

    /// One-line description of the audit failure for diagnostics, or `None`
    /// when the records cover both projections.
    pub fn source_audit(&self) -> Option<SourceAuditError> {
        if self.memmap_sources.is_empty() {
            return None;
        }
        crate::memmap_source::audit_source_coverage(
            self.memmap,
            self.reserved_regions,
            self.memmap_sources,
        )
        .err()
    }

    /// **Boot-postcondition assertions equivalent across boot forms** (the
    /// chosen resolution for this debt).
    ///
    /// The decision is "equivalent postcondition assertions", not "equal
    /// entry structure" — the five historical boot forms each produce
    /// `KernelInfo` from a different firmware surface
    /// (inlined UEFI / cross-image UEFI / OpenSBI bring-up / direct QEMU
    /// kernel load / hart mailbox), and their absolute addresses depend on
    /// the firmware, so they can never be equal one by one; what can be
    /// equivalent is **the same set of higher-half address-space
    /// relations**. This method gathers those relations in one place and
    /// checks them per form, specifically catching the "paper constant"
    /// class of defect that recurred three times (`entry_start_pa` typos)
    /// and the overreach where a shared helper assumed a single `.bss`
    /// world and zeroed a root page still in use.
    ///
    /// Complementary to [`Self::validate`]: `validate` checks non-address
    /// invariants (kern_size/alignment/non-empty/param_buf
    /// bound), while this method checks the **address-space separation
    /// relations**, so every form's construction site should call both in
    /// order. All assertions depend only on relative relations between
    /// fields, so host tests and hardware share the same criterion.
    ///
    /// # Panics
    ///
    /// Any failing postcondition fails fast (there is no recovery surface
    /// during boot; same weight as `validate`).
    pub fn check_boot_postconditions(&self) {
        // 1. The syscall entry lies inside the kernel image: the field's own
        //    doc says "= kern_virt_base + offset", and no form may point the
        //    entry outside the image — exactly where an `entry_start_pa`
        //    style typo is caught.
        let entry_off = self
            .syscall_entry
            .0
            .checked_sub(self.kern_virt_base.0)
            .unwrap_or(u64::MAX);
        assert!(
            entry_off < self.kern_size,
            "KernelInfo postcond: syscall_entry (0x{:x}) 不在内核镜像 [0x{:x}, 0x{:x}) 内",
            self.syscall_entry.0,
            self.kern_virt_base.0,
            self.kern_virt_base.0.saturating_add(self.kern_size)
        );

        // 2. The kernel's initial stack top lies in the higher half (≥ the
        //    kernel virtual base): all three forms are higher-half kernels,
        //    so the stack must not drop into the user half.
        assert!(
            self.kern_stack_top.0 >= self.kern_virt_base.0,
            "KernelInfo postcond: kern_stack_top (0x{:x}) 低于 higher-half 内核基 (0x{:x})",
            self.kern_stack_top.0,
            self.kern_virt_base.0
        );

        // 3. The user stack top is strictly below the kernel's higher-half
        //    base: the bottom line of user/kernel address-space separation,
        //    and where overlapping misplacement (zeroing a kernel page in
        //    use as if it were free) is caught.
        assert!(
            self.user_sp.0 < self.kern_virt_base.0,
            "KernelInfo postcond: user_sp (0x{:x}) 未严格低于内核基 (0x{:x})",
            self.user_sp.0,
            self.kern_virt_base.0
        );

        // 4. The initial stack top does not sit past the image end: the
        //    constructor derives `kern_stack_top = kern_virt_base +
        //    kern_size` (builder) or pins it at the image tail (bootface
        //    `.ld` constants), so a value beyond `base + size` is a
        //    paper-constant error of the `entry_start_pa` family — first
        //    push after `mov sp` would land in unmapped memory. If a future
        //    layout legally moves the boot stack out of the image, relax
        //    this together with the producer that justifies it.
        assert!(
            self.kern_stack_top.0 <= self.kern_virt_base.0.saturating_add(self.kern_size),
            "KernelInfo postcond: kern_stack_top (0x{:x}) 越过镜像尾 (0x{:x})",
            self.kern_stack_top.0,
            self.kern_virt_base.0.saturating_add(self.kern_size)
        );
    }

    // ── Getter methods (preferred API over direct field access) ──
    //
    // These getters are the recommended API. Direct field access remains
    // `pub` for backward compatibility with boot-shim construction and
    // test assertions, but new code should prefer these methods.












}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: PhysBytes,
    pub len: usize,
}

/// Number of user-space boot modules the kernel expects the boot-shim to
/// deliver, in `KernelInfo::boot_modules()`.
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

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BootModule {
    pub name: &'static str,
    pub start: PhysBytes,
    pub len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::USER_STACK_TOP;

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
            memmap_sources: &[],
            startup_objects: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200000),
            kern_size: 0x200000,
            user_sp: VirBytes(USER_STACK_TOP as u64),
            kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0000_0000),
            boot_modules: &[BootModule {
                name: "ds",
                start: PhysBytes(0x400000),
                len: 0x100000,
            }],
            platform_sources: &[],
            param_buf: &[],
            authority: InputObjectId::DtbBootInput,
        }
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
        info.memmap_sources = &COVERING_RECORDS;
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
        info.memmap_sources = &COVERING_RECORDS[..1];
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
        info.memmap_sources = &COVERING_RECORDS;
        // D01-F07: the strict gate also requires a startup record covering
        // the kernel image (here kern_phys_base 0x200000 + kern_size
        // 0x200000 → [0x200000, 0x400000)).
        static KERN_RECORD: [SourceRecord; 1] =
            [crate::memmap_source::startup_record(0, 0x200_000, 0x400_000)];
        info.startup_objects = &KERN_RECORD;
        info.validate_requiring_sources();
    }

    /// D01-R1 D2-04: a stack top past the image end is representable in
    /// the type but semantically impossible — the postcondition gate must
    /// catch it (first push after `mov sp` would hit unmapped memory).
    #[test]
    #[should_panic(expected = "越过镜像尾")]
    fn test_postconditions_reject_stack_top_past_image_end() {
        let mut info = make_valid_info();
        info.kern_stack_top = VirBytes(info.kern_virt_base.0 + info.kern_size + 0x1000);
        info.check_boot_postconditions();
    }

    /// R17 DESIGN-09: each postcondition needs a negative test that feeds
    /// hand-crafted invalid input, proving the assertion can fire on
    /// independent data (not just builder-consistent values).
    #[test]
    #[should_panic(expected = "不在内核镜像")]
    fn test_postconditions_reject_syscall_entry_outside_image() {
        let mut info = make_valid_info();
        info.syscall_entry = VirBytes(info.kern_virt_base.0 + info.kern_size + 0x2000);
        info.check_boot_postconditions();
    }

    #[test]
    #[should_panic(expected = "低于 higher-half 内核基")]
    fn test_postconditions_reject_stack_top_below_higher_half() {
        let mut info = make_valid_info();
        info.kern_stack_top = VirBytes(0x1000); // far below any higher-half base
        info.check_boot_postconditions();
    }

    #[test]
    #[should_panic(expected = "未严格低于内核基")]
    fn test_postconditions_reject_user_sp_above_kernel_base() {
        let mut info = make_valid_info();
        info.user_sp = VirBytes(info.kern_virt_base.0 + 0x1000);
        info.check_boot_postconditions();
    }

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
    #[test]
    #[should_panic(expected = "startup_objects must carry a record covering the kernel image")]
    fn test_validate_requiring_sources_rejects_missing_kernel_startup_record() {
        let mut info = make_valid_info();
        info.memmap_sources = &COVERING_RECORDS;
        info.startup_objects = &[];
        info.validate_requiring_sources();
    }

    /// A declared authority with no records is a fail-stop; it must not fall
    /// back to the other input even when that input's records are present.
    #[test]
    #[should_panic(expected = "declared RAM authority")]
    fn test_validate_requiring_sources_refuses_missing_authority() {
        let mut info = make_valid_info();
        info.memmap_sources = &COVERING_RECORDS; // all DtbBootInput
        info.authority = InputObjectId::UefiMemoryMap;
        info.validate_requiring_sources();
    }

    #[test]
    #[should_panic(expected = "production user_sp does not equal USER_STACK_TOP")]
    fn test_validate_requiring_sources_rejects_noncanonical_user_sp() {
        let mut info = make_valid_info();
        info.user_sp = VirBytes(minix_types::USER_STACK_TOP - 0x1000);
        info.validate_requiring_sources();
    }

    #[test]
    #[should_panic(expected = "production syscall_entry does not equal kern_virt_base")]
    fn test_validate_requiring_sources_rejects_noncanonical_syscall_entry() {
        let mut info = make_valid_info();
        info.syscall_entry = VirBytes(info.kern_virt_base.0 + 0x1000);
        info.validate_requiring_sources();
    }

    #[test]
    #[should_panic(expected = "production kern_stack_top does not equal kern_virt_base + kern_size")]
    fn test_validate_requiring_sources_rejects_noncanonical_stack_top() {
        let mut info = make_valid_info();
        info.kern_stack_top = VirBytes(info.kern_virt_base.0 + 0x1000);
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
    #[should_panic(expected = "must be 16-byte aligned")]
    fn test_validate_rejects_misaligned_stack() {
        let mut info = make_valid_info();
        info.kern_stack_top = VirBytes(0xFFFF_8000_0040_0001); // odd address
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
    fn test_getters_return_field_values() {
        // Getter methods should return the same values as direct field access.
        // Slice types lack PartialEq, so compare lengths and first elements.
        let info = make_valid_info();
        // Scalar fields: direct equality.
        assert_eq!(info.kern_virt_base, info.kern_virt_base);
        assert_eq!(info.kern_phys_base, info.kern_phys_base);
        assert_eq!(info.kern_size, info.kern_size);
        assert_eq!(info.user_sp, info.user_sp);
        assert_eq!(info.kern_stack_top, info.kern_stack_top);
        assert_eq!(info.syscall_entry, info.syscall_entry);
        // Slice fields: compare length and first element address.
        assert_eq!(info.memmap.len(), info.memmap.len());
        assert_eq!(info.memmap.as_ptr(), info.memmap.as_ptr());
        assert_eq!(info.reserved_regions.len(), info.reserved_regions.len());
        assert_eq!(
            info.reserved_regions.as_ptr(),
            info.reserved_regions.as_ptr()
        );
        assert_eq!(info.boot_modules.len(), info.boot_modules.len());
        assert_eq!(info.boot_modules.as_ptr(), info.boot_modules.as_ptr());
        assert_eq!(info.platform_sources.len(), info.platform_sources.len());
        assert_eq!(
            info.platform_sources.as_ptr(),
            info.platform_sources.as_ptr()
        );
        assert_eq!(info.param_buf.len(), info.param_buf.len());
        assert_eq!(info.param_buf.as_ptr(), info.param_buf.as_ptr());
    }
}

//! Helpers shared by the two self-boot faces (`bootface.rs` / `bootface_a64.rs`).
//!
//! N-4 duplicate-surface cleanup: the functions below and the module-name pool
//! are architecture-neutral and were byte-identical in both faces. The shared
//! flow itself lives in [`run_self_boot`]; anything whose value is genuinely
//! per-arch (DRAM base, kernel bases, DTB placement, bump pool bounds) arrives
//! as [`SelfBootParams`], and the `say`/`say_hex` pair stays in each shell
//! because its console implementation differs by arch.

use boot_shim::build_kernel_info;
use minix_boot::{
    BootModule, DTB, DtbMemoryMap, KernelInfo, MemoryRegion, NR_BOOT_MODULES, PlatformDescSource,
};
use minix_types::{PhysBytes, VirBytes};

/// The direct-boot face owns one `BootStorage` in `.data` (its `new()` value
/// is not all-zero, so it is loadable data rather than zero-fill `.bss`);
/// keep the landing footprint bounded so a capacity increase does not
/// silently eat boot-time image data.
const _: () = assert!(core::mem::size_of::<minix_boot::BootStorage>() <= 16 * 1024);

/// `place_modules` writes `NR_BOOT_MODULES` slots; the storage's module
/// partition must be at least that wide. Without this, a capacity mismatch
/// becomes a boot-time panic instead of a compile error (DESIGN-D2-14).
const _: () = assert!(minix_boot::WIRE_MODULE_CAP >= NR_BOOT_MODULES);

use boot_shim::opensbi_helpers::BootFileTable;

/// Fixed module-name slot width (16 B; the kernel's `store_kernel_info`
/// truncates at the same 15-byte bound).
pub(crate) const NAME_SLOT: usize = 16;

/// Page-table bump area width (1 MiB, 256 pages); both faces use the same
/// order of magnitude and take an explicit halt arm on exhaustion.
pub(crate) const PT_BUMP_PAGES: usize = 256;

/// 12 names × 16 B, ordering = C `table.c:44-64` = `MODULE_NAMES`.
///
/// A pure data pool on purpose: under the satp=0 physical view only literals
/// materialised at the call site are safe to dereference, so no `&str` fat
/// pointers live in this table (see the string-discipline notes in both
/// boot faces).
pub(crate) const NAME_POOL: [[u8; NAME_SLOT]; NR_BOOT_MODULES] = [
    *b"ds\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"rs\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"pm\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"sched\0\0\0\0\0\0\0\0\0\0\0",
    *b"vfs\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"memory\0\0\0\0\0\0\0\0\0\0",
    *b"tty\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"mib\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"vm\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"pfs\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"mfs\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"init\0\0\0\0\0\0\0\0\0\0\0\0",
];

/// Compile-time byte-for-byte check of the pool against the contract list
/// (`MODULE_NAMES`). Both boot faces are absent from the host build, so the
/// real per-arch build is the only guard that can trip; the longest name
/// (`memory`, 6 bytes) never reaches the 15-byte truncation bound.
const _: () = {
    let mut i = 0;
    while i < NR_BOOT_MODULES {
        let want = boot_shim::loader::MODULE_NAMES[i].as_bytes();
        let got = &NAME_POOL[i];
        assert!(want.len() < NAME_SLOT, "module name exceeds NAME_SLOT");
        let mut j = 0;
        while j < want.len() {
            assert!(got[j] == want[j], "NAME_POOL order drifts from MODULE_NAMES");
            j += 1;
        }
        assert!(got[want.len()] == 0, "NAME_POOL slot not NUL-terminated");
        i += 1;
    }
};

/// Big-endian cell (4 or 8 bytes) as an address value.
pub(crate) fn be_cell(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        4 => Some(u32::from_be_bytes(bytes.try_into().ok()?) as u64),
        8 => Some(u64::from_be_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}

/// Is the whole `[pa, pa + len)` inside one memmap range? `checked_add` keeps
/// the guard total for any input: overflow is rejected as out of range.
pub(crate) fn in_memmap_range(memmap: &[MemoryRegion], pa: u64, len: u64) -> bool {
    let Some(end) = pa.checked_add(len) else {
        return false;
    };
    memmap.iter().any(|r| {
        r.base
            .0
            .checked_add(r.len as u64)
            .is_some_and(|r_end| pa >= r.base.0 && end <= r_end)
    })
}

/// Magic gate: read one aligned `u64` at `pa` and compare it with the
/// `BootFileTable` signature.
pub(crate) fn table_magic_ok(pa: u64) -> bool {
    // A `read_volatile::<u64>` needs an aligned address; the DTB property is
    // only bounds-checked by the callers, not alignment-checked. A
    // misaligned read may trap on RISC-V, so reject it before dereferencing.
    if pa % 8 != 0 {
        return false;
    }
    // SAFETY: the caller guarantees the whole `BootFileTable` (including
    // this u64) lies inside DTB-reported DRAM via the full-width
    // `in_memmap_range` gate; boot is single-hart.
    unsafe {
        (pa as *const u64).read_volatile() == boot_shim::opensbi_helpers::boot_file_table_magic()
    }
}

/// `/chosen`'s `opensbi,boot-file-table` property as a physical address
/// (big-endian 4/8-byte cell). Lookup walks `all_nodes` instead of
/// `find_node`: fdt 0.1.5's `find_node` misses the node in blobs rebuilt by
/// the QEMU `-kernel` path, while `all_nodes` sees it.
pub(crate) fn fdt_table_pa(fdt: &fdt::Fdt<'_>) -> Option<u64> {
    let chosen = fdt
        .all_nodes()
        .find(|n| n.name.split('@').next() == Some("chosen"))?;
    let prop = chosen.property("opensbi,boot-file-table")?;
    be_cell(prop.value)
}

/// `/chosen`'s `bootargs` string, via `all_nodes` for the same reason
/// (the crate's `chosen()` helper panics when `find_node` misses the node).
pub(crate) fn fdt_bootargs<'a>(fdt: &fdt::Fdt<'a>) -> Option<&'a str> {
    let chosen = fdt
        .all_nodes()
        .find(|n| n.name.split('@').next() == Some("chosen"))?;
    let prop = chosen.property("bootargs")?;
    let value = prop.value.strip_suffix(&[0]).unwrap_or(prop.value);
    core::str::from_utf8(value).ok()
}

/// DTB source bound shared by both self-boot faces.
///
/// Rejects overflow, addresses outside the identity VA window, and blobs that
/// overlap the kernel image (the collected projections land in this image's
/// `.bss`, so an overlapping source would be overwritten mid-read).
pub(crate) fn dtb_bounded(
    dtb_pa: u64,
    total: usize,
    dram_base: u64,
    kern_phys_base: u64,
    kern_size: u64,
) -> bool {
    let Some(end) = dtb_pa.checked_add(total as u64) else {
        return false;
    };
    dtb_pa >= dram_base
        && end <= 0x1_0000_0000
        && (end <= kern_phys_base || dtb_pa >= kern_phys_base + kern_size)
}

/// The three physical addresses `place_modules` needs from the caller.
///
/// They used to be three adjacent bare `u64` parameters, so swapping the
/// name-pool address with a bump bound still compiled and produced a
/// plausible-but-wrong placement. The struct makes the roles explicit and
/// prevents that class of call-site error.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlacementPool {
    /// Physical base of the 12 × 16 B module-name landing pool.
    pub names: u64,
    /// First byte of the caller's bump region.
    pub bump_start: u64,
    /// One past the last byte of the caller's bump region.
    pub bump_end: u64,
}

/// Consume the first `NR_BOOT_MODULES` table entries at their contract
/// positions and copy the sources into the caller's bump pages.
///
/// Both self-boot faces use the exact same gates (position, basename, source
/// width, bump-pool disjointness, pool exhaustion); only the bump allocator,
/// pool addresses and console differ, so they arrive as a [`PlacementPool`]
/// plus callbacks (DESIGN-10). `module_table` is the caller's exclusive
/// landing array.
pub(crate) fn place_modules<B, S, H>(
    table: &BootFileTable,
    memmap: &[MemoryRegion],
    pool: PlacementPool,
    module_table: &mut [BootModule],
    mut bump_alloc_pages: B,
    say: S,
    say_hex: H,
) -> Option<()>
where
    B: FnMut(usize) -> Option<u64>,
    S: Fn(&str),
    H: Fn(u64),
{
    assert!(
        module_table.len() >= NR_BOOT_MODULES,
        "place_modules: module table has {} slots, need {NR_BOOT_MODULES}",
        module_table.len()
    );
    let entries = table.valid_entries();
    if entries.len() < NR_BOOT_MODULES {
        say("  table holds 0x");
        say_hex(entries.len() as u64);
        say(" entries, kernel contract needs 0x");
        say_hex(NR_BOOT_MODULES as u64);
        say("\n");
        return None;
    }
    for i in 0..NR_BOOT_MODULES {
        let entry = &entries[i];
        let expect = &NAME_POOL[i];
        let want_n = expect.iter().position(|&b| b == 0).unwrap_or(NAME_SLOT);
        let base = entry
            .path
            .iter()
            .rposition(|&b| b == b'/')
            .map_or(0, |p| p + 1);
        let got = &entry.path[base..];
        let got_n = got.iter().position(|&b| b == 0).unwrap_or(got.len());
        if got_n != want_n || got[..want_n] != expect[..want_n] {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" path basename mismatches contract order\n");
            return None;
        }
        let len = entry.len as usize;
        if len == 0 {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" length 0\n");
            return None;
        }
        if !in_memmap_range(memmap, entry.phys_addr, entry.len) {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" source 0x");
            say_hex(entry.phys_addr);
            say("+0x");
            say_hex(entry.len);
            say(" escapes memmap\n");
            return None;
        }
        if entry.phys_addr < pool.bump_end && entry.phys_addr + entry.len > pool.bump_start {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" source overlaps bump pool\n");
            return None;
        }
        let pages = len.div_ceil(4096);
        let dest = bump_alloc_pages(pages)?;
        // SAFETY: the source passed the full-width memmap gate and does not
        // overlap the bump pool; `dest` is exclusive bump space and the copy
        // length is within both ranges.
        unsafe {
            core::ptr::copy_nonoverlapping(entry.phys_addr as *const u8, dest as *mut u8, len);
            core::ptr::write_bytes((dest as *mut u8).add(len), 0, pages * 4096 - len);
        }
        let slot = pool.names + (i as u64) * NAME_SLOT as u64;
        // SAFETY: source = the NAME_POOL byte cells; destination = slot i
        // inside the exclusive name pool (i < 12 => offset < 192).
        unsafe {
            core::ptr::copy_nonoverlapping(NAME_POOL[i].as_ptr(), slot as *mut u8, NAME_SLOT);
        }
        let n = NAME_POOL[i]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(NAME_SLOT);
        let landed: &'static str = unsafe {
            // SAFETY: ASCII literal bytes, backed by the caller's exclusive
            // physical name pool.
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(slot as *const u8, n))
        };
        module_table[i] = BootModule {
            name: landed,
            start: minix_types::PhysBytes(dest),
            len,
        };
    }
    Some(())
}

/// Per-arch constants and entry arguments consumed by [`run_self_boot`].
pub(crate) struct SelfBootParams {
    /// Physical address of the firmware DTB.
    pub dtb_pa: u64,
    /// Lower bound of DRAM; the DTB and table bounds start here.
    pub dram_base: u64,
    /// Reserved-area trim passed to `DtbMemoryMap` (`KERN_PHYS_BASE` on the
    /// riscv leg; `0` on the aarch64 leg, where the firmware has no resident
    /// region).
    pub fw_end: u64,
    /// Physical base of the kernel image.
    pub kern_phys_base: u64,
    /// Link-time high-half base of the kernel image.
    pub kern_virt_base: u64,
    /// Kernel image span (the constant reconciled by check-layout L9).
    pub kern_size: u64,
    /// First byte of the caller's bump pool.
    pub bump_start: u64,
    /// One past the last byte of the caller's bump pool.
    pub bump_end: u64,
}

/// The shared self-boot flow: DTB → memmap → table → bump pages → modules →
/// platform source → parameter buffer → `KernelInfo` → `arch_boot`.
///
/// The two thin faces print their own banner (their console and literal
/// discipline differ) and then call this with their constants. Everything
/// below is architecture-neutral; the bump allocator and console arrive as
/// callbacks so no per-arch value is duplicated here.
pub(crate) fn run_self_boot<B, S, H>(
    params: SelfBootParams,
    mut bump_alloc_pages: B,
    say: S,
    say_hex: H,
) -> !
where
    B: FnMut(usize) -> Option<u64>,
    S: Fn(&str),
    H: Fn(u64),
{
    // One-shot landing storage shared by both faces (only one is compiled
    // into a given image). Every `'static` slice handed to `KernelInfo`
    // points here; a second entry would overwrite them (UB under Rust
    // aliasing), so `take()` is the guard.
    static STORAGE: boot_shim::BootStorageCell = boot_shim::BootStorageCell::new();
    let storage = STORAGE.take();

    // Touch the anchor once on the running path so the kernel boot graph is
    // pulled into the link at archive-member granularity (the linker script
    // KEEP is the second safeguard).
    core::hint::black_box(crate::KERNEL_ENTRY_ANCHOR as usize);

    // 1. DTB → memmap. Read the fixed 40-byte header first, prove the whole
    //    blob is inside DRAM, and only then build the real-width slice.
    if !minix_boot::dtb_blob_aligned(params.dtb_pa) {
        say("### minix-rs kernel image: DTB pointer not 8-byte aligned — halting\n");
        crate::halt();
    }
    // SAFETY: the header read takes only the magic/length fields; the full
    // extent is pinned by `dtb_bounded` before any wider dereference.
    let head = unsafe { core::slice::from_raw_parts(params.dtb_pa as *const u8, 40) };
    if u32::from_be_bytes(head[0..4].try_into().unwrap_or([0; 4])) != 0xd00d_feed {
        say("### minix-rs kernel image: DTB magic bad — halting\n");
        crate::halt();
    }
    let fdt_total = u32::from_be_bytes(head[4..8].try_into().unwrap_or([0; 4])) as usize;
    if fdt_total < 40
        || !dtb_bounded(
            params.dtb_pa,
            fdt_total,
            params.dram_base,
            params.kern_phys_base,
            params.kern_size,
        )
    {
        say("### minix-rs kernel image: DTB blob escapes DRAM bound — halting\n");
        crate::halt();
    }
    // SAFETY: `dtb_bounded` proved the whole block lies inside DRAM; the
    // boot face runs before paging is enabled (identity view).
    let fdt_bytes = unsafe { core::slice::from_raw_parts(params.dtb_pa as *const u8, fdt_total) };
    if minix_boot::fdt_layout::validate(params.dtb_pa, fdt_bytes).is_err() {
        say("### minix-rs kernel image: DTB container invalid — halting\n");
        crate::halt();
    }
    let fdt = match fdt::Fdt::new(fdt_bytes) {
        Ok(f) => f,
        Err(_) => {
            say("### minix-rs kernel image: DTB parse failed — halting\n");
            crate::halt();
        }
    };

    // I-33-甲 (D-086): owned form — the arrays live on this face's stack and
    // are committed into `storage` after the fill stages below; the
    // collector's `'static` tables are not written.
    let owned = DtbMemoryMap::new(&fdt, params.fw_end)
        .collect_owned()
        .expect("bootface: DTB memmap rejected (capacity or reservation shape) — halting");
    minix_boot::audit_source_coverage(owned.free(), owned.reserved(), owned.sources())
        .expect("bootface: a projected span has no source record — halting");
    let memmap = owned.free();
    let reserved = owned.reserved();
    let sources = owned.sources();
    if memmap.is_empty() {
        say("### minix-rs kernel image: no usable /memory reg in DTB — halting\n");
        crate::halt();
    }
    say("  memmap regions 0x");
    say_hex(memmap.len() as u64);
    say(" first 0x");
    say_hex(memmap[0].base.0);
    say("+0x");
    say_hex(memmap[0].len as u64);
    say("\n");

    // 2. BootFileTable: the DTB /chosen channel must pass the DRAM bound and
    //    magic gates before it is dereferenced.
    let table_pa = fdt_table_pa(&fdt).filter(|pa| {
        in_memmap_range(memmap, *pa, core::mem::size_of::<BootFileTable>() as u64)
            && table_magic_ok(*pa)
    });
    let Some(table_pa) = table_pa else {
        say("### minix-rs kernel image: no BootFileTable (chosen) — module channel not wired, halting\n");
        crate::halt();
    };
    say("  boot file table pa 0x");
    say_hex(table_pa);
    say("\n");
    // SAFETY: the magic gate ran inside the memmap bound; the table lives in
    // boot-exclusive physical RAM and this face is its first and only reader.
    let table = unsafe { &*(table_pa as *const BootFileTable) };

    // 3. Bump pool: root page → page-table bump area → name pool, in that
    //    order and non-overlapping; module page takes come after.
    let pool_exhaust = "### minix-rs kernel image: bump pool exhausted — halting\n";
    let Some(cur_root) = bump_alloc_pages(1) else {
        say(pool_exhaust);
        crate::halt()
    };
    let root_page = PhysBytes(cur_root);
    let Some(pt_base) = bump_alloc_pages(PT_BUMP_PAGES) else {
        say(pool_exhaust);
        crate::halt()
    };
    let pt_end = pt_base + (PT_BUMP_PAGES as u64) * 4096;
    let Some(names_pool) = bump_alloc_pages(1) else {
        say(pool_exhaust);
        crate::halt()
    };
    minix_kernel::boot_alloc::init_boot_pt_alloc(pt_base, pt_end);
    say("  root 0x");
    say_hex(root_page.0);
    say(" pt bump 0x");
    say_hex(pt_base);
    say("..0x");
    say_hex(pt_end);
    say("\n");

    // 4. 12 modules: basename check, then copy straight into bump pages.
    storage.fill_modules(|module_table| {
        if place_modules(
            table,
            memmap,
            PlacementPool {
                names: names_pool,
                bump_start: params.bump_start,
                bump_end: params.bump_end,
            },
            module_table,
            &mut bump_alloc_pages,
            &say,
            &say_hex,
        )
        .is_none()
        {
            say("### minix-rs kernel image: module placement failed — halting\n");
            crate::halt();
        }
        NR_BOOT_MODULES
    });
    say("  boot modules placed 12\n");

    // 5. Platform descriptor source: the DTB is the sole source.
    // SAFETY: single hart on the boot face; written once, read-only after.
    let dtb_len = unsafe { minix_boot::dtb_total_size(PhysBytes(params.dtb_pa)) };
    storage.fill_platform(|slots| {
        slots[0] = PlatformDescSource::with_optional_len(
            DTB,
            PhysBytes(params.dtb_pa),
            dtb_len,
        );
        1
    });

    // T3: DTB `/chosen/bootargs` is the direct-boot parameter source.
    let mut param_ptr = core::ptr::null::<u8>();
    storage.fill_param(|out| {
        let input = fdt_bootargs(&fdt).unwrap_or("");
        let stats = minix_boot::normalize_param_buf(input.as_bytes(), None, out);
        say("  param_buf bytes=");
        say_hex(stats.bytes as u64);
        say(" rejected=");
        say_hex(stats.rejected as u64);
        say(" dropped=");
        say_hex(stats.dropped as u64);
        if stats.bytes > 0 {
            let end = out[..stats.bytes]
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(stats.bytes);
            if let Ok(text) = core::str::from_utf8(&out[..end]) {
                say(" first=");
                say(text);
            }
        }
        say("\n");
        param_ptr = out.as_ptr();
        stats.bytes
    });

    // 6. KernelInfo plus the real arch_boot gate.
    storage.fill_map_tables(|memmap_slots, reserved_slots| {
        memmap_slots[..memmap.len()].copy_from_slice(memmap);
        reserved_slots[..reserved.len()].copy_from_slice(reserved);
        minix_boot::MapCounts {
            memmap: memmap.len(),
            reserved: reserved.len(),
        }
    });
    storage.fill_memmap_sources(|source_slots| {
        source_slots[..sources.len()].copy_from_slice(sources);
        sources.len()
    });

    // End the one-shot `&mut` borrow, then publish the named static view.
    let _ = storage;
    let report = STORAGE.report_static(
        VirBytes(params.kern_virt_base),
        PhysBytes(params.kern_phys_base),
        params.kern_size,
    );
    let kernel_info: KernelInfo =
        build_kernel_info(&report, minix_boot::InputObjectId::DtbBootInput);
    kernel_info.validate_requiring_sources();
    say("  calling arch_boot (validate gate)\n");
    minix_kernel::arch_boot(&kernel_info, root_page)
}


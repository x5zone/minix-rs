//! Helpers shared by the two self-boot faces (`bootface.rs` / `bootface_a64.rs`).
//!
//! N-4 duplicate-surface cleanup: the functions below and the module-name pool
//! are architecture-neutral and were byte-identical in both faces. Anything
//! whose value is genuinely per-arch (DRAM base, kernel physical base, DTB
//! placement, bump pool bounds, the `place_modules` glue over the local bump
//! allocator) stays in its own file with a cross-reference; so does the
//! `say`/`say_hex` pair, whose `early_console` implementation differs by arch.

use minix_boot::{NR_BOOT_MODULES, BootModule, MemoryRegion};

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


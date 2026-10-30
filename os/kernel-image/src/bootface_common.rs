//! Helpers shared by the two self-boot faces (`bootface.rs` / `bootface_a64.rs`).
//!
//! N-4 duplicate-surface cleanup: the functions below and the module-name pool
//! are architecture-neutral and were byte-identical in both faces. Anything
//! whose value is genuinely per-arch (DRAM base, kernel physical base, DTB
//! placement, bump pool bounds, the `place_modules` glue over the local bump
//! allocator) stays in its own file with a cross-reference; so does the
//! `say`/`say_hex` pair, whose `early_console` implementation differs by arch.

use minix_boot::{NR_BOOT_MODULES, MemoryRegion};

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

/// Is `pa` inside any memmap range? (Guard: never dereference a hartid or
/// other garbage value.)
pub(crate) fn in_memmap(memmap: &[MemoryRegion], pa: u64) -> bool {
    memmap.iter().any(|r| pa >= r.base.0 && pa < r.base.0 + r.len as u64)
}

/// Is the whole `[pa, pa + len)` inside one memmap range? `checked_add` keeps
/// the guard total for any input: overflow is rejected as out of range.
pub(crate) fn in_memmap_range(memmap: &[MemoryRegion], pa: u64, len: u64) -> bool {
    let Some(end) = pa.checked_add(len) else {
        return false;
    };
    memmap
        .iter()
        .any(|r| pa >= r.base.0 && end <= r.base.0 + r.len as u64)
}

/// Magic gate: read one aligned `u64` at `pa` and compare it with the
/// `BootFileTable` signature.
pub(crate) fn table_magic_ok(pa: u64) -> bool {
    // SAFETY: the caller guarantees `in_memmap(pa)` — the address is inside
    // DTB-reported DRAM and readable; one aligned u64 read; boot is
    // single-hart.
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

/// Static landing pad for the normalized `param_buf`.
///
/// Both boot faces are allocator-free, so the normalized records are written
/// into this fixed buffer before `arch_boot`; the kernel's `store_kernel_info`
/// deep-copies them into its own `.bss` immediately afterwards.
pub(crate) fn param_landing() -> &'static mut [u8] {
    static mut PARAM_BUF: [u8; minix_boot::PARAM_BUF_CAP] =
        [0u8; minix_boot::PARAM_BUF_CAP];
    // SAFETY: one call per boot, single-hart; the buffer lives for the whole
    // run and `addr_of_mut!` avoids creating a reference to the `static mut`
    // itself.
    unsafe { &mut *core::ptr::addr_of_mut!(PARAM_BUF) }
}

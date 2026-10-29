//! M4 (TODO-01): `store_kernel_info` must run before anything in a boot entry
//! can allocate a page-table page or read the free list.
//!
//! The three production entries are architecture-gated and reach the mapping
//! machinery only through `arch_boot_impl` (which allocates from the boot bump
//! pool) or, on the aarch64 cross-image resume path, `establish_boot_dm`
//! (additive DM leaves on the active root). The mock entry (`arch_boot_test`)
//! deliberately bypasses the store and is therefore not listed here — its
//! counterpart assertion lives in `arch_boot_impl` behind `not(feature = "mock")`.
//!
//! A source-order pin is the honest instrument for this invariant on the host:
//! the entries themselves cannot be executed in the host test binary (they
//! switch page tables / are `-> !`). The runtime half of TODO-01 is the
//! `assert!` at the top of `arch_boot_impl`.

const SRC: &str = include_str!("../src/lib.rs");

/// Slice from `marker` to the first column-0 closing brace (function end).
fn fn_body(marker: &str) -> &'static str {
    let start = SRC
        .find(marker)
        .unwrap_or_else(|| panic!("M4 pin: entry marker not found in lib.rs: {marker}"));
    let rest = &SRC[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("M4 pin: function body end not found after: {marker}"));
    &rest[..end + 1]
}

#[test]
fn kernel_entries_store_before_any_page_allocation() {
    let entries: &[(&str, &str)] = &[
        (
            "#[cfg(all(not(feature = \"mock\"), target_arch = \"x86_64\"))]\npub fn arch_boot",
            "arch_boot_impl",
        ),
        (
            "#[cfg(all(not(feature = \"mock\"), target_arch = \"aarch64\"))]\npub fn arch_boot",
            "arch_boot_impl",
        ),
        (
            "#[cfg(all(not(feature = \"mock\"), target_arch = \"riscv64\"))]\npub fn arch_boot",
            "arch_boot_impl",
        ),
        (
            "pub fn arch_boot_resume_high_half",
            "establish_boot_dm",
        ),
    ];

    for (marker, alloc_site) in entries {
        let body = fn_body(marker);
        let store = body.find("store_kernel_info(").unwrap_or_else(|| {
            panic!("M4 pin: {marker} never calls store_kernel_info")
        });
        let alloc = body.find(alloc_site).unwrap_or_else(|| {
            panic!("M4 pin: {marker} no longer reaches {alloc_site} (update the pin)")
        });
        assert!(
            store < alloc,
            "M4 pin: {marker} reaches {alloc_site} before store_kernel_info \
             (deep copy must precede the first page allocation)"
        );
    }
}

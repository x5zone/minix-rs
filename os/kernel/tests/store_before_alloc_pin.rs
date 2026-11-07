//! `store_kernel_info` must run before anything in a boot entry
//! can allocate a page-table page or read the free list.
//!
//! The three production entries are architecture-gated and reach the mapping
//! machinery only through `arch_boot_impl` (which allocates from the boot bump
//! pool) or, on the aarch64 cross-image resume path, `establish_boot_dm`
//! (additive DM leaves on the active root). The mock entry (`arch_boot_test`)
//! deliberately bypasses the store and is therefore not listed here.
//!
//! A source-order pin is the honest instrument for this invariant on the host:
//! the entries themselves cannot be executed in the host test binary (they
//! switch page tables / are `-> !`). A runtime assertion inside
//! `arch_boot_impl` was tried and reverted: the QEMU bootstrap test kernels
//! call `arch_boot_impl` directly to exercise individual boot stages, so the
//! check cannot tell a production entry from a staged test and would fire on
//! legitimate boots.

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
    // The three direct `arch_boot` entries now delegate to one generic
    // `arch_boot_finish` helper; pin the ordering in the helper, then check
    // that every entry actually calls it.
    // D01-R1 D2-07 merged the three additive-resume bodies into the generic
    // `resume_additive` (same pattern as `arch_boot_finish`); the store-
    // before-alloc ordering is now pinned there once, and each entry must
    // still route through its helper.
    let entries: &[(&str, &str)] = &[
        ("fn arch_boot_finish", "arch_boot_impl"),
        ("fn resume_additive", "establish_boot_dm"),
        // The three delegates must not regain their own store calls: the
        // ordering contract lives in `resume_additive` alone.
        (
            "#[cfg(all(not(feature = \"mock\"), target_arch = \"aarch64\"))]\npub fn arch_boot_resume_high_half",
            "resume_additive::<",
        ),
        (
            "#[cfg(all(not(feature = \"mock\"), target_arch = \"x86_64\"))]\nfn resume_after_v1_decode",
            "resume_additive::<",
        ),
        (
            "#[cfg(all(not(feature = \"mock\"), target_arch = \"riscv64\"))]\nfn resume_after_v1_decode",
            "resume_additive::<",
        ),
    ];

    for (marker, alloc_site) in entries {
        let body = fn_body(marker);
        let alloc = body.find(alloc_site).unwrap_or_else(|| {
            panic!("M4 pin: {marker} no longer reaches {alloc_site} (update the pin)")
        });
        // Delegate shells route through `resume_additive`/`arch_boot_finish`
        // and carry no store of their own — their ordering contract is pinned
        // inside the helpers above. Helper bodies must hold store < alloc.
        if marker.contains("fn arch_boot_finish") || marker.contains("fn resume_additive") {
            let store = body.find("store_kernel_info(").unwrap_or_else(|| {
                panic!("M4 pin: {marker} never calls store_kernel_info")
            });
            assert!(
                store < alloc,
                "M4 pin: {marker} reaches {alloc_site} before store_kernel_info \
                 (deep copy must precede the first page allocation)"
            );
        }
    }

    // R2 (refute): pin the v1 resume sequence's ordering — decode → validate →
    // resume the boot region → delegate. The registration before the delegate
    // is what makes the fallback allocator dead code on the resume legs.
    {
        let body = fn_body("pub fn arch_boot_resume_v1");
        let validate = body
            .find("validate_requiring_sources(")
            .unwrap_or_else(|| panic!("M4 pin: arch_boot_resume_v1 never validates (update the pin)"));
        let resume_reg = body
            .find("resume_boot_pt_alloc(")
            .unwrap_or_else(|| panic!("M4 pin: arch_boot_resume_v1 never calls resume_boot_pt_alloc"));
        let delegate = body
            .find("resume_after_v1_decode(")
            .unwrap_or_else(|| panic!("M4 pin: arch_boot_resume_v1 never delegates (update the pin)"));
        assert!(
            validate < resume_reg && resume_reg < delegate,
            "M4 pin: arch_boot_resume_v1 sequence drift — expected validate < resume_boot_pt_alloc < resume_after_v1_decode"
        );
    }

    // Every direct entry must route through the helper; a future arch_boot
    // that bypasses it would evade the ordering pin above.
    for arch in ["x86_64", "aarch64", "riscv64"] {
        let marker = format!(
            "#[cfg(all(not(feature = \"mock\"), target_arch = \"{arch}\"))]\npub fn arch_boot"
        );
        let body = fn_body(&marker);
        assert!(
            body.contains("arch_boot_finish::<"),
            "M4 pin: {arch} arch_boot no longer calls arch_boot_finish"
        );
    }
}

//! D-72: the kernel must not inject the kernel-information page mapping in
//! `vmctl_set_addr_space` any more.
//!
//! Ownership moved to VM: every new root maps `KERNINFO_USER_VA` in
//! `init_page_table`, and the kernel-side bind path no longer draws page-table
//! pages from the boot bump pool. The function cannot run in a host test (it
//! touches the live process table / hardware roots), so this is the
//! source-order pin: the body must not mention the old injection machinery.
//!
//! Negative control: the same scanner flags a synthetic body containing
//! `boot_pt_alloc`, so a reintroduced call fails this test.

const SRC: &str = include_str!("../src/syscall.rs");

/// Slice from `marker` to the first column-0 closing brace (function end).
fn fn_body(source: &'static str, marker: &str) -> &'static str {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("D-72 pin: marker not found in syscall.rs: {marker}"));
    let rest = &source[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("D-72 pin: function body end not found after: {marker}"));
    &rest[..end + 1]
}

fn violations(body: &str) -> Vec<&'static str> {
    // Comments legitimately name the retired machinery; scan code only.
    let code: String = body
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    ["boot_pt_alloc", "kerninfo_page_phys", "KERNINFO_USER_VA"]
        .into_iter()
        .filter(|needle| code.contains(needle))
        .collect()
}

#[test]
fn set_addr_space_does_not_inject_the_kerninfo_mapping() {
    let body = fn_body(SRC, "fn vmctl_set_addr_space");
    let found = violations(body);
    assert!(
        found.is_empty(),
        "D-72 pin: vmctl_set_addr_space still contains the kernel-side kerninfo \
         injection machinery: {found:?} (VM owns the mapping in init_page_table)"
    );
}

#[test]
fn scanner_flags_the_old_injection_shape() {
    let synthetic = "\
fn vmctl_set_addr_space(...) {
    let _ = crate::kerninfo::kerninfo_page_phys();
    let _ = boot_pt_alloc();
}
";
    assert!(!violations(fn_body(synthetic, "fn vmctl_set_addr_space")).is_empty());
}

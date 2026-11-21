//! Source-order pins for the boot protection sequence.
//!
//! The entries themselves are architecture-gated and cannot run in the host
//! test binary (they execute `lgdt`/`ltr`/`lidt`), so a source-order pin is the
//! honest instrument — the same pattern as `store_before_alloc_pin.rs`.
//!
//! Pinned invariants:
//! 1. `store_protection` → `refresh_descriptors` → `load` → `program_entry_base`
//!    (the instance must be in its final global before the embedded TSS bases
//!    are rebuilt and loaded; the GS base must be programmed after the segment
//!    reload that zeroes GS).
//! 2. `install_trap_stubs` before `trap.load` (no empty-handler gate may go
//!    live).
//! 3. `set_kernel_stack` before `boot_ap` in `smp_init` (the AP's TSS sp0 and
//!    CPU-id stamp must exist before the SIPI).

const LIB_SRC: &str = include_str!("../src/lib.rs");
const SMP_SRC: &str = include_str!("../src/smp.rs");

/// Slice from `marker` to the first column-0 closing brace (function end).
fn fn_body<'a>(src: &'a str, marker: &str) -> &'a str {
    let start = src
        .find(marker)
        .unwrap_or_else(|| panic!("order pin: marker not found: {marker}"));
    let rest = &src[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("order pin: function end not found after: {marker}"));
    &rest[..end + 1]
}

fn position(body: &str, needle: &str, what: &str) -> usize {
    body.find(needle)
        .unwrap_or_else(|| panic!("order pin: {what} missing `{needle}`"))
}

#[test]
fn bsp_identity_has_one_source() {
    let body = fn_body(LIB_SRC, "pub fn init_protection(kernel_info: &KernelInfo) {");
    assert!(
        body.contains("CurrentProtection::init(bsp_logical"),
        "init_protection: protection init must use the elected BSP logical id, not 0"
    );
    assert!(
        !body.contains("CurrentProtection::init(0"),
        "init_protection: init(0) hardcodes TSS slot 0 as the BSP"
    );
    assert!(
        body.contains("program_entry_base(bsp_logical"),
        "init_protection: program_entry_base must use the same bsp_logical"
    );
}

#[test]
fn protection_store_refresh_load_entry_base_order() {
    let body = fn_body(LIB_SRC, "pub fn init_protection(kernel_info: &KernelInfo) {");
    let store = position(body, "store_protection(prot)", "init_protection");
    let refresh = position(body, "refresh_descriptors()", "init_protection");
    let load = position(body, "with_protection(|prot| prot.load())", "init_protection");
    let entry = position(body, "program_entry_base(", "init_protection");
    assert!(
        store < refresh,
        "init_protection: refresh_descriptors must run after the instance is stored"
    );
    assert!(
        refresh < load,
        "init_protection: load must run after refresh_descriptors"
    );
    assert!(
        load < entry,
        "init_protection: program_entry_base must run after load (mov gs,0 zeroes GS base)"
    );
}

#[test]
fn trap_stubs_installed_before_idt_load() {
    let body = fn_body(LIB_SRC, "pub fn init_protection(kernel_info: &KernelInfo) {");
    let install = position(body, "install_trap_stubs(&mut trap)", "init_protection");
    let load = position(body, "with_trap_entry(|trap| trap.load())", "init_protection");
    assert!(
        install < load,
        "init_protection: install_trap_stubs must run before the IDT is loaded"
    );
}

#[test]
fn ap_stack_programmed_before_sipi() {
    let body = fn_body(SMP_SRC, "pub fn smp_init() {");
    let set_stack = position(body, "set_kernel_stack(logical", "smp_init");
    let sipi = position(body, "boot_ap(", "smp_init");
    assert!(
        set_stack < sipi,
        "smp_init: the AP TSS sp0/stamp must be written before the SIPI"
    );
}

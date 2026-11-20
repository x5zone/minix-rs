//! Write-once contract for the U-Boot leg's two install-type globals:
//! a second `install_dtb_ptr` / `install_boot_file_table` must abort in
//! **all** build profiles (D-064 / I-32).
//!
//! This lives in `tests/` on purpose: `DTB_PTR` and `BOOT_FILE_TABLE_PTR`
//! are process-global `static mut`s that cannot be reset once installed, and
//! the in-crate test binary already has tests that read the installed state
//! (see `opensbi_helpers::tests::test_boot_file_table_validates_magic`, whose
//! comment states that the test cannot safely install the global). An
//! integration test gets its own process and starts from the pristine
//! (zero) state — the same reason `arch/tests/pt_alloc_write_once.rs` exists.

#![cfg(feature = "opensbi")]

use boot_shim::opensbi_helpers::{
    boot_file_table, dtb_ptr, install_boot_file_table, install_dtb_ptr,
};

const FIRST: u64 = 0x1000;
const SECOND: u64 = 0x2000;

#[test]
fn install_dtb_ptr_is_write_once() {
    assert_eq!(dtb_ptr(), None, "a fresh test process starts uninstalled");

    // SAFETY: the address is never dereferenced — `dtb_ptr()` only reads the
    // stored integer back.
    unsafe { install_dtb_ptr(FIRST) };
    assert_eq!(dtb_ptr(), Some(FIRST));

    // Second install fails closed in every profile. `catch_unwind` rather
    // than `#[should_panic]` so the assertions below still run: the rejected
    // call must not have overwritten the installed value.
    let second = std::panic::catch_unwind(|| unsafe { install_dtb_ptr(SECOND) });
    assert!(
        second.is_err(),
        "double install_dtb_ptr must panic in all build profiles"
    );
    assert_eq!(
        dtb_ptr(),
        Some(FIRST),
        "the rejected second install must not swap the live DTB pointer"
    );
}

#[test]
fn install_boot_file_table_is_write_once() {
    assert!(
        boot_file_table().is_none(),
        "a fresh test process starts with no installed table"
    );

    // The table contents are irrelevant to the write-once contract: the
    // guard runs before the store. This test therefore never calls
    // `boot_file_table()` after installing — that reader dereferences the
    // stored address before the magic check, so a fake address would fault.
    // SAFETY: the address is only stored, never read back through the
    // dereferencing reader (`boot_file_table()` is not called here).
    unsafe { install_boot_file_table(FIRST) };

    let second = std::panic::catch_unwind(|| unsafe { install_boot_file_table(SECOND) });
    assert!(
        second.is_err(),
        "double install_boot_file_table must panic in all build profiles"
    );
    // The rejected install must not have replaced the stored pointer. The
    // public API cannot read it back (that reader would dereference the fake
    // address), so re-installing the same first address is the observable
    // probe: it must still be rejected, proving the pointer is still non-zero.
    let third = std::panic::catch_unwind(|| unsafe { install_boot_file_table(FIRST) });
    assert!(
        third.is_err(),
        "the rejected second install must not clear the installed pointer"
    );
}

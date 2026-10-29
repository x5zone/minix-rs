//! M7 write-once contract for the page-table allocator singletons
//! (`TODO-02`): a second `register` / `register_free` must abort in debug
//! builds and be refused (not overwrite) in release builds.
//!
//! This lives in `tests/` on purpose: the singleton state is process-global,
//! and the in-crate test binary already registers an allocator for the
//! page-table destroy test. An integration test gets its own process and
//! starts from the pristine (unregistered) state.

use core::sync::atomic::{AtomicU64, Ordering};

use minix_arch::paging::PageTableError;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};

const FIRST_ALLOC: u64 = 0xA000;
const SECOND_ALLOC: u64 = 0xB000;

fn alloc_first() -> Result<(PhysBytes, VirBytes), PageTableError> {
    Ok((PhysBytes(FIRST_ALLOC), VirBytes(FIRST_ALLOC)))
}

fn alloc_second() -> Result<(PhysBytes, VirBytes), PageTableError> {
    Ok((PhysBytes(SECOND_ALLOC), VirBytes(SECOND_ALLOC)))
}

#[test]
fn register_is_write_once() {
    assert!(
        !pt_alloc::is_registered(),
        "a fresh integration-test process starts unregistered"
    );
    pt_alloc::register(alloc_first);
    assert!(pt_alloc::is_registered());
    assert_eq!(pt_alloc::alloc_pt_page().unwrap().0, PhysBytes(FIRST_ALLOC));

    // Second registration: debug panics, release refuses without writing.
    let second = std::panic::catch_unwind(|| pt_alloc::register(alloc_second));
    if cfg!(debug_assertions) {
        assert!(second.is_err(), "double register must panic in debug builds");
    } else {
        assert!(
            second.is_ok(),
            "double register must be refused (not panic) in release builds"
        );
    }

    // Either way the first allocator stays installed.
    assert_eq!(
        pt_alloc::alloc_pt_page().unwrap().0,
        PhysBytes(FIRST_ALLOC),
        "the rejected registration must not overwrite the live allocator"
    );
}

static FREED: AtomicU64 = AtomicU64::new(0);

fn free_first(phys: PhysBytes) {
    FREED.store(phys.0 + 1, Ordering::Relaxed);
}

fn free_second(phys: PhysBytes) {
    FREED.store(phys.0 + 2, Ordering::Relaxed);
}

#[test]
fn register_free_is_write_once() {
    assert!(
        !pt_alloc::is_free_registered(),
        "a fresh integration-test process starts with no free hook"
    );
    pt_alloc::register_free(free_first);
    pt_alloc::free_pt_page(PhysBytes(0x1000));
    assert_eq!(FREED.load(Ordering::Relaxed), 0x1001);

    let second = std::panic::catch_unwind(|| pt_alloc::register_free(free_second));
    if cfg!(debug_assertions) {
        assert!(
            second.is_err(),
            "double register_free must panic in debug builds"
        );
    } else {
        assert!(
            second.is_ok(),
            "double register_free must be refused (not panic) in release builds"
        );
    }

    pt_alloc::free_pt_page(PhysBytes(0x2000));
    assert_eq!(
        FREED.load(Ordering::Relaxed),
        0x2001,
        "the rejected registration must not overwrite the live free hook"
    );
}

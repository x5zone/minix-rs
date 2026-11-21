//! Concurrent double-registration pressure.
//!
//! The slot's compare-exchange is the only writer admission; this test
//! starts two threads at a barrier and proves exactly one registration
//! survives, even though the loser panics. A separate integration-test
//! binary gives the singleton a pristine process.

use std::sync::{Arc, Barrier};
use std::thread;

use minix_arch::paging::PageTableError;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};

fn alloc_a() -> Result<(PhysBytes, VirBytes), PageTableError> {
    Ok((PhysBytes(0xA000), VirBytes(0xA000)))
}

fn alloc_b() -> Result<(PhysBytes, VirBytes), PageTableError> {
    Ok((PhysBytes(0xB000), VirBytes(0xB000)))
}

#[test]
fn concurrent_registers_have_exactly_one_winner() {
    assert!(!pt_alloc::is_registered());
    let barrier = Arc::new(Barrier::new(2));
    let b1 = Arc::clone(&barrier);
    let b2 = Arc::clone(&barrier);
    let h1 = thread::spawn(move || {
        b1.wait();
        std::panic::catch_unwind(|| pt_alloc::register(alloc_a)).is_ok()
    });
    let h2 = thread::spawn(move || {
        b2.wait();
        std::panic::catch_unwind(|| pt_alloc::register(alloc_b)).is_ok()
    });
    let wins = usize::from(h1.join().unwrap()) + usize::from(h2.join().unwrap());
    assert_eq!(wins, 1, "exactly one concurrent register must win");
    assert!(pt_alloc::is_registered());
    let page = pt_alloc::alloc_pt_page().expect("winner's allocator must be callable");
    assert!(
        page.0 == PhysBytes(0xA000) || page.0 == PhysBytes(0xB000),
        "live allocator is not one of the two registrants"
    );
}

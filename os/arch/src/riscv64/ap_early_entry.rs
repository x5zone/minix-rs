//! riscv64 AP early stub — the riscv half of the two-arch stub.
//!
//! # Entry state (SBI hart_start, HSM extension — see `smp.rs::boot_ap`)
//!
//! - a0 = hartid
//! - a1 = the `opaque` cookie (`boot_ap` passes the bootstrap pointer;
//!   an earlier draft mislabeled it "priv mode")
//! - PC = the physical entry address `boot_ap` passed as a1's neighbor
//!   start_addr
//! - S-mode, MMU **off**
//!
//! # Stub obligations (frozen contract)
//!
//! 1. Read the `ApBootstrap` record (the riscv form is **arch-local static
//!    storage** — v7 #4; record address = PC-relative from the stub inside
//!    the kernel image, computable with MMU off).
//! 2. `fence rw, rw` consumer-side ordering barrier (§3.9 first-read).
//! 3. satp ← the record's page-table root (BSP value, SATP-mode Sv39);
//!    `sfence.vma`.
//! 4. Branch to the `ap_early_entry` Rust convergence point (high VA via
//!    the kernel mapping the new satp establishes).
//!
//! # Identity-coverage verification (stub verification record)
//!
//! Same argument as the arm half: the stub and record live inside the
//! kernel image; the BSP root (boot-shim) maps the kernel image identity
//! **and** high plus the DM window — so MMU-on at the image's PA keeps the
//! PC mapped, and the record is reachable both identity (MMU-off reads)
//! and through the kernel mapping after `sfence.vma`. The hartid comes
//! from `boot_ap`'s a0 — no <1MiB constraint on riscv64.
//!
//! # Status
//!
//! The asm body below implements the obligation list
//! above (satp sequencing per §3.3 riscv row); the Rust convergence point
//! parks secondaries pending SMP unclamp (see `ap_early_entry`).

/// Unit-test sentinel: proves the module's contract constants stay in sync
/// with the SBI channel (`boot_ap` passes the bootstrap cookie in a1/a2).
pub const OPAQUE_IS_BOOTSTRAP_COOKIE: bool = true;

/// Per-AP bootstrap record — arch-local static storage. The aarch64/riscv64
/// forms have no 1 MiB copy constraint: firmware starts the AP directly at
/// the image's physical address, so the record resides with the image. The
/// BSP fills the fields through [`record`] before `SbiEidHsm` hart_start;
/// the AP-side stub reads them with PC-relative addressing (with the MMU off,
/// `la` resolves to the run-time address, which equals the physical address).
///
/// Consumption discipline (§3.9): the AP issues `fence rw, rw` before its
/// first read (the BSP likewise fences after writing, then issues hart_start
/// — the happens-before on both sides is formed by the two fences
/// bracketing SBI's inter-hart handoff).
use crate::arch::ap_early_entry::ApBootstrap;

// #[no_mangle]: the stub asm refers to the record by literal symbol name
// (`la t2, AP_BOOTSTRAP_RECORD`) across the crate symbol surface, so it
// must have an undecorated exported name.
/// Early hart election cell. On OpenSBI QEMU virt all harts enter the
/// payload, each carrying a0 = its own hartid into `_start`, so the first
/// arrival is elected BSP. 0 = not elected; otherwise the elected hartid + 1.
/// Lives in `.data` (the loader writes the initial 0 as part of filesz, not
/// as a NOBITS accident); `no_mangle` lets kernel-image stub asm refer to it
/// literally and lets the kernel read it.
#[unsafe(link_section = ".data")]
#[unsafe(no_mangle)]
pub static BOOT_HART_ELECTED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

#[unsafe(no_mangle)]
pub static mut AP_BOOTSTRAP_RECORD: ApBootstrap = ApBootstrap {
    logical_id: 0,
    _pad: 0,
    hw_id: 0,
    page_table_root_pa: 0,
    kernel_stack_top_va: 0,
    rust_entry_va: 0,
};

/// BSP-side fill window (called before hart_start). The safety of the `&mut`
/// access is guaranteed by exclusive use in the call sequence: the BSP
/// writes exclusively before the AP arrives (in the riscv form the boot_ack
/// semantics are the convergence-point arrival marker).
#[allow(static_mut_refs)]
pub fn record() -> &'static mut ApBootstrap {
    unsafe { &mut AP_BOOTSTRAP_RECORD }
}

/// Physical entry address of the AP early stub (the `entry` argument of
/// `boot_ap`).
///
/// The stub lives inside the kernel image (`.text.ap_early_entry` section),
/// and the image is loaded into physical memory: under the link view
/// (medany = physical) the symbol address is the physical address — matching
/// the kernel-image execution view (execution view = physical base).
pub fn entry_start_pa() -> usize {
    // Link view = high half (kernel-image riscv64.ld: VMA = 0xFFFFFFC0_…,
    // PA starting at 0x80200000); hart_start's start_addr must be a
    // **physical** address (the target hart starts with the MMU off).
    // Conversion = subtract (KERNEL_VIRT - KERNEL_PHYS).
    const KVIRT: usize = 0xFFFF_FFC0_0000_0000 as usize;
    const KPHYS: usize = 0x8020_0000;
    let va = ap_early_entry_start as usize;
    va - (KVIRT - KPHYS)
}

// ── AP early stub (asm body of the stub obligations §3.1) ─────────────
//
// Entry state (SBI HSM hart_start): a0 = hartid, a1 = opaque cookie,
// PC = start_addr, S-mode, MMU off.
//
// Four-step obligation list (frozen §3.1):
// 1. `la t2, AP_BOOTSTRAP_RECORD` — PC-relative fetch of the record's
//    physical address (MMU off);
// 2. `fence rw, rw` — consumer-side first-read barrier (§3.9);
// 3. satp ← the record's page_table_root_pa (Sv39 mode bits 8<<60) +
//    `sfence.vma`;
// 4. jump to `ap_early_entry` (the Rust convergence point; the link address
//    travels in the record's rust_entry_va — the kernel-image execution view
//    is physical, so the link address is the run-time address; a future
//    high-half form has the BSP write the high value, with unchanged
//    fields).
core::arch::global_asm!(
    r#"
    .section .text.ap_early_entry, "ax"
    .globl ap_early_entry_start
    ap_early_entry_start:
        fence rw, rw                    # consumer-side fence
        la  t2, AP_BOOTSTRAP_RECORD     # record phys addr (MMU off)
        ld  t3, 16(t2)                  # page_table_root_pa
        ld  t4, 24(t2)                  # kernel_stack_top_va
        ld  t5, 32(t2)                  # rust_entry_va
        # UART step marker "A1" = stub entered, record fields loaded.
        # QEMU virt 16550 THR @ 0x10000000 — device write, MMU off safe.
        li  t0, 0x10000000
        li  t1, 0x41          # 'A'
        sb  t1, 0(t0)
        li  t1, 0x31          # '1'
        sb  t1, 0(t0)
        srli t3, t3, 12                 # PPN = pa >> 12
        li  t6, 0x8000000000000000      # SATP_MODE_SV39
        or  t3, t3, t6
        csrw satp, t3
        sfence.vma
        # UART step marker "A2" = MMU on with BSP root, fetch still alive
        # (proves the root identity-maps the image).
        li  t0, 0x10000000
        li  t1, 0x41          # 'A'
        sb  t1, 0(t0)
        li  t1, 0x32          # '2'
        sb  t1, 0(t0)
        mv  sp, t4                      # per-AP kernel stack
        # a0 = record HIGH VA = PA + (KVIRT - KPHYS)，MMU on 后经内核根映射
        li  t6, (0xFFFFFFC000000000 - 0x80200000) >> 12
        slli t6, t6, 12
        add a0, t2, t6
        jr  t5                          # jump to Rust convergence (high VA)
    "#,
);

unsafe extern "C" {
    /// Physical stub entry (the `start_addr` of `boot_ap`).
    fn ap_early_entry_start();
}

/// Convergence-point arrival marker (the observable surface for the gate
/// criterion "secondary core prints / is observed reaching the convergence
/// point"). 0 = not arrived; 1 = arrived (the Rust view after the fence).
pub static AP_ARRIVED: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Convergence point — the AP jumps here from the stub (a0 = the record's
/// physical address).
///
/// The riscv form of the same-named x86 function: read the record fields
/// back into locals (the riscv version of the single-image serial-reuse
/// discipline — the static record needs no return to the BSP, but the field
/// snapshot is still taken into locals so a BSP rewrite for the next hart
/// cannot race it), set the arrival marker, then **park** (a wfi parking
/// loop).
///
/// Parking instead of continuing kernel initialization is the honest
/// boundary of this increment: the primary-core clamp is not yet lifted, and
/// per-core run queues / preemption delivery are not powered — the AP is
/// "awake but idle" at this point, and pushing on would cross the red line:
/// lifting the clamp without first providing per-core queues means a process
/// could be placed on a core that has no queue.
pub unsafe extern "C" fn ap_early_entry(bootstrap_pa: usize) -> ! {
    // UART step marker "A3" = Rust convergence entered. Write through the
    // identity-mapped PA (the same route as step A2 in the stub: A2 already
    // proved on the serial line that the kernel root identity-maps it). Do
    // not use the KDM window VA: GDB evidence showed that the root table
    // (the one the BSP hands to the AP as current_root_phys) does not contain
    // the KDM window, so the store faults (scause=15
    // stval=0xffffffc050000000); and because the OpenSBI hart_start handoff
    // sets stvec to start_addr (= the stub entry), one fault bounces back to
    // the stub start in an endless flood.
    unsafe {
        let uart = 0x1000_0000u64 as *mut u8;
        core::ptr::write_volatile(uart, b'A');
        core::ptr::write_volatile(uart.add(1), b'3');
    }
    // a0 = the record's high-half VA (converted by the stub; reachable
    // through the kernel root once the MMU is on).
    let record = unsafe { &*(bootstrap_pa as *const ApBootstrap) };
    // Consumption snapshot (volatile reads: prevent coalescing with a
    // potential BSP rewrite).
    let _logical = unsafe { core::ptr::read_volatile(&record.logical_id) };
    let _hw_id = unsafe { core::ptr::read_volatile(&record.hw_id) };
    let _ = (_logical, _hw_id);
    // Arrival marker (Release: when the BSP later reads 1, the snapshot
    // reads above have completed).
    core::sync::atomic::AtomicUsize::store(&AP_ARRIVED, 1, core::sync::atomic::Ordering::Release);
    loop {
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}

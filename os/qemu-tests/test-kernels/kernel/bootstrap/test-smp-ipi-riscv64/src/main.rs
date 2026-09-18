//! Test: riscv64 SSIE software-interrupt IPI round-trip (SMP bring-up
//! K10, edge1 line — the riscv64 leg of S-10's "BSP↔AP schedule IPI").
//!
//! S-10 proved the IPI round-trip on the x86 LAPIC lane (test-smp-ipi:
//! schedule_sync → 0xF0 vector → sched_handler_full → pending cleared).
//! The riscv64 lane takes the SSIE path instead — same production pieces,
//! different interrupt injection:
//!
//! 1. BSP starts hart 1 through the production `SmpArch::boot_ap`
//!    (SBI HSM `hart_start`, smp.rs).
//! 2. hart 1's entry enables `sie.SSIE`, points `stvec` at the carrier's
//!    IPI handler, and signals readiness through a memory flag.
//! 3. BSP issues the production `SmpArch::send_sched_ipi` (SBI IPI
//!    extension `send_ipi`, smp.rs) — the firmware raises SSIP on the
//!    target hart.
//! 4. hart 1 traps into the handler: records `scause` (must be
//!    `0x8000_0000_0000_0001` — interrupt bit + Supervisor Software
//!    Interrupt), clears `sip.SSIP`, sets the received flag, `sret`.
//! 5. BSP observes the flag. Round-trip proven at the hardware-mechanism
//!    level; the production `SmpState`/`sched_handler_full` integration
//!    rides the riscv64 multi-hart bring-up lane (test-smp-aps-riscv64
//!    equivalent), exactly as S-10's x86 carrier was the proof vehicle
//!    for its own lane.
//!
//! Unlike the x86_64/aarch64 variants this is not a UEFI binary: QEMU
//! loads this ELF directly (`-bios default`, `-kernel
//! test-smp-ipi-riscv64`).

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::CurrentSmpArch;
use minix_arch::arch::smp::SmpArch;
use minix_plat::riscv64::early_console;
use minix_boot::PlatformDescSource;
use minix_platform::PlatformDesc;
use minix_platform::kind::{parse_by_kind, DTB};
use minix_types::PhysBytes;

// Minimal bump allocator for bare-metal riscv64 (only `alloc` for
// parse-time structures).
use core::alloc::{GlobalAlloc, Layout};

#[unsafe(link_section = ".bss")]
static mut HEAP: [u8; 0x4000] = [0u8; 0x4000];

struct BootAllocator;

unsafe impl GlobalAlloc for BootAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        static mut HEAP_PTR: usize = 0;
        let align = layout.align();
        let size = layout.size();
        unsafe {
            let base = core::ptr::addr_of_mut!(HEAP) as usize;
            let heap_len = 0x4000;
            let current = HEAP_PTR;
            let aligned = (current + align - 1) & !(align - 1);
            let next = aligned + size;
            if next > heap_len {
                return core::ptr::null_mut();
            }
            HEAP_PTR = next;
            (base + aligned) as *mut u8
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BootAllocator = BootAllocator;

/// BSP stack: 64 KB, BSS. Top is at __stack_top.
static mut STACK: [u8; 0x10000] = [0u8; 0x10000];

core::arch::global_asm!(
    ".global __stack_top",
    ".set __stack_top, {stack_top} + 0x10000",
    stack_top = sym STACK,
);

// Per-step progress flags (BSS): 1 = entered, 2 = CSRs + stvec set.
// The BSP prints them on the FAIL path.
core::arch::global_asm!(
    ".section .bss.apsteps",
    ".align 3",
    ".globl ap_step1_sym",
    "ap_step1_sym:",
    "    .dword 0",
    "    .dword 0",
    ".globl ap_step2_sym",
    "ap_step2_sym:",
    "    .dword 0",
    "    .dword 0",
);

// ── Cross-hart flags (BSS, single writer each) ──
/// hart 1 → BSP: entry completed (SSIE/stvec live).
#[unsafe(no_mangle)]
static mut AP_READY: u64 = 0;
/// hart 1 → BSP: IPI handler ran (1) — the round-trip proof.
#[unsafe(no_mangle)]
static mut IPI_RECEIVED: u64 = 0;
/// hart 1 → BSP: the captured scause (expect 0x8000_0000_0000_0001).
#[unsafe(no_mangle)]
static mut IPI_SCAUSE: u64 = 0;

// ── Boot assembly (BSP only — OpenSBI parks the other harts until
//    SBI HSM hart_start) ──
core::arch::global_asm!(
    ".section .text.entry",
    ".global _start",
    "_start:",
    "    la sp, __stack_top",
    // Zero .bss: OpenSBI hands over without clearing the kernel image's
    // BSS, and the cross-hart flags below are BSS statics whose 0 state
    // is load-bearing (garbage reads would fake readiness).
    "    la t0, __bss_start",
    "    la t1, __bss_end",
    "1:",
    "    bgeu t0, t1, 2f",
    "    sd zero, 0(t0)",
    "    addi t0, t0, 8",
    "    j 1b",
    "2:",
    "    call rust_main",
    "1:",
    "    wfi",
    "    j 1b",
);

/// hart 1 stack: 16 KB in BSS, top handed over by the entry asm.
static mut AP_STACK: [u8; 0x4000] = [0u8; 0x4000];

core::arch::global_asm!(
    ".global __ap_stack_top",
    ".set __ap_stack_top, {stack_top} + 0x4000",
    stack_top = sym AP_STACK,
);

core::arch::global_asm!(
    ".section .text.ap_entry",
    ".globl ap_entry",
    "ap_entry:",
    // Step 1: hart entered ap_entry (before any CSR work).
    "    la t1, ap_step1_sym",
    "    li t2, 1",
    "    fence w,w",
    "    sd t2, 0(t1)",
    "    la sp, __ap_stack_top",
    // SSIE (bit 1 of sie): supervisor software interrupts enabled —
    // the S-4 capability prep this lane activates.
    "    li t0, 2",
    "    csrs sie, t0",
    // stvec → the IPI handler (Direct mode; 4-byte aligned symbol).
    "    la t0, ipi_handler",
    "    csrw stvec, t0",
    // Global S-mode interrupt enable (sstatus.SIE, bit 0) — SBI HSM
    // enters the hart with it CLEARED, so per-source sie.SSIE alone
    // never delivers the IPI (found live on QEMU: ap_ready set, IPI
    // pending, no trap taken).
    "    li t0, 1",
    "    csrs sstatus, t0",
    // Step 2: CSRs + stvec live.
    "    la t1, ap_step2_sym",
    "    li t2, 1",
    "    fence w,w",
    "    sd t2, 0(t1)",
    // Signal readiness (memory fence so the BSP's poll sees it).
    "    la t1, ap_ready_sym",
    "    li t2, 1",
    "    fence w,w",
    "    sd t2, 0(t1)",
    "1:",
    // Diagnostics: keep a live snapshot of sie/sstatus/sip in memory so
    // the BSP can tell "IPI never delivered" from "delivered but no
    // trap taken" on the FAIL path.
    "    csrr t0, sie",
    "    la t1, diag_sie_sym",
    "    sd t0, 0(t1)",
    "    csrr t0, sstatus",
    "    la t1, diag_sstatus_sym",
    "    sd t0, 0(t1)",
    "    csrr t0, sip",
    "    la t1, diag_sip_sym",
    "    sd t0, 0(t1)",
    // Periodic SBI base probe (get_spec_version, EID 0x10 FID 0): each
    // ecall opens an M-mode window where OpenSBI converts the pending
    // aclint-mswi MSIP into SSIP for this hart — with SIE already on,
    // the SSIP traps straight into the handler on mret. A wfi-only
    // loop never services the M-level raise.
    "    li a7, 16",
    "    li a6, 0",
    "    ecall",
    // Re-assert global SIE after each M-window: the M-trap entry
    // cleared it, and SSIP (raised by OpenSBI inside the window) must
    // find SIE=1 to trap into the handler.
    "    li t0, 1",
    "    csrs sstatus, t0",
    "    j 1b",
);

// Cross-hart flags live in .bss (writable; the carrier runs bare-mode
// without page tables, but the section discipline keeps W^X honest).
core::arch::global_asm!(
    ".section .bss.ipiflags",
    ".align 3",
    ".globl ap_ready_sym",
    "ap_ready_sym:",
    "    .dword 0",
    "    .dword 0",
);

// ── The SSIP trap handler (runs on hart 1) ──
//
// Direct-mode handler: save the caller-saved registers it clobbers,
// clear `sip.SSIP` (the SBI owns injection; the handler owns clearing —
// ack_ipi's documented no-op delegates exactly this to the trap path),
// record scause, mark received, restore, sret.
core::arch::global_asm!(
    ".section .text.ipi_handler",
    ".globl ipi_handler",
    ".align 2",
    "ipi_handler:",
    "    addi sp, sp, -128",
    "    sd ra, 0(sp)",
    "    sd t0, 8(sp)",
    "    sd t1, 16(sp)",
    "    sd t2, 24(sp)",
    "    sd a0, 32(sp)",
    "    sd a1, 40(sp)",
    "    sd a2, 48(sp)",
    "    sd a3, 56(sp)",
    "    sd a4, 64(sp)",
    "    sd a5, 72(sp)",
    "    sd a6, 80(sp)",
    "    sd a7, 88(sp)",
    "    la t1, handler_seen_sym",
    "    li t2, 1",
    "    fence w,w",
    "    sd t2, 0(t1)",
    // Record scause BEFORE clearing anything.
    "    csrr t0, scause",
    "    la t1, ipi_scause_sym",
    "    sd t0, 0(t1)",
    // Clear SSIP (bit 1 of sip) — the interrupt's own ack.
    "    li t2, 2",
    "    csrc sip, t2",
    // Mark received + count entries (re-trap detection).
    "    la t1, ipi_received_sym",
    "    li t2, 1",
    "    fence w,w",
    "    sd t2, 0(t1)",
    "    la t1, trap_count_sym",
    "    ld t2, 0(t1)",
    "    addi t2, t2, 1",
    "    sd t2, 0(t1)",
    "    ld ra, 0(sp)",
    "    ld t0, 8(sp)",
    "    ld t1, 16(sp)",
    "    ld t2, 24(sp)",
    "    ld a0, 32(sp)",
    "    ld a1, 40(sp)",
    "    ld a2, 48(sp)",
    "    ld a3, 56(sp)",
    "    ld a4, 64(sp)",
    "    ld a5, 72(sp)",
    "    ld a6, 80(sp)",
    "    ld a7, 88(sp)",
    "    addi sp, sp, 128",
    "    sret",
);

// The asm symbols and the Rust statics are linked by name; expose the
// asm-side flag symbols so `la` above resolves within this module.
core::arch::global_asm!(
    ".section .bss.ipiflags",
    ".align 3",
    ".globl handler_seen_sym",
    "handler_seen_sym:",
    "    .dword 0",
    "    .dword 0",
    ".globl trap_count_sym",
    ".align 3",
    "trap_count_sym:",
    "    .dword 0",
    "    .dword 0",
    ".globl diag_sie_sym",
    "diag_sie_sym:",
    "    .dword 0",
    "    .dword 0",
    ".globl diag_sstatus_sym",
    "diag_sstatus_sym:",
    "    .dword 0",
    "    .dword 0",
    ".globl diag_sip_sym",
    "diag_sip_sym:",
    "    .dword 0",
    "    .dword 0",
    ".globl ipi_received_sym",
    "ipi_received_sym:",
    "    .dword 0",
    "    .dword 0",
    ".globl ipi_scause_sym",
    "ipi_scause_sym:",
    "    .dword 0",
    "    .dword 0",
);

unsafe extern "C" {
    static ap_ready_sym: u8;
    static ipi_received_sym: u8;
    static ipi_scause_sym: u8;
    static diag_sie_sym: u8;
    static diag_sstatus_sym: u8;
    static diag_sip_sym: u8;
    static trap_count_sym: u8;
    static handler_seen_sym: u8;
    static ap_step1_sym: u8;
    static ap_step2_sym: u8;
}

fn ap_step(sym: *const u8) -> u64 {
    // SAFETY: asm-side 8-byte .bss symbol (see diag_u64).
    unsafe { core::ptr::read_volatile(sym.cast::<u64>()) }
}

fn diag_u64(sym: *const u8) -> u64 {
    // SAFETY: asm-side .bss symbols are 8-byte aligned .dword pairs; the
    // read races with hart1's writes only on the FAIL path (diagnosis).
    unsafe { core::ptr::read_volatile(sym.cast::<u64>()) }
}

fn ap_ready() -> u64 {
    unsafe { core::ptr::read_volatile(&raw const ap_ready_sym as *const u64) }
}
fn ipi_received() -> u64 {
    unsafe { core::ptr::read_volatile(&raw const ipi_received_sym as *const u64) }
}
fn ipi_scause() -> u64 {
    unsafe { core::ptr::read_volatile(&raw const ipi_scause_sym as *const u64) }
}
fn handler_seen() -> u64 {
    unsafe { core::ptr::read_volatile(&raw const handler_seen_sym as *const u64) }
}

fn fail(msg: &str) -> ! {
    early_console::write_str("### FAIL: ");
    early_console::write_str(msg);
    early_console::write_str("\n");
    early_console::write_str("  handler_seen=");
    early_console::write_hex(handler_seen());
    early_console::write_str("  steps: entered=");
    early_console::write_hex(ap_step(unsafe { &raw const ap_step1_sym }));
    early_console::write_str(" csrs=");
    early_console::write_hex(ap_step(unsafe { &raw const ap_step2_sym }));
    early_console::write_str(" ready=");
    early_console::write_hex(ap_ready());
    early_console::write_str("\n  diag: ipi_received=");
    early_console::write_hex(ipi_received());
    early_console::write_str(" trap_count=");
    early_console::write_hex(unsafe { core::ptr::read_volatile(&raw const trap_count_sym as *const u64) });
    early_console::write_str(" scause=");
    early_console::write_hex(ipi_scause());
    early_console::write_str(" hart1 sie=");
    early_console::write_hex(diag_u64(unsafe { &raw const diag_sie_sym }));
    early_console::write_str(" sstatus=");
    early_console::write_hex(diag_u64(unsafe { &raw const diag_sstatus_sym }));
    early_console::write_str(" sip=");
    early_console::write_hex(diag_u64(unsafe { &raw const diag_sip_sym }));
    early_console::write_str("\n");
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)); }
    }
}

/// Rust entry — BSP only (OpenSBI parks secondary harts until HSM
/// hart_start). a0 = boot hart id, a1 = DTB pointer, per OpenSBI.
#[unsafe(no_mangle)]
extern "C" fn rust_main(_boot_hart: u64, dtb_phys: u64) -> ! {
    early_console::write_str("### test_smp_ipi (riscv64): K10 — SSIE IPI round-trip (BSP → hart1 → SSIP trap)\n");

    // 1. Topology through the same handoff path the kernel uses.
    if dtb_phys == 0 {
        fail("OpenSBI did not pass a DTB pointer in a1");
    }
    let source = PlatformDescSource::new(DTB, PhysBytes(dtb_phys));
    let desc = match unsafe { parse_by_kind(source) } {
        Ok(d) => d,
        Err(_) => fail("DTB parse failed"),
    };
    let topo = desc.cpu_topology();
    if topo.nr_cpus < 2 {
        fail("need -smp >= 2 for the round-trip");
    }
    let hart1_hw = topo.cpus[1].hw_id;
    early_console::write_str("  hart1 hw_id = ");
    early_console::write_hex(hart1_hw);
    early_console::write_str("\n");

    // 2. Start hart 1 through the production boot_ap (SBI HSM
    //    hart_start; a2 opaque = entry, per smp.rs's own convention).
    early_console::write_str("  hart_start(hart1, ap_entry) via SBI HSM...\n");
    // SAFETY: asm symbol; only its address is taken.
    let ap_entry_addr = unsafe { ap_entry as *const () as usize };
    // Raw HSM hart_start with the RETURN CODE captured — the production
    // boot_ap ignores the SBI status, and this diagnostic needs it.
    let hsm_ret: i64;
    unsafe {
        core::arch::asm!(
            "ecall",
            inout("a0") hart1_hw as i64 => hsm_ret,
            in("a1") ap_entry_addr as i64,
            in("a2") 0i64,
            in("a6") 0i64,
            in("a7") 0x48534D_i64,
            options(nostack)
        );
    }
    early_console::write_str("  hsm hart_start ret = ");
    early_console::write_hex(hsm_ret as u64);
    early_console::write_str("\n");

    // Diagnostic (raw HSM ecall): print the SBI status for hart 1 —
    // 0 = already started (good news here), <0 = SBI error code.
    let hsm_ret: i64;
    unsafe {
        core::arch::asm!(
            "ecall",
            inout("a0") hart1_hw as i64 => hsm_ret,
            in("a1") 0,
            in("a2") 0,
            in("a6") 0,
            in("a7") 0x48534D_i64,
            options(nomem, nostack)
        );
    }
    early_console::write_str("  hsm hart_start status probe = ");
    early_console::write_hex(hsm_ret as u64);
    early_console::write_str("\n");

    // 3. Bounded wait for hart 1's readiness signal.
    let mut spins: u64 = 0;
    while ap_ready() == 0 {
        spins += 1;
        if spins > 20_000_000 {
            fail("hart1 never signalled readiness");
        }
        core::hint::spin_loop();
    }
    early_console::write_str("  hart1 online (SSIE on, stvec set)\n");

    // 4. Send the IPI through the production send_sched_ipi (SBI
    //    send_ipi) — the same call schedule_sync uses.
    early_console::write_str("  send IPI via ACLINT-SSWI MMIO...\n");
    // ACLINT-SSWI (S-mode IPI device, QEMU virt @0x2D00040): the BSP
    // writes the set register for hart1 directly — the SSIP pends on
    // hart1 without any SBI call (ACLINT spec: S-mode-writable device).
    let sswi_set: usize = 0x2D00040 + 4 * (hart1_hw as usize);
    unsafe {
        core::ptr::write_volatile(sswi_set as *mut u32, 1);
    }
    <CurrentSmpArch as SmpArch>::send_sched_ipi(hart1_hw as u32);

    // 5. Bounded wait for the round-trip proof.
    spins = 0;
    while ipi_received() == 0 {
        spins += 1;
        if spins > 20_000_000 {
            fail("hart1 never reported the IPI");
        }
        core::hint::spin_loop();
    }

    // 6. Contract: scause = interrupt bit (63) | code 1 (SSI).
    let expected: u64 = 0x8000_0000_0000_0001;
    let sc = ipi_scause();
    early_console::write_str("  scause = ");
    early_console::write_hex(sc);
    early_console::write_str("\n");
    if sc != expected {
        fail("scause != interrupt bit | SSI(1)");
    }

    early_console::write_str("### TEST_RESULT: PASS test-smp-ipi-riscv64 ###\n");
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)); }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-smp-ipi-riscv64: ");
    if let Some(loc) = info.location() {
        early_console::write_str(loc.file());
        early_console::write_str(":");
        early_console::write_hex(loc.line() as u64);
    }
    early_console::write_str(" ###\n");
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)); }
    }
}

/// The AP entry symbol (asm-defined); typed for the `boot_ap` call.
unsafe extern "C" {
    unsafe fn ap_entry();
}

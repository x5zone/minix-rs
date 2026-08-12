//! x86-64 SMP implementation using the Local APIC.
//!
//! Implements `SmpArch` for x86-64 by programming the LAPIC (Local Advanced
//! Programmable Interrupt Controller) MMIO registers to send IPIs, acknowledge
//! interrupts, and boot Application Processors via the INIT-SIPI sequence.
//!
//! # LAPIC base discovery
//!
//! The LAPIC base address is read from MSR `IA32_APIC_BASE` (MSR 0x1B).
//! On reset, the CPU sets this to `0xFEE00000`. The MMIO region is 4 KiB
//! and contains the LAPIC register set. We mask bits 12..35 to extract the
//! physical base (Intel SDM Vol. 3A §10.4.4).
//!
//! # Anti-translate vs C
//!
//! C stores the LAPIC base in a global variable (`lapic_base`) set by
//! `apic_init()`. Rust reads MSR `IA32_APIC_BASE` directly — no global state,
//! the hardware is the single source of truth. This is safe because the MSR
//! is per-CPU and identical across all CPUs in a symmetric multiprocess.
//!
//! C: arch/i386/smp.c, arch/i386/apic.c

use crate::smp::SmpArch;

/// MSR address for `IA32_APIC_BASE` (Intel SDM Vol. 4 §2.1).
const IA32_APIC_BASE: u32 = 0x1B;

/// Bit mask for the LAPIC physical base address in MSR `IA32_APIC_BASE`.
/// Bits 12..35 hold the base; bits 0-11 are status/control flags.
/// Intel SDM Vol. 3A §10.4.4.
const LAPIC_BASE_MASK: u64 = 0xFFFFF000;

/// LAPIC register offsets (in bytes from base).
/// Intel SDM Vol. 3A §10.5.
const LAPIC_ID: u32 = 0x20;         // Local APIC ID Register
const LAPIC_EOI: u32 = 0xB0;        // End of Interrupt
const LAPIC_ICR_LOW: u32 = 0x300;   // Interrupt Command Register (low)
const LAPIC_ICR_HIGH: u32 = 0x310;  // Interrupt Command Register (high)

/// ICR Delivery Mode: Fixed (000b). Intel SDM Vol. 3A §10.6.1.
const ICR_DELIVERY_FIXED: u32 = 0x0000_0000;
/// ICR Delivery Mode: INIT (101b). Intel SDM Vol. 3A §10.6.1.
const ICR_DELIVERY_INIT: u32 = 0x0000_0500;
/// ICR Delivery Mode: Startup IPI (110b). Intel SDM Vol. 3A §10.6.1.
const ICR_DELIVERY_SIPI: u32 = 0x0000_0600;

/// ICR Destination Mode: Physical (0). Intel SDM Vol. 3A §10.6.1.
const ICR_DEST_PHYSICAL: u32 = 0x0000_0000;

/// ICR Level: Assert (1). Intel SDM Vol. 3A §10.6.1.
/// For xAPIC mode, Level=1 for all IPI sends.
const ICR_LEVEL_ASSERT: u32 = 0x0000_4000;

/// ICR Delivery Status: Send Pending bit. When set, a previous IPI is still
/// in flight. We spin-wait until it clears before sending the next IPI.
/// Intel SDM Vol. 3A §10.6.1.
const ICR_DELIVERY_PENDING: u32 = 0x0000_1000;

/// LAPIC vector for the schedule IPI.
/// C: `SCHED_IPI` — arch/i386/smp.h
/// Vector 0xF0 is in the range 0x20-0xFF (usable for user-defined interrupts
/// in x86-64; vectors 0x00-0x1F are reserved for exceptions).
const SCHED_IPI_VECTOR: u32 = 0xF0;

/// x86-64 SMP backend using the Local APIC.
///
/// This is a zero-sized type — all hardware state is read directly from
/// MSRs and MMIO registers, so no instance fields are needed.
///
/// C: arch/i386/apic.c + arch/i386/smp.c
pub struct X86_64SmpArch;

/// Read the LAPIC MMIO base address from MSR `IA32_APIC_BASE`.
///
/// Returns the physical address of the 4 KiB LAPIC register region.
/// This is typically `0xFEE00000` on x86-64 systems.
///
/// # Safety
///
/// `rdmsr` is a privileged instruction that reads a model-specific register.
/// It is safe to call in kernel mode (CPL 0) with interrupts disabled.
#[inline]
fn lapic_base() -> u64 {
    let lo: u32;
    let hi: u32;
    // SAFETY: `rdmsr` reads a CPU MSR. We read IA32_APIC_BASE (0x1B) which
    // is a read-only snapshot of the APIC base configuration. This is safe
    // in ring 0 (kernel mode).
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") IA32_APIC_BASE,
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack),
        );
    }
    ((hi as u64) << 32 | lo as u64) & LAPIC_BASE_MASK
}

/// Write a 32-bit value to a LAPIC MMIO register.
///
/// # Safety
///
/// Caller must ensure `offset` is a valid LAPIC register offset and the
/// LAPIC is enabled (via IA32_APIC_BASE MSR).
#[inline]
unsafe fn lapic_write(offset: u32, value: u32) {
    let addr = (lapic_base() as usize + offset as usize) as *mut u32;
    // SAFETY: `addr` points to the LAPIC MMIO region. The write is
    // side-effecting (triggers IPI send, EOI, etc.) but is memory-mapped
    // and volatile.
    unsafe {
        core::ptr::write_volatile(addr, value);
    }
}

/// Read a 32-bit value from a LAPIC MMIO register.
///
/// # Safety
///
/// Caller must ensure `offset` is a valid LAPIC register offset.
#[inline]
#[allow(dead_code)]
unsafe fn lapic_read(offset: u32) -> u32 {
    let addr = (lapic_base() as usize + offset as usize) as *const u32;
    // SAFETY: `addr` points to the LAPIC MMIO region. Volatile read is
    // required because reads from some registers (e.g., ICR delivery
    // status) have side effects.
    unsafe {
        core::ptr::read_volatile(addr)
    }
}

/// Wait for the LAPIC ICR (Interrupt Command Register) to become idle.
///
/// The LAPIC has a single ICR; if a previous IPI is still being delivered
/// (Delivery Status = Send Pending), we must wait before sending a new one.
/// Intel SDM Vol. 3A §10.6.1: "Software should poll the delivery status bit
/// to ensure that the previous IPI has been sent."
#[inline]
fn wait_icr_idle() {
    // SAFETY: Reading ICR_LOW is safe; the Delivery Status bit is a
    // status field with no side effects on read.
    while unsafe { lapic_read(LAPIC_ICR_LOW) } & ICR_DELIVERY_PENDING != 0 {
        core::hint::spin_loop();
    }
}

impl SmpArch for X86_64SmpArch {
    fn send_sched_ipi(cpu: u32) {
        // C: arch_send_smp_schedule_ipi(cpu) — arch/i386/smp.c:65
        //
        // Program the LAPIC ICR to send a Fixed IPI with the schedule
        // vector to the target CPU's APIC ID.
        //
        // ICR layout (xAPIC mode):
        //   Bits 0-7:   Vector (SCHED_IPI_VECTOR = 0xF0)
        //   Bits 8-10:  Delivery Mode (Fixed = 0b000)
        //   Bits 11:    Destination Mode (Physical = 0)
        //   Bit  12:    Delivery Status (0 = Idle, read-only)
        //   Bit  14:    Level (Assert = 1)
        //   Bits 18-19: Trigger Mode (Edge = 0)
        //   Bits 24-31: Destination APIC ID (physical mode)
        //
        // ICR_HIGH (offset 0x310): bits 24-31 hold the destination APIC ID.
        wait_icr_idle();

        let icr_high = (cpu & 0xFF) << 24;
        let icr_low = SCHED_IPI_VECTOR
            | ICR_DELIVERY_FIXED
            | ICR_DEST_PHYSICAL
            | ICR_LEVEL_ASSERT;

        // SAFETY: Writing ICR_HIGH then ICR_LOW triggers the IPI send.
        // The order matters: ICR_HIGH must be written first.
        unsafe {
            lapic_write(LAPIC_ICR_HIGH, icr_high);
            lapic_write(LAPIC_ICR_LOW, icr_low);
        }
    }

    fn halt_cpu() {
        // C: arch_smp_halt_cpu() — smp.c:60
        //
        // `hlt` stops instruction execution until the next interrupt.
        // SAFETY: `hlt` is a privileged instruction that halts the CPU
        // until an interrupt. Safe in kernel mode with interrupts enabled.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }

    fn ack_ipi() {
        // C: ipi_ack() — smp.c:58,198
        //
        // Write to the LAPIC EOI register to signal end-of-interrupt.
        // The value written is ignored (should be 0); the write itself
        // clears the in-service bit for the current interrupt.
        // SAFETY: EOI write is required after every interrupt handler.
        unsafe {
            lapic_write(LAPIC_EOI, 0);
        }
    }

    fn boot_ap(cpu: u32, entry: usize) {
        // C: AP boot protocol — arch/i386/smp.c
        //
        // INIT-SIPI-SIPI sequence (Intel SDM Vol. 3A §8.4):
        //   1. Send INIT IPI to target CPU
        //   2. Wait 10ms (de-assert INIT — in xAPIC mode, a single INIT
        //      IPI suffices; the de-assert is a legacy 82489DX requirement)
        //   3. Send SIPI with vector = entry >> 12
        //      (AP starts at CS:IP = vector:0x0000, i.e., physical address
        //       vector << 12)
        //   4. Wait 200us, send second SIPI
        //
        // The `entry` address must be page-aligned and within the first
        // 1 MiB of physical memory (real mode startup address constraint).
        //
        // SIPI vector: the AP starts in real mode at CS = vector, IP = 0,
        // so the entry physical address = vector << 12. The trampoline
        // must be at a page-aligned address < 1 MiB.
        assert!(
            entry & 0xFFF == 0 && entry < 0x10_0000,
            "AP entry must be page-aligned and < 1 MiB, got {:#x}",
            entry
        );
        let sipi_vector = (entry >> 12) as u32;

        // Step 1: Send INIT IPI
        wait_icr_idle();
        let icr_high = (cpu & 0xFF) << 24;
        let icr_init = ICR_DELIVERY_INIT | ICR_DEST_PHYSICAL | ICR_LEVEL_ASSERT;
        unsafe {
            lapic_write(LAPIC_ICR_HIGH, icr_high);
            lapic_write(LAPIC_ICR_LOW, icr_init);
        }

        // Step 2: Wait for INIT delivery.
        // Intel SDM recommends 10ms; we spin-wait on ICR delivery status
        // instead of using a timer, which is simpler in early boot.
        wait_icr_idle();

        // Step 3: Send first SIPI
        let icr_sipi = sipi_vector
            | ICR_DELIVERY_SIPI
            | ICR_DEST_PHYSICAL
            | ICR_LEVEL_ASSERT;
        unsafe {
            lapic_write(LAPIC_ICR_HIGH, icr_high);
            lapic_write(LAPIC_ICR_LOW, icr_sipi);
        }
        wait_icr_idle();

        // Step 4: Send second SIPI (recommended for reliability).
        unsafe {
            lapic_write(LAPIC_ICR_HIGH, icr_high);
            lapic_write(LAPIC_ICR_LOW, icr_sipi);
        }
        wait_icr_idle();
    }

    fn current_cpu() -> u32 {
        // C: `cpuid` macro reads CPU ID from the kernel stack top.
        // Rust: read LAPIC ID register and extract the APIC ID.
        //
        // LAPIC ID Register (offset 0x20): bits 24-31 hold the APIC ID.
        // Intel SDM Vol. 3A §10.4.6.
        //
        // The APIC ID is set by hardware and is unique per logical CPU.
        // In simple configurations (no x2APIC, no hyperthreading), the
        // APIC ID equals the logical CPU ID (0, 1, 2, ...).
        //
        // SAFETY: `lapic_read` reads from the LAPIC MMIO region, which is
        // always mapped in kernel mode. The read has no side effects.
        let lapic_id = unsafe { lapic_read(LAPIC_ID) };
        (lapic_id >> 24) & 0xFF
    }
}

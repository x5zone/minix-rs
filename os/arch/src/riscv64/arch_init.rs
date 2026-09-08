//! RISC-V 64-bit architecture-specific initialization
//!
//! Implements `ArchInit` for RISC-V 64-bit, performing PMP configuration
//! and S-mode interrupt enablement.
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! RISC-V has no architecture-misc parameters, so `new(desc)` is a no-op
//! constructor. The struct exists only to satisfy the instance-based trait
//! contract.
//!
//! C: No Minix3 equivalent (Minix3 has no RISC-V port) — architectural
//! evolution `[ARCH: K-2]` (05-clock-interrupt-init.md §3.8).

use minix_platform::ArchMiscDesc;

use crate::arch_init::ArchInit;

/// RISC-V 64-bit architecture-specific initialization.
///
/// Performs hardware-specific setup after protection structures and
/// interrupt controller are initialized:
///
/// 1. Configure PMP (Physical Memory Protection) to allow all access
/// 2. Enable S-mode interrupts (SIE register)
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port) — architectural
/// evolution `[ARCH: K-2]` (05-clock-interrupt-init.md §3.8).
pub struct Riscv64ArchInit;

impl ArchInit for Riscv64ArchInit {
    fn new(_desc: &ArchMiscDesc) -> Self {
        // RISC-V has no architecture-misc parameters to store.
        Self
    }

    fn init(&mut self) {
        // 1. Configure PMP (Physical Memory Protection)
        // OpenSBI may have already configured PMP entries.
        // We add a default entry that allows all access.
        unsafe {
            // pmpaddr0 = 0xFFFFFFFFFFFFFFFF (match all addresses)
            core::arch::asm!("csrw pmpaddr0, {}", in(reg) u64::MAX);
            // pmpcfg0 = A=NAPOT (0x18) + X+R+W (0x7) = 0x1F
            // This allows all access to all memory regions.
            core::arch::asm!("csrw pmpcfg0, {}", in(reg) 0x1Fu64);
        }

        // 2. (Removed by D-59, 2026-09-09.) This step used to set
        // `sie = STIE | SSIE` (0x22) here — enabling the S-mode timer gate
        // before any handler is registered. Both bits now have owners:
        //   - STIE (timer): opened by `TimerIrqGate::enable_timer_irq` at
        //     bsp_finish_booting Step 6, after the handler registration
        //     (C `boot_cpu_init_timer` position, clock.c:294).
        //   - SSIE (software/IPI): belongs to the SMP IPI bring-up
        //     (smp_todo.md S-7/S-10) — enabling it before an IPI producer
        //     and handler exist would arm a second unhandled source.
        // SEIE (external) stays per-source via `InterruptController::unmask`.
    }
}

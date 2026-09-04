//! RISC-V 64-bit SMP implementation using the SBI (Supervisor Binary
//! Interface).
//!
//! Implements `SmpArch` for RISC-V 64-bit by delegating SMP operations to
//! the SBI firmware (e.g. OpenSBI): IPIs are requested through the legacy
//! `send_ipi` ecall, secondary harts are started through the HSM extension,
//! and the CPU is halted with the `wfi` instruction.
//!
//! # SMP mechanism overview
//!
//! - **IPI send**: The legacy SBI `send_ipi` call (EID 0, FID 3) takes a
//!   hart mask and a hart-mask base. SBI firmware delivers a software
//!   interrupt (SSIP) to each selected hart.
//! - **IPI acknowledge**: No-op. SBI/firmware surfaces IPIs as supervisor
//!   software interrupts; the `sip.SSIP` bit is cleared by the trap
//!   handler, so there is no separate EOI-style acknowledgement for the
//!   `SmpArch` layer to perform.
//! - **CPU halt**: The `wfi` instruction suspends the hart until an
//!   interrupt is pending.
//! - **AP boot**: The SBI HSM extension `hart_start` (EID 0x48534D, FID 0)
//!   powers on a secondary hart at a given start address in S-mode.
//!
//! # Anti-translate vs C
//!
//! Minix3's C SMP layer (arch/i386/smp.c) dispatches architecture operations
//! through global function pointers set during `arch_init`. This Rust
//! backend anti-translates that design: instead of indirect calls through
//! global state, each operation is an inlined `ecall` straight to SBI
//! firmware (or a single `wfi` instruction). There is no per-arch mutable
//! global at all — the hart mask is computed from the `cpu` argument, and
//! SBI tracks all delivery state. This keeps the IPI hot path a direct
//! `ecall` with zero indirection.
//!
//! C: arch/riscv64/smp.c — RISC-V equivalent of arch/i386/smp.c (this file
//!    does not exist in Minix3, which has no RISC-V port; it is the
//!    architecture equivalent of arch/i386/smp.c).

use crate::smp::SmpArch;

/// Legacy SBI extension ID (EID 0). The legacy calling convention places
/// the EID in `a7` and the function ID in `a6`. RISC-V SBI specification
/// §3 (legacy extensions).
const SBI_LEGACY_EID: u64 = 0;

/// Legacy SBI `send_ipi` function ID (FID 3). RISC-V SBI specification
/// §3.3.
const SBI_LEGACY_SEND_IPI_FID: u64 = 3;

/// SBI HSM extension ID (EID 0x48534D, ASCII "HSM"). RISC-V SBI
/// specification §9 (Hart State Management extension).
const SBI_HSM_EID: u64 = 0x48534D;

/// SBI HSM `hart_start` function ID (FID 0). RISC-V SBI specification §9.1.
const SBI_HSM_HART_START_FID: u64 = 0;

/// RISC-V 64-bit SMP backend using SBI.
///
/// This is a zero-sized type — all SMP state lives in SBI firmware, and the
/// target hart is derived from the `cpu` argument at each call.
///
/// C: arch/riscv64/smp.c (architecture equivalent of arch/i386/smp.c;
///    Minix3 has no RISC-V port)
pub struct Riscv64SmpArch;

impl SmpArch for Riscv64SmpArch {
    #[inline]
    fn send_sched_ipi(cpu: u32) {
        // C: arch_send_smp_schedule_ipi(cpu) — smp.c:65
        //
        // Legacy SBI send_ipi (RISC-V SBI specification §3.3):
        //   a7 = 0           (legacy EID)
        //   a6 = 3           (legacy FID for send_ipi)
        //   a0 = hart mask   (one bit per hart; bit n = hart n)
        //   a1 = hart_mask_base (hart id of bit 0; 0 for hart 0)
        // The legacy mask covers harts 0..63 relative to hart_mask_base, so
        // a single CPU maps to mask = 1 << cpu with base 0. The CPU id is
        // masked to 6 bits to stay within the 64-bit mask and to keep the
        // shift in range (no undefined shift).
        let hart_mask: u64 = 1u64 << (cpu & 0x3F);
        let hart_mask_base: u64 = 0;
        let ret: u64;

        // SAFETY: `ecall` traps to SBI firmware. All argument registers
        // (a0, a1, a6, a7) are initialized. The call raises a supervisor
        // software interrupt on the targeted hart(s); it may synchronize
        // with the target hart, so `nomem` is not set. `nostack` is set
        // because ecall does not access the stack. The SBI return code is
        // delivered in a0.
        unsafe {
            core::arch::asm!(
                "ecall",
                inout("a0") hart_mask => ret,
                in("a1") hart_mask_base,
                in("a6") SBI_LEGACY_SEND_IPI_FID,
                in("a7") SBI_LEGACY_EID,
                options(nostack),
            );
        }
        // Legacy SBI returns 0 on success in a0; non-zero is an error. We
        // discard the result to match the trait's fire-and-forget contract.
        let _ = ret;
    }

    #[inline]
    fn halt_cpu() {
        // C: arch_smp_halt_cpu() — smp.c:60
        //
        // `wfi` (Wait For Interrupt) suspends the hart until a pending
        // interrupt (SSIE/STIE/SEIE) is observed. RISC-V Privileged ISA
        // §3.3.7.
        //
        // SAFETY: `wfi` is a hint instruction and never faults. Safe in
        // S-mode with interrupts enabled (otherwise it may execute as a
        // no-op or spin indefinitely per the spec). `nomem`/`nostack`
        // mirror the x86_64 `hlt` usage.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }

    fn idle_halt() {
        // C: halt_cpu() — klib.S:407-414 (idle-loop variant: `sti; hlt`).
        //
        // riscv64 equivalent: set `sstatus.SIE` (bit 5 — supervisor
        // interrupts globally enabled in S-mode) then `wfi`. Same
        // requirement as x86_64/aarch64: the wait must not sleep with
        // interrupts masked, or the hart never wakes.
        //
        // SAFETY: `csrs sstatus` and `wfi` are privileged but confined:
        // the first only enables interrupts, the second only sleeps the
        // hart. Safe in S-mode kernel context.
        unsafe {
            core::arch::asm!(
                "csrs sstatus, {sie}",
                "wfi",
                sie = in(reg) 1u64 << 5, // sstatus.SIE
                options(nomem, nostack)
            );
        }
    }

    #[inline]
    fn ack_ipi() {
        // C: ipi_ack() — smp.c:58,198
        //
        // No-op for RISC-V with SBI. IPIs arrive as supervisor software
        // interrupts (SSIP). SBI firmware owns interrupt injection, and
        // the trap handler is responsible for clearing `sip.SSIP`
        // (typically via `csrc sip, SSIP` or an SBI call) as part of
        // dispatch — not the `SmpArch` layer. There is no GIC-style EOI
        // register to write, so `ack_ipi` has nothing to do.
    }

    #[inline]
    fn boot_ap(cpu: u32, entry: usize) {
        // C: AP boot protocol — arch/i386/smp.c (architecture equivalent)
        //
        // SBI HSM extension `hart_start` (RISC-V SBI specification §9.1):
        //   a7 = 0x48534D   (EID, HSM)
        //   a6 = 0          (FID, hart_start)
        //   a0 = hartid     (target hart to start)
        //   a1 = start_addr (physical address of AP entry/trampoline)
        //   a2 = priv       (0 = S-mode)
        // On success a0 = 0; otherwise a0 holds an SBI error code (e.g.
        // SBI_ERR_ALREADY_AVAILABLE if the hart is already running).
        let hartid = cpu as u64;
        let start_addr = entry as u64;
        let priv_mode: u64 = 0;
        let ret: u64;

        // SAFETY: `ecall` traps to SBI firmware. All argument registers
        // (a0, a1, a2, a6, a7) are initialized. The call starts a
        // secondary hart at `start_addr` in S-mode; it may synchronize
        // with the incoming hart, so `nomem` is not set. `nostack` is set
        // because ecall does not use the stack. The SBI return code is
        // delivered in a0.
        unsafe {
            core::arch::asm!(
                "ecall",
                inout("a0") hartid => ret,
                in("a1") start_addr,
                in("a2") priv_mode,
                in("a6") SBI_HSM_HART_START_FID,
                in("a7") SBI_HSM_EID,
                options(nostack),
            );
        }
        // Discard the SBI return code: the trait contract is
        // fire-and-forget, mirroring the C port's best-effort AP boot.
        let _ = ret;
    }

    fn current_cpu() -> u32 {
        // C: `cpuid` reads from per-CPU kernel stack top.
        // Rust: read the RISC-V hart ID via SBI.
        //
        // In S-mode, `mhartid` CSR is not directly accessible (it's
        // M-mode only). The hart ID is typically passed to S-mode via
        // a0 during boot and stored in a per-CPU variable.
        //
        // For now, we use `sscratch` which stores the per-CPU hart ID
        // (set by the trap entry path). If sscratch is 0, we're on the
        // BSP (hart 0).
        //
        // SAFETY: `csrr` reads a CSR. `sscratch` is a scratch register
        // with no side effects. It is safe to read in S-mode.
        let hartid: u64;
        unsafe {
            core::arch::asm!(
                "csrr {0}, sscratch",
                out(reg) hartid,
                options(nomem, nostack),
            );
        }
        hartid as u32
    }
}

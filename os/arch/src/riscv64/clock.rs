//! RISC-V 64-bit clock implementation using CLINT mtime
//!
//! Implements `ClockArch` for RISC-V 64-bit using the CLINT (Core Local
//! Interruptor) mtime register. The mtime register is a memory-mapped
//! counter that increments at a fixed frequency.
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! Hardware parameters (CLINT mtime/mtimecmp addresses, frequency) are
//! stored in instance fields, populated by `new(desc)` via `Any` downcast
//! to `ClintDesc`. This replaces the previous hardcoded
//! `CLINT_MTIME` / `CLINT_MTIMECMP` / `MTIME_FREQ` constants.
//!
//! C: No Minix3 equivalent (Minix3 has no RISC-V port) — architectural
//! evolution `[ARCH: K-2]` (05-clock-interrupt-init.md §3.8).

use minix_platform::arch::riscv64::ClintDesc;

use crate::clock::{ClockArch, ProfileClockError};

/// SBI TIME extension EID ("TIME", RISC-V SBI Specification §6.1) — the
/// S-mode timer programming channel under M-mode firmware.
const SBI_EXT_TIME: usize = 0x5449_4D45;
/// SBI TIME function 0: `sbi_set_timer(stime_value)`.
const SBI_TIME_SET_TIMER_FID: usize = 0;

/// Program this hart's next S-mode timer deadline through the SBI TIME
/// ecall.
///
/// Under OpenSBI the CLINT/ACLINT-MTIMER frame is PMP-protected M-mode
/// memory (live: an S-mode MMIO read took access fault scause=5,
/// stval=0x200bff8 — see `read_ticks`), so both arming and re-arming go
/// through the SBI: on firmware with Sstc delegated the ecall writes the
/// S-mode `stimecmp`, otherwise M-mode programs `mtimecmp` on our behalf.
/// Either way the deadline semantics are identical to a direct MMIO
/// write and `sie.STIE` remains the delivery gate.
fn sbi_set_timer(stime_value: u64) {
    let error: usize;
    // SAFETY: the SBI ecall is the architectural S→M service call; a0 is
    // both the argument and the error return, a1 the value return
    // (unused), a6/a7 select function and extension.
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") stime_value as usize => error,
            lateout("a1") _,
            in("a6") SBI_TIME_SET_TIMER_FID,
            in("a7") SBI_EXT_TIME,
            options(nostack),
        );
    }
    // A non-zero SBI error here would mean the firmware lacks the TIME
    // extension — arming failed and the tick train stops. Fail loudly
    // instead of silently losing the clock.
    assert!(error == 0, "sbi_set_timer failed: SBI error {error}");
}

/// Interval between ticks in time-counter units, armed by `init_timer`
/// and re-read by the per-tick re-arm. A static (not instance state)
/// because the kernel constructs transient `ClockArch` instances per
/// call — see `kernel::clock::local_tick`.
static TICK_INTERVAL: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// RISC-V 64-bit clock using CLINT mtime.
///
/// The CLINT provides:
/// - mtime: 64-bit memory-mapped counter (read-only)
/// - mtimecmp: 64-bit compare register per hart (read-write)
///
/// When mtime >= mtimecmp, a timer interrupt is generated.
/// The handler must update mtimecmp to schedule the next interrupt.
///
/// # Fields
///
/// - `mtime_addr`: CLINT mtime register MMIO address, from `ClintDesc`.
/// - `mtimecmp_base`: CLINT mtimecmp base address (hart 0), from `ClintDesc`.
///   Each hart's comparator lives at `mtimecmp_base + hart_id * mtimecmp_stride`.
/// - `mtimecmp_stride`: per-hart mtimecmp spacing, from `ClintDesc`.
/// - `freq`: mtime counter frequency (Hz), from `ClintDesc`.
///
/// C: No Minix3 equivalent (Minix3 has no RISC-V port).
pub struct Riscv64ClockArch {
    mtime_addr: usize,
    /// CLINT `mtimecmp` base address (hart 0). This hart's comparator is
    /// `mtimecmp_base + hart_id * mtimecmp_stride`.
    mtimecmp_base: usize,
    /// Byte distance between consecutive harts' `mtimecmp` registers.
    mtimecmp_stride: usize,
    freq: u64,
}

impl Riscv64ClockArch {
    /// Read the `time` CSR (the S-mode view of mtime) — the only legal
    /// counter read under OpenSBI's PMP layout (see `read_ticks`).
    fn read_time_csr() -> u64 {
        let ticks: u64;
        // SAFETY: reading the `time` CSR is a side-effect-free S-mode
        // operation (Time extension, RV64); nomem/nostack per the usual
        // CSR-read contract.
        unsafe {
            core::arch::asm!(
                "csrr {}, time",
                out(reg) ticks,
                options(nomem, nostack)
            );
        }
        ticks
    }
}

impl ClockArch for Riscv64ClockArch {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
        let clint = desc.as_any()
            .downcast_ref::<ClintDesc>()
            .expect("Riscv64ClockArch::new: expected ClintDesc");
        Self {
            mtime_addr: clint.mtime_addr,
            mtimecmp_base: clint.mtimecmp_base,
            mtimecmp_stride: clint.mtimecmp_stride,
            freq: clint.freq,
        }
    }

    fn init_timer(&mut self, hz: u32, _cpu_id: u32) {
        // Arm the first S-mode timer deadline via the SBI TIME ecall
        // (the CLINT MMIO comparator is PMP-protected M-mode memory
        // under OpenSBI — see `sbi_set_timer`). CNTP-style module gates
        // do not exist here: arming alone never raises an interrupt
        // while `sie.STIE` is clear, and opening that gate is
        // `TimerIrqGate::enable_timer_irq`'s job (D-59 ordering —
        // handler registered first, gate opened at bsp_finish_booting
        // Step 6).
        //
        // C: No Minix3 equivalent (`[ARCH: K-2]`); the three-part
        // sequence parity (program → register → open) mirrors
        // init_local_timer / register_local_timer_handler
        // (arch_clock.c:177-196).
        let interval = self.freq / hz as u64;
        let now = Self::read_time_csr();
        // SAFETY: an SBI ecall with a deadline in the future; see the
        // safety note on `sbi_set_timer`.
        unsafe {
            sbi_set_timer(now + interval);
        }
        TICK_INTERVAL.store(interval, core::sync::atomic::Ordering::Relaxed);
    }

    fn read_ticks(&self) -> u64 {
        // The S-mode time read goes through the `time` CSR (the privileged
        // spec's S-mode view of mtime), NOT the MMIO register: under
        // OpenSBI the CLINT/ACLINT-MTIMER frame is PMP-protected M-mode
        // memory (QEMU virt Domain0 Region00: S/U access ()), so an MMIO
        // read takes an access fault (live: scause=5, stval=0x200bff8 in
        // the first riscv64 scheduler run). The `time` CSR — 0xC01, the
        // `rdtime` instruction's target — reads the same counter and is
        // S-mode legal. The MMIO addresses stay in the descriptor for the
        // M-mode-only timer-programming paths.
        Self::read_time_csr()
    }

    fn stop_local_timer(&mut self, _cpu_id: u32) {
        // Disarm via the SBI (an mtimecmp MMIO write would take the
        // PMP access fault documented in `read_ticks`): a deadline at
        // u64::MAX never fires, and clearing `sie.STIE` closes the
        // delivery gate as well.
        //
        // C: smp.c:56-61 — inline timer disable in smp_ipi_halt_handler
        unsafe {
            sbi_set_timer(u64::MAX);
            // Clear STIE (bit 5) in sie — close the delivery gate too.
            core::arch::asm!("csrc sie, {bits}", bits = in(reg) 0x20u64);
        }
    }

    fn local_timer_eoi(&mut self) {
        // Re-arm the one-shot: the mtimecmp-style deadline is consumed by
        // each interrupt (unlike a periodic PIT), so the tick body must
        // schedule the next deadline or the tick train stops dead — the
        // riscv64 analogue of the x86 LAPIC ICR re-arm
        // (`kernel::clock::local_tick` calls this after every tick).
        let interval = TICK_INTERVAL.load(core::sync::atomic::Ordering::Relaxed);
        let interval = if interval == 0 {
            // `local_timer_eoi` before any `init_timer` would be a wiring
            // bug (D-59: the gate opens only after the source is armed);
            // fall back to the descriptor frequency at DEFAULT_HZ rather
            // than arming a zero interval (an interrupt storm).
            self.freq / crate::clock::DEFAULT_HZ as u64
        } else {
            interval
        };
        let now = Self::read_time_csr();
        // SAFETY: an SBI ecall with a deadline in the future; see the
        // safety note on `sbi_set_timer`.
        unsafe {
            sbi_set_timer(now + interval);
        }
    }

    fn init_profile_clock(&mut self, _hz: u32) -> Result<(), ProfileClockError> {
        // RISC-V does not have a separate profiling timer. The CLINT
        // mtimecmp is already used for scheduling. A second mtimecmp
        // (if available for S-mode) could be used, but this is not
        // standardized in the privilege spec.
        //
        // Return Err to indicate profiling is not available on RISC-V.
        Err(ProfileClockError::Unsupported)
    }

    fn stop_profile_clock(&mut self) {
        // No profiling timer to stop on RISC-V.
    }

    fn ack_profile_clock(&mut self) {
        // No profiling timer to ack on RISC-V.
        // C: arch_ack_profile_clock() — profile.c:123
    }
}

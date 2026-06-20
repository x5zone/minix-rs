//! x86-64 clock implementation using 8254 PIT
//!
//! Implements `ClockArch` for x86-64. The 8254 PIT (Programmable Interval
//! Timer) is used as the boot-time clock source. LAPIC Timer may replace
//! it after APIC initialization.
//!
//! # Instance-based design (see `plat-design.md` §5.1)
//!
//! Hardware parameters (PIT base frequency, LAPIC base address) are stored
//! in instance fields, populated by `new(desc)` from `TimerDesc::Pit`.
//! This replaces the previous hardcoded `PIT_BASE_FREQ` constant.
//!
//! C: clock.c hardware init + apic.c lapic_enable()

use minix_platform::TimerDesc;

use crate::clock::ClockArch;

/// PIT command port (channel 0, lobyte/hibyte access).
const PIT_COMMAND: u16 = 0x43;
/// PIT channel 0 data port.
const PIT_CHANNEL0: u16 = 0x40;
/// PIT command: channel 0, lobyte/hibyte access, rate generator mode.
const PIT_CMD_RATE_GEN: u8 = 0x36;

/// x86-64 clock using 8254 PIT as boot timer.
///
/// The PIT is configured in rate generator mode (square wave) with
/// a divisor calculated from the desired tick frequency. After APIC
/// initialization, the LAPIC Timer may be used instead.
///
/// # Fields
///
/// - `pit_base_freq`: 8254 PIT base frequency (Hz), from `TimerDesc::Pit`.
/// - `lapic_base`: LAPIC MMIO base address, from `TimerDesc::Pit`.
///   Used for LAPIC Timer setup after APIC init.
///
/// C: clock.c hardware init + apic.c lapic_enable()
pub struct X86_64ClockArch {
    pit_base_freq: u32,
    #[allow(dead_code)]
    lapic_base: usize,
}

impl ClockArch for X86_64ClockArch {
    fn new(desc: &TimerDesc) -> Self {
        match desc {
            TimerDesc::Pit { pit_base_freq, lapic_base } => Self {
                pit_base_freq: *pit_base_freq,
                lapic_base: *lapic_base,
            },
            _ => panic!(
                "X86_64ClockArch::new: expected TimerDesc::Pit, got {:?}",
                desc
            ),
        }
    }

    fn init_timer(&mut self, hz: u32) {
        // Configure 8254 PIT channel 0 for periodic mode.
        // C: intr_init_8254() — i8259.c equivalent
        //
        // PIT divisor is 16-bit, so hz must be >= 19 (1193182 / 65535 ≈ 18.2).
        // Values below 19 would overflow the divisor.
        assert!(hz >= 19, "PIT divisor overflow: hz must be >= 19, got {}", hz);

        let divisor = (self.pit_base_freq / hz) as u16;

        unsafe {
            // Send command byte: channel 0, lobyte/hibyte, rate generator
            core::arch::asm!("out dx, al", in("dx") PIT_COMMAND, in("al") PIT_CMD_RATE_GEN);
            // Send divisor low byte
            let lo = divisor as u8;
            core::arch::asm!("out dx, al", in("dx") PIT_CHANNEL0, in("al") lo);
            // Send divisor high byte
            let hi = (divisor >> 8) as u8;
            core::arch::asm!("out dx, al", in("dx") PIT_CHANNEL0, in("al") hi);
        }
    }

    fn read_ticks(&self) -> u64 {
        // Use TSC (Time Stamp Counter) for high-resolution tick reading.
        // NOTE: TSC frequency varies across CPUs and is not calibrated here.
        // This value should only be used for relative timing (deltas), not
        // absolute time conversion. For absolute time, use ClockState::uptime.
        //
        // **Multi-core invariant**: ensure CPUID.80000007H:EDX[8] (Invariant TSC)
        // is set on every logical CPU. Without it, TSC offsets differ across
        // cores (due to per-core reset or warm-reset) and deltas computed
        // across CPUs are meaningless. If Invariant TSC is unavailable, fall
        // back to LAPIC TSC-deadline timer (which uses a per-core offset
        // table) or HPET instead.
        //
        // C: read_tsc() — not in Minix3, but standard x86-64 practice
        let tsc: u64;
        unsafe {
            core::arch::asm!("rdtsc", out("rax") tsc, out("rdx") _, options(nomem));
        }
        tsc
    }
}

//! x86-64 clock implementation using 8254 PIT
//!
//! Implements `ClockArch` for x86-64. The 8254 PIT (Programmable Interval
//! Timer) is used as the boot-time clock source. LAPIC Timer may replace
//! it after APIC initialization.
//!
//! C: clock.c hardware init + apic.c lapic_enable()

use crate::clock::ClockArch;

/// 8254 PIT base frequency in Hz.
const PIT_BASE_FREQ: u32 = 1_193_182;

/// PIT command port.
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
/// C: clock.c hardware init + apic.c lapic_enable()
pub struct X86_64ClockArch;

impl ClockArch for X86_64ClockArch {
    fn init_timer(hz: u32) {
        // Configure 8254 PIT channel 0 for periodic mode.
        // C: intr_init_8254() — i8259.c equivalent
        let divisor = (PIT_BASE_FREQ / hz) as u16;

        unsafe {
            // Send command byte: channel 0, lobyte/hibyte, rate generator
            core::arch::asm!("out 0x43, al", in("al") PIT_CMD_RATE_GEN);
            // Send divisor low byte
            let lo = divisor as u8;
            core::arch::asm!("out 0x40, al", in("al") lo);
            // Send divisor high byte
            let hi = (divisor >> 8) as u8;
            core::arch::asm!("out 0x40, al", in("al") hi);
        }
    }

    fn read_ticks() -> u64 {
        // Use TSC (Time Stamp Counter) for high-resolution tick reading.
        // C: read_tsc() — not in Minix3, but standard x86-64 practice
        let tsc: u64;
        unsafe {
            core::arch::asm!("rdtsc", out("rax") tsc, out("rdx") _, options(nomem));
        }
        tsc
    }
}

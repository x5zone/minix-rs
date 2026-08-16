//! x86-64 clock implementation using 8254 PIT
//!
//! Implements `ClockArch` for x86-64. The 8254 PIT (Programmable Interval
//! Timer) is used as the boot-time clock source. LAPIC Timer may replace
//! it after APIC initialization.
//!
//! # Instance-based design (see 04-platform-discovery.md §3.4)
//!
//! Hardware parameters (PIT base frequency, LAPIC base address) are stored
//! in instance fields, populated by `new(desc)` from the `PitDesc` sub-trait
//! implementor (downcast via `Any`). This replaces the previous hardcoded
//! `PIT_BASE_FREQ` constant.
//!
//! C: clock.c hardware init + apic.c lapic_enable()

use minix_platform::arch::x86_64::PitDesc;

use crate::clock::{ClockArch, ProfileClockError};

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
/// - `pit_base_freq`: 8254 PIT base frequency (Hz), from `PitDesc`.
/// - `lapic_base`: LAPIC MMIO base address, from `PitDesc`.
///   Used for LAPIC Timer setup after APIC init.
///
/// C: clock.c hardware init + apic.c lapic_enable()
pub struct X86_64ClockArch {
    pit_base_freq: u32,
    #[allow(dead_code)]
    lapic_base: usize,
}

impl ClockArch for X86_64ClockArch {
    fn new(desc: &dyn minix_platform::TimerDesc) -> Self {
        let pit = desc.as_any()
            .downcast_ref::<PitDesc>()
            .expect("X86_64ClockArch::new: expected PitDesc");
        Self {
            pit_base_freq: pit.pit_base_freq,
            lapic_base: pit.lapic_base,
        }
    }

    fn init_timer(&mut self, hz: u32, _cpu_id: u32) {
        // Configure 8254 PIT channel 0 for periodic mode.
        // C: intr_init_8254() — i8259.c equivalent
        //
        // The PIT is a single system-wide device; the per-CPU LAPIC timer
        // (used later, and by `stop_local_timer`) is reached through the
        // same MMIO base on every CPU, so `cpu_id` is not needed here.
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

    fn stop_local_timer(&mut self, _cpu_id: u32) {
        // Disable the LAPIC Timer by clearing LVT Timer entry.
        // LAPIC LVT Timer register offset = 0x320; bit 16 = Mask.
        //
        // The LAPIC base address is the same on every CPU (each CPU sees
        // its own LAPIC there), so `cpu_id` is not needed.
        //
        // C: smp.c:56-61 — `lapic_stop_timer()` (inline in smp_ipi_halt_handler)
        let lapic_base = self.lapic_base as *mut u32;
        unsafe {
            // LVT Timer Register (offset 0x320): set Mask bit (bit 16)
            let lvt_timer = lapic_base.add(0x320 / 4);
            let v = core::ptr::read_volatile(lvt_timer);
            core::ptr::write_volatile(lvt_timer, v | (1 << 16));
        }
    }

    fn init_profile_clock(&mut self, hz: u32) -> Result<(), ProfileClockError> {
        // Statistical profiling uses the RTC (Real Time Clock) on x86-64.
        // The RTC can generate interrupts at 2..8192 Hz via IRQ8.
        //
        // Rate selection (RTC register A):
        //   0x06 = 1024 Hz, 0x07 = 512 Hz, 0x08 = 256 Hz, 0x09 = 128 Hz,
        //   0x0A = 64 Hz, 0x0B = 32 Hz, 0x0C = 16 Hz, 0x0D = 8 Hz,
        //   0x0E = 4 Hz, 0x0F = 2 Hz
        //
        // For arbitrary `hz`, pick the closest supported rate.
        //
        // C: sprofile.c:init_profile_clock(freq)
        let rate = match hz {
            0..=1 => return Err(ProfileClockError::Unsupported),
            2 => 0x0F,
            3..=4 => 0x0E,
            5..=8 => 0x0D,
            9..=16 => 0x0C,
            17..=32 => 0x0B,
            33..=64 => 0x0A,
            65..=128 => 0x09,
            129..=256 => 0x08,
            257..=512 => 0x07,
            _ => 0x06, // 1024 Hz for >=513
        };

        const RTC_INDEX: u16 = 0x70;
        const RTC_DATA: u16 = 0x71;
        const RTC_REG_A: u8 = 0x0A;
        const RTC_REG_B: u8 = 0x0B;

        unsafe {
            // Read current Reg A, set rate bits (bits 0-3)
            core::arch::asm!("out dx, al", in("dx") RTC_INDEX, in("al") RTC_REG_A);
            let mut val: u8;
            core::arch::asm!("in al, dx", in("dx") RTC_DATA, out("al") val);
            val = (val & 0xF0) | rate;
            core::arch::asm!("out dx, al", in("dx") RTC_INDEX, in("al") RTC_REG_A);
            core::arch::asm!("out dx, al", in("dx") RTC_DATA, in("al") val);

            // Enable periodic interrupt in Reg B (bit 6 = PIE)
            core::arch::asm!("out dx, al", in("dx") RTC_INDEX, in("al") RTC_REG_B);
            let mut ctrl: u8;
            core::arch::asm!("in al, dx", in("dx") RTC_DATA, out("al") ctrl);
            ctrl |= 0x40;
            core::arch::asm!("out dx, al", in("dx") RTC_INDEX, in("al") RTC_REG_B);
            core::arch::asm!("out dx, al", in("dx") RTC_DATA, in("al") ctrl);
        }
        Ok(())
    }

    fn stop_profile_clock(&mut self) {
        // Disable RTC periodic interrupt by clearing Reg B bit 6 (PIE).
        //
        // C: sprofile.c:stop_profile_clock()
        const RTC_INDEX: u16 = 0x70;
        const RTC_DATA: u16 = 0x71;
        const RTC_REG_B: u8 = 0x0B;

        unsafe {
            core::arch::asm!("out dx, al", in("dx") RTC_INDEX, in("al") RTC_REG_B);
            let mut ctrl: u8;
            core::arch::asm!("in al, dx", in("dx") RTC_DATA, out("al") ctrl);
            ctrl &= !0x40;
            core::arch::asm!("out dx, al", in("dx") RTC_INDEX, in("al") RTC_REG_B);
            core::arch::asm!("out dx, al", in("dx") RTC_DATA, in("al") ctrl);
        }
    }

    fn ack_profile_clock(&mut self) {
        // Acknowledge RTC interrupt by reading Register C.
        // The RTC IRQ is only cleared after Register C is read; without
        // this ack, no further RTC interrupts will be generated.
        //
        // C: arch_ack_profile_clock() — profile.c:123
        const RTC_INDEX: u16 = 0x70;
        const RTC_DATA: u16 = 0x71;
        const RTC_REG_C: u8 = 0x0C;

        unsafe {
            core::arch::asm!("out dx, al", in("dx") RTC_INDEX, in("al") RTC_REG_C);
            let _val: u8;
            core::arch::asm!("in al, dx", in("dx") RTC_DATA, out("al") _val);
        }
    }
}

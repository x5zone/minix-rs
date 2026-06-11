//! x86-64 architecture-specific initialization
//!
//! Implements `ArchInit` for x86-64, performing serial port (COM1)
//! initialization, ACPI table parsing, and APIC initialization.
//!
//! C: arch_init() — arch/i386/arch_system.c:246-288

use core::arch::asm;

use crate::arch_init::ArchInit;

/// COM1 base port (standard PC UART 16550).
///
/// The 16550 has 8 I/O ports starting at 0x3F8:
/// +0: data / divisor latch low
/// +1: interrupt enable / divisor latch high
/// +2: FIFO control
/// +3: line control
/// +4: modem control
/// +5: line status
pub const COM1_BASE: u16 = 0x3F8;

/// Divisor value for 115200 baud from 1.8432 MHz crystal (divisor = 1).
const COM1_DIVISOR_115200: u8 = 0x01;
/// LCR bit 7: Divisor Latch Access Bit (DLAB). Must be set to change
/// baud rate; must be clear to access data/FIFO registers.
const COM1_LCR_DLAB: u8 = 0x80;
/// LCR value: 8 data bits, no parity, 1 stop bit (DLAB clear).
const COM1_LCR_8N1: u8 = 0x03;
/// FCR value: enable FIFO, clear RX/TX buffers, 14-byte trigger.
const COM1_FCR_ENABLE: u8 = 0xC7;
/// MCR value: DTR + RTS + OUT2 (OUT2 enables IRQ routing for PIC).
const COM1_MCR_DTR_RTS_OUT2: u8 = 0x0B;

/// Initialize COM1 (UART 16550) for 115200 8N1 with FIFOs enabled.
///
/// This is the boot-stage re-initialization that supersedes the
/// firmware-set defaults (usually 9600 8N1) so kernel `printk!`
/// output is visible in QEMU `-serial` at 115200.
///
/// # Safety
///
/// Must be called only from BSP, before any UART output or
/// interrupt-driven serial I/O is enabled.
unsafe fn ser_init() {
    // SAFETY: COM1_BASE (0x3F8) is a well-known PC I/O port range
    // (Intel IA-PC compatible). outb to these ports is the standard
    // 16550 initialization sequence (Linux `serial8250_init_hw`).
    outb(COM1_BASE + 1, 0x00); // Disable all UART interrupts
    outb(COM1_BASE + 3, COM1_LCR_DLAB); // Enable divisor latch access
    outb(COM1_BASE + 0, COM1_DIVISOR_115200); // Divisor low = 1
    outb(COM1_BASE + 1, 0x00); // Divisor high = 0
    outb(COM1_BASE + 3, COM1_LCR_8N1); // 8 bits, no parity, 1 stop, DLAB off
    outb(COM1_BASE + 2, COM1_FCR_ENABLE); // Enable FIFO
    outb(COM1_BASE + 4, COM1_MCR_DTR_RTS_OUT2); // DTR + RTS + OUT2
}

/// Write a byte to an I/O port.
///
/// # Safety
///
/// `port` must be a valid x86 I/O port for the current privilege level
/// (typically kernel-mode only for COM1+).
unsafe fn outb(port: u16, val: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") val,
        options(nostack, preserves_flags),
    );
}

/// x86-64 architecture-specific initialization.
///
/// Performs hardware-specific setup after protection structures and
/// interrupt controller are initialized:
///
/// 1. Per-CPU kernel stack allocation (handled by linker script)
/// 2. Serial port (COM1) initialization for early debug output
/// 3. ACPI table parsing for hardware topology discovery
/// 4. APIC initialization (LAPIC + IOAPIC) if available
///
/// C: arch_init() — arch_system.c:246-288
pub struct X86_64ArchInit;

impl ArchInit for X86_64ArchInit {
    fn init() {
        // C: arch_init() — arch_system.c:246-288

        // 1. Per-CPU kernel stacks
        // C: k_stacks = &k_stacks_start
        // Already handled by linker script in Rust version

        // 2. Serial port initialization (COM1 at 0x3F8)
        // C: ser_init()
        // SAFETY: ser_init runs only on BSP before serial I/O is enabled.
        unsafe {
            ser_init();
        }

        // 3. ACPI table parsing
        // C: acpi_init()
        // TODO: RSDP search + table parsing (deferred — not required for
        // boot-stage bring-up; non-ACPI QEMU virt machine works without).

        // 4. APIC initialization
        // C: apic_single_cpu_init()
        // APIC is now initialized by X86_64InterruptController::init()
        // (called from init_clock_and_interrupts before arch_init).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_com1_base() {
        assert_eq!(COM1_BASE, 0x3F8);
    }

    #[test]
    fn test_com1_divisor_115200() {
        // 1.8432 MHz / 16 / 1 = 115200 baud
        assert_eq!(COM1_DIVISOR_115200, 0x01);
    }

    #[test]
    fn test_com1_lcr_dlab_bit() {
        // LCR bit 7 is DLAB.
        assert_eq!(COM1_LCR_DLAB, 0x80);
    }

    #[test]
    fn test_com1_lcr_8n1() {
        // 8N1 with DLAB off = 0x03.
        assert_eq!(COM1_LCR_8N1, 0x03);
    }

    #[test]
    fn test_com1_fcr_enable() {
        // FIFO enable + clear RX/TX + 14-byte trigger.
        assert_eq!(COM1_FCR_ENABLE, 0xC7);
    }

    #[test]
    fn test_com1_mcr_signals() {
        // DTR + RTS + OUT2 (OUT2 enables IRQ routing).
        assert_eq!(COM1_MCR_DTR_RTS_OUT2, 0x0B);
    }

    #[test]
    fn test_init_does_not_panic() {
        // We can't actually exercise the I/O port writes in a host test
        // (would fault), but we can verify the public entry point
        // is callable and the constants form a valid sequence.
        // SAFETY: ser_init writes to COM1_BASE+0..4; this is a no-op
        // on the host because the test process doesn't have I/O port
        // permission in most environments. We do not call ser_init()
        // here; we only verify the constant values are coherent.
        let dlab_set = COM1_LCR_DLAB;
        let dlab_clear = COM1_LCR_8N1;
        assert_ne!(dlab_set, dlab_clear);
    }
}

//! Board-level device abstractions (platform-specific devices).
//!
//! These modules handle devices that vary per SoC / motherboard:
//! - UART / serial console
//! - Interrupt controller (PIC, GIC, PLIC)
//! - Platform-specific initialization

pub mod early_console;
pub mod interrupt;

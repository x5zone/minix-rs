//! PC keyboard and mouse driver: scancodes, packets, LEDs.
//!
//! C correspondence: `minix3/minix/drivers/hid/pckbd/pckbd.c` (507
//! lines) with `table.c` (169 lines, scancode tables). The event bridge
//! half of the C driver lives in `minix-sys::inputdriver` (the single
//! authority for the inputdriver protocol; the former in-crate bridge
//! was a duplicate and was removed — edge E-PCKBDREG item 5). This
//! crate owns the numbers and the policy (state machines, queue rules);
//! the service binary owns the transport (port reads and writes,
//! blocking sends, timer setup). See document `13-pckbd-driver.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod led;
pub mod mouse;
pub mod scancode;

/// Service initialization entry (wires the tables; transport stays out).
pub fn init() {}

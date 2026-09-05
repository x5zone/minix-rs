//! Network driver framework: request protocol, port access, and dispatch.
//!
//! C correspondence: `minix3/minix/lib/libnetdriver/netdriver.c` (993
//! lines, framework plus send/receive queues plus status reports),
//! `minix3/minix/lib/libnetdriver/portio.c` (193 lines, port-based input
//! and output helpers), `minix3/minix/include/minix/netdriver.h` (callback
//! table), the network request constants in
//! `minix3/minix/include/minix/com.h:1085-1144`, and the sizing constants in
//! `minix3/minix/include/minix/config.h:102-104`.
//!
//! The framework sits between the TCP/IP stack (request side) and one
//! network card driver (device side). It owns the send queue, the receive
//! queue, the link and statistics bookkeeping, and the grant-vector copy
//! helpers; the card driver owns the wire. This crate renders that split in
//! Rust, organized to match document `03-netdriver-framework.md`:
//!
//! - [`protocol`] — request numbers, modes, capabilities, flags, hardware
//!   addresses, queue bounds (document sections 1 and 2).
//! - [`portio`] — the port-access abstraction (document section 3).
//! - [`driver`] — the driver trait, routing rules, and the server state
//!   machine (document sections 3 and 4).
//!
//! All drivers built on this framework are single-threaded event loops: one
//! message at a time, no shared mutable state across threads.

#![no_std]

extern crate alloc;

pub mod driver;
pub mod portio;
pub mod protocol;

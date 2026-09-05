//! Minix-RS lightweight network service: startup plus buffer pool.
//!
//! C correspondence: `minix3/minix/net/lwip/lwip.c` (382 lines,
//! startup chain plus main loop), `mibtree.c` (141 lines,
//! management-tree registration), `mempool.c` (821 lines, custom
//! buffer pool), `pchain.c` (154 lines, chain tools). This crate
//! owns the order and sizing halves (which step comes when, how big
//! each slice is); the service binary owns message traffic and pool
//! storage. See documents `03-lwip-main-init.md` and
//! `04-lwip-mempool.md` in
//! `notes/rewrite/fork-syscall-rewrite/17-stage-net/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod addr;
pub mod ethif;
pub mod ifaddr;
pub mod ifdev;
pub mod ipsock;
pub mod lnksock;
pub mod mcast;
pub mod mempool;
pub mod ndev;
pub mod pktsock;
pub mod rawsock;
pub mod startup;
pub mod tcpsock;
pub mod udpsock;
pub mod util;

/// Service initialization entry (wires the tables; traffic and storage stay out).
pub fn init() {}

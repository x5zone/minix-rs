//! Virtio network card: queue roles plus refill discipline.
//!
//! C correspondence: `minix3/minix/drivers/net/virtio_net/virtio_net.c`
//! (446 lines). The driver registers `virtio_net_table`
//! (`virtio_net.c:92-99`, name plus init, stop, receive, send, and
//! interrupt callbacks) and serves it through `netdriver_task`
//! (`virtio_net.c:438-443`). This crate owns the discipline half
//! (which queue does what, when to refill); the service binary owns
//! queue traffic. See document `22-net-driver-reference.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod queues;

/// Service initialization entry (wires the card table; queue traffic stays out).
pub fn init() {}

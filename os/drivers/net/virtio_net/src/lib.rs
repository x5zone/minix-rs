//! Virtio network card: queue roles plus refill discipline.
//!
//! C correspondence: `minix3/minix/drivers/net/virtio_net/virtio_net.c`
//! (446 lines). The driver registers `virtio_net_table`
//! (`virtio_net.c:92-99`, name plus init, stop, receive, send, and
//! interrupt callbacks) and serves it through `netdriver_task`
//! (`virtio_net.c:438-443`). This crate owns the discipline half
//! (which queue does what, when to refill); the service binary owns
//! queue traffic. See document `22-net-driver-reference.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod char_face;
pub mod queues;

use char_face::VirtioNetFace;
use minix_netdriver::pump::NetService;

/// Build the driver's service for the binary.
///
/// Queue traffic stays in the card seam this crate documents; the pump
/// and its queues live in the shared framework.
pub fn init() -> NetService<VirtioNetFace> {
    NetService::new(VirtioNetFace::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_netdriver::driver::NetDriver;

    #[test]
    fn test_init_builds_the_card_service() {
        let service = init();
        assert_eq!(service.face().name(), "virtio_net");
        assert!(service.server().is_running());
    }
}

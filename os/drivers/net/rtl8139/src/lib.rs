//! Realtek 8139 network card: slot picker plus ring wrap.
//!
//! C correspondence: `minix3/minix/drivers/net/rtl8139/rtl8139.c`
//! (1530 lines) with `rtl8139.h` (slot count, ring size, cursors).
//! The driver registers `rl_table` (`rtl8139.c:101-113`, eleven
//! callbacks) and serves it through `netdriver_task`
//! (`rtl8139.c:118-123`). This crate owns the cursor half (slot
//! rotation, ring wrap); the service binary owns register traffic.
//! See document `23-net-driver-variants.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod char_face;
pub mod txrx;

use char_face::Rtl8139Face;
use minix_netdriver::pump::NetService;

/// Build the driver's service for the binary.
///
/// Register traffic stays in the card seam this crate documents; the pump
/// and its queues live in the shared framework.
pub fn init() -> NetService<Rtl8139Face> {
    NetService::new(Rtl8139Face::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_netdriver::driver::NetDriver;

    #[test]
    fn test_init_builds_the_card_service() {
        let service = init();
        assert_eq!(service.face().name(), "rtl8139");
        assert!(service.server().is_running());
    }
}

//! National-semiconductor 8390 network card: receive ring cursor.
//!
//! C correspondence: `minix3/minix/drivers/net/dp8390/dp8390.c`
//! (998 lines) with board files `3c503.c`, `ne2000.c`, `rtl8029.c`,
//! `wdeth.c`. The driver registers `dp_table` (`dp8390.c:103-111`,
//! name plus init, stop, mode, receive, send, interrupt, and tick
//! callbacks) and serves it through `netdriver_task`
//! (`dp8390.c:117-121`). This crate owns the cursor half (page wrap,
//! length guard); the service binary owns register traffic. See
//! document `22-net-driver-reference.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod char_face;
pub mod ring;

use char_face::Dp8390Face;
use minix_netdriver::pump::NetService;

/// Build the driver's service for the binary.
///
/// Register traffic stays in the card seam this crate documents; the pump
/// and its queues live in the shared framework.
pub fn init() -> NetService<Dp8390Face> {
    NetService::new(Dp8390Face::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_netdriver::driver::NetDriver;

    #[test]
    fn test_init_builds_the_card_service() {
        let service = init();
        assert_eq!(service.face().name(), "dp8390");
        assert!(service.server().is_running());
    }
}

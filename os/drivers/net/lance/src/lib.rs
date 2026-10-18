//! AMD LANCE network card: small rings plus chip identity.
//!
//! C correspondence: `minix3/minix/drivers/net/lance/lance.c`
//! (895 lines). The driver registers `lance_table`
//! (`lance.c:158-167`, eight callbacks) and serves it through
//! `netdriver_task` (`lance.c:172-177`). This crate owns the table
//! half (ring sizes, chip matching); the service binary owns
//! register traffic. See document `23-net-driver-variants.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

pub mod char_face;
pub mod ring;

use char_face::LanceFace;
use minix_netdriver::pump::NetService;

/// Build the driver's service for the binary.
///
/// The chip identity comes from the version register, which is the
/// register-traffic seam this crate does not model yet; without a probe
/// result the face carries `None`, the same fallback the C probe reports
/// when its version gate fails (`lance_probe`, `lance.c:707-722`).
pub fn init() -> NetService<LanceFace> {
    NetService::new(LanceFace::new(None))
}

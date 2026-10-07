//! Memory driver: RAM disks, absolute and kernel memory, null and zero.
//!
//! C correspondence: `minix3/minix/drivers/storage/memory/memory.c` (599
//! lines). This crate owns the numbers (minor decoding, geometry table,
//! open counts, transfer plans, page-window policy) and the face routing
//! ([`service::MemoryService`]); the service *binary* owns the runtime shell
//! (announce, birth handshake, real transport, physical mapping). See
//! document `05-memory-driver.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! The driver serves both faces with one receive loop: block requests go
//! to the block framework, everything else to the character framework
//! (`memory.c:101-104`). That split-by-family gate is
//! [`service::MemoryService::dispatch`]; the face-membership predicate
//! ([`device::MemoryMinor::is_character_only`]) is the shared rule both
//! faces apply behind it.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

extern crate alloc;

pub mod device;
pub mod block_face;
pub mod char_face;
pub mod service;
pub mod transfer;

use block_face::{MemoryBlock, VecRamDisk};
use char_face::{MemoryChar, VecBackend};
use service::MemoryService;

/// Build the dual-face service for the driver binary.
///
/// The fresh tables carry the startup geometry the linked-in image would
/// otherwise supply: the absolute-memory extent spans four gigabytes (set in
/// [`device::DeviceTable::fresh`]), while the image disk and RAM disks start
/// at zero size and grow through the resize control (exactly as C defers
/// them to `sef_cb_init_fresh` and `m_block_ioctl`). The default vector
/// backings host the pump; the byte-level grant and physical-page copies ride
/// the transport in the binary (see the service module's data-plane seam).
pub fn init() -> MemoryService<VecBackend, VecRamDisk> {
    MemoryService::new(
        MemoryChar::new(VecBackend::default()),
        MemoryBlock::new(VecRamDisk::default()),
    )
}

//! Ensoniq 1371 sound card: sample-rate policy.
//!
//! C correspondence: `minix3/minix/drivers/audio/es1371/es1371.c`
//! (656 lines), `SRC.c` (196 lines), `codec.c` (264 lines),
//! `AC97.c` (496 lines), `sample_rate_converter.c` (240 lines).
//! This crate owns the rate policy half (bounds, channel routing);
//! the service binary owns codec writes and converter traffic. See
//! document `21-audio-drivers.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod char_face;
pub mod rate;

use alloc::vec;
use char_face::Es1371Face;
use minix_audiodriver::dma::UnboundDma;
use minix_audiodriver::service::AudioService;
use minix_audiodriver::special::{NO_CHANNEL, SpecialFile};
use minix_audiodriver::subdev::DmaMode;

/// The minor device the card's audio node answers on (C's `drv_init`
/// fills the special-file table; the modeled table serves one node whose
/// write side is the DAC pair and whose read side is the capture side).
pub const AUDIO_MINOR: i32 = 0;

/// The channel geometry the card reports at `drv_init`
/// (`es1371.c:69-72`, `:76-79`): a 64 KiB DMA buffer split into two
/// fragments of 32 KiB each, with four extra buffers. The fragment size
/// is the seeded one (`DmaSize / NrOfDmaFragments`, `es1371.c:165`),
/// which is what the framework's per-request size gate compares against.
pub const CHANNEL_GEOMETRY: [(DmaMode, u32, u32, u32); 2] = [
    (DmaMode::Write, 2, 32 * 1024, 4),
    (DmaMode::Read, 2, 32 * 1024, 4),
];

/// Builds the service for the binary: the card's hooks, the DMA allocator
/// seam (registered `E-DMABUF`), and the special-file table.
pub fn init() -> AudioService<Es1371Face, UnboundDma> {
    let files = vec![SpecialFile {
        minor: AUDIO_MINOR,
        read_chan: 1,
        write_chan: 0,
        ioctl_chan: 0,
    }];
    let _ = NO_CHANNEL;
    AudioService::new(Es1371Face::new(), UnboundDma, files, &CHANNEL_GEOMETRY)
}

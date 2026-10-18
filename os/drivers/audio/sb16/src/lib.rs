//! SoundBlaster 16 sound card: processor command policy.
//!
//! C correspondence: `minix3/minix/drivers/audio/sb16/sb16.c`
//! (449 lines) with `sb16.h` (command bytes, ports, limits) and
//! `mixer.c` (254 lines). This crate owns the command half (which
//! bytes mean what, how a rate splits); the service binary owns port
//! traffic. See document `21-audio-drivers.md` in
//! `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod char_face;
pub mod dsp;

use alloc::vec;
use char_face::Sb16Face;
use minix_audiodriver::dma::UnboundDma;
use minix_audiodriver::service::AudioService;
use minix_audiodriver::special::{NO_CHANNEL, SpecialFile};
use minix_audiodriver::subdev::DmaMode;

/// The three special files C's `drv_init` fills (`sb16.c:56-68`): the
/// write node, the read node, and the mixer's control-only node.
pub fn special_files() -> alloc::vec::Vec<SpecialFile> {
    vec![
        SpecialFile {
            minor: 0,
            read_chan: NO_CHANNEL,
            write_chan: 0,
            ioctl_chan: 0,
        },
        SpecialFile {
            minor: 1,
            read_chan: 0,
            write_chan: NO_CHANNEL,
            ioctl_chan: 0,
        },
        SpecialFile {
            minor: 2,
            read_chan: NO_CHANNEL,
            write_chan: NO_CHANNEL,
            ioctl_chan: 1,
        },
    ]
}

/// The channel geometry C's `drv_init` sets (`sb16.c:46-54`): the audio
/// channel runs two 32 KiB fragments with four extra buffers; the mixer
/// channel is control-only (no fragment geometry, C's `NO_DMA`).
pub const CHANNEL_GEOMETRY: [(DmaMode, u32, u32, u32); 2] = [
    // DmaSize 64 KiB over two fragments, both directions.
    (DmaMode::Write, 2, 32 * 1024, 4),
    // The mixer channel: no DMA, only controls.
    (DmaMode::Write, 0, 0, 0),
];

/// Builds the service for the binary.
pub fn init() -> AudioService<Sb16Face, UnboundDma> {
    AudioService::new(
        Sb16Face::new(),
        UnboundDma,
        special_files(),
        &CHANNEL_GEOMETRY,
    )
}

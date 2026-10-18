//! Trident sound card: the register map, the AC97 codec, and the card
//! face.
//!
//! C correspondence: `minix3/minix/drivers/audio/trident/trident.c` (603
//! lines) with `trident.h` (the register map and format bits), `io.h`
//! (the port macros), `mixer.c`/`mixer.h` (the `MIXER_AC97` half), and
//! the audio framework it plugs into
//! (`minix3/minix/lib/libaudiodriver/audio_fw.c`). This crate owns the
//! card's register traffic (`ports.rs`), the register map
//! (`registers.rs`), the codec (`mixer.rs`), and the `drv_*` face
//! (`device.rs`); the service binary owns the loop and the PCI probe.
//! See document `21-audio-drivers.md` in `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod device;
pub mod mixer;
pub mod ports;
pub mod probe;
pub mod registers;

use alloc::vec;
use device::Trident;
use minix_audiodriver::dma::UnboundDma;
use minix_audiodriver::service::AudioService;
use minix_audiodriver::special::{NO_CHANNEL, SpecialFile};
use minix_audiodriver::subdev::DmaMode;
use ports::KernelPorts;

/// The playback node (`special_file[0]`, `trident.c:387-390`).
pub const AUDIO_MINOR: i32 = 0;
/// The capture node (`special_file[1]`, `trident.c:392-395`).
pub const ADC_MINOR: i32 = 1;
/// The mixer node (`special_file[2]`, `trident.c:397-401`).
pub const MIXER_MINOR: i32 = 2;

/// The three special files C's `drv_init` fills (`trident.c:387-401`):
/// the write node, the read node, and the control-only mixer node.
pub fn special_files() -> alloc::vec::Vec<SpecialFile> {
    vec![
        SpecialFile {
            minor: AUDIO_MINOR,
            read_chan: NO_CHANNEL,
            write_chan: 0,
            ioctl_chan: 0,
        },
        SpecialFile {
            minor: ADC_MINOR,
            read_chan: 1,
            write_chan: NO_CHANNEL,
            ioctl_chan: 1,
        },
        SpecialFile {
            minor: MIXER_MINOR,
            read_chan: NO_CHANNEL,
            write_chan: NO_CHANNEL,
            ioctl_chan: 2,
        },
    ]
}

/// The channel geometry C's `drv_init` sets (`trident.c:373-385`): the
/// playback and capture channels each run two 32 KiB fragments of a
/// 64 KiB buffer with four extra buffers; the mixer channel is
/// control-only (C's `NO_DMA`).
pub const CHANNEL_GEOMETRY: [(DmaMode, u32, u32, u32); 3] = [
    // DmaSize 64 KiB over two fragments, playback.
    (DmaMode::Write, 2, 32 * 1024, 4),
    // The same geometry on the capture side.
    (DmaMode::Read, 2, 32 * 1024, 4),
    // The mixer channel: no DMA, only controls.
    (DmaMode::Write, 0, 0, 0),
];

/// Builds the service for the binary: the card's face over the real port
/// seam, the DMA allocator seam (registered `E-DMABUF`), and the
/// special-file table.
pub fn init(base_port: u16) -> AudioService<Trident<KernelPorts>, UnboundDma> {
    AudioService::new(
        Trident::new(KernelPorts { base: base_port }),
        UnboundDma,
        special_files(),
        &CHANNEL_GEOMETRY,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_special_file_table_matches_c() {
        let files = special_files();
        assert_eq!(files.len(), 3);
        // Minor 0 writes the DAC and controls it.
        assert_eq!(files[0].write_chan, 0);
        assert_eq!(files[0].read_chan, NO_CHANNEL);
        assert_eq!(files[0].ioctl_chan, 0);
        // Minor 1 reads the ADC and controls it.
        assert_eq!(files[1].read_chan, 1);
        assert_eq!(files[1].write_chan, NO_CHANNEL);
        assert_eq!(files[1].ioctl_chan, 1);
        // Minor 2 is the mixer: neither direction, only controls.
        assert_eq!(files[2].read_chan, NO_CHANNEL);
        assert_eq!(files[2].write_chan, NO_CHANNEL);
        assert_eq!(files[2].ioctl_chan, 2);
    }

    #[test]
    fn test_the_channel_geometry_matches_c() {
        assert_eq!(CHANNEL_GEOMETRY[0], (DmaMode::Write, 2, 32 * 1024, 4));
        assert_eq!(CHANNEL_GEOMETRY[1], (DmaMode::Read, 2, 32 * 1024, 4));
        assert_eq!(CHANNEL_GEOMETRY[2], (DmaMode::Write, 0, 0, 0));
    }

    #[test]
    fn test_the_service_initializes_the_card() {
        let mut service = init(0x0200);
        // `drv_init` reports three sub-devices, then `drv_init_hw` runs;
        // in a host build the port calls cannot complete, so the card
        // reports the failure instead of pretending to be ready.
        assert_eq!(service.init(), -minix_types::EIO);
    }
}

//! CMI8738 sound card: the register map, the mixer, and the card face.
//!
//! C correspondence: `minix3/minix/drivers/audio/cmi8738/cmi8738.c`
//! (598 lines) with `cmi8738.h` (the register map and command masks),
//! `mixer.c`/`mixer.h` (the `MIXER_SB16` half), and the audio framework
//! it plugs into (`minix3/minix/lib/libaudiodriver/audio_fw.c`). This
//! crate owns the card's register traffic (`ports.rs`), the register map
//! (`registers.rs`), the mixer (`mixer.rs`), and the `drv_*` face
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
use device::Cmi8738;
use minix_audiodriver::dma::DmaAlloc;
use minix_audiodriver::service::AudioService;
use minix_audiodriver::special::{NO_CHANNEL, SpecialFile};
use minix_audiodriver::subdev::DmaMode;
use ports::KernelPorts;

/// The DAC playback node (`special_file[0]`, `cmi8738.c:364-367`).
pub const AUDIO_MINOR: i32 = 0;
/// The ADC capture node (`special_file[1]`, `cmi8738.c:369-372`).
pub const ADC_MINOR: i32 = 1;
/// The mixer node (`special_file[2]`, `cmi8738.c:374-378`).
pub const MIXER_MINOR: i32 = 2;

/// The three special files C's `drv_init` fills (`cmi8738.c:364-378`):
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

/// The channel geometry C's `drv_init` sets (`cmi8738.c:351-361`): the
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
pub fn init<A: DmaAlloc>(base_port: u16, dma: A) -> AudioService<Cmi8738<KernelPorts>, A> {
    AudioService::new(
        Cmi8738::new(KernelPorts { base: base_port }),
        dma,
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
        let mut service = init(0x0200, minix_audiodriver::dma::UnboundDma);
        // `drv_init` reports three sub-devices, then `drv_init_hw` runs;
        // in a host build the port calls cannot complete, so the card
        // reports the failure instead of pretending to be ready.
        assert_eq!(service.init(), -minix_types::EIO);
    }
}

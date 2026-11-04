//! CS4281 sound card: the register map, the AC97 codec, and the card face.
//!
//! C correspondence: `minix3/minix/drivers/audio/cs4281/cs4281.c` (670
//! lines) with `cs4281.h` (the register map and command bits), `io.h`
//! (the **memory-mapped** register macros), `mixer.c`/`mixer.h` (the
//! `MIXER_AC97` half), and the audio framework it plugs into
//! (`minix3/minix/lib/libaudiodriver/audio_fw.c`). This crate owns the
//! register window seam (`window.rs`), the register map (`registers.rs`),
//! the codec (`mixer.rs`), and the `drv_*` face (`device.rs`); the
//! service binary owns the loop, the PCI probe, and the BAR mapping.
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
pub mod probe;
pub mod registers;
pub mod window;

use alloc::vec;
use device::Cs4281;
use minix_audiodriver::dma::DmaAlloc;
use minix_audiodriver::service::AudioService;
use minix_audiodriver::special::{NO_CHANNEL, SpecialFile};
use minix_audiodriver::subdev::DmaMode;
use window::MappedWindow;

/// The playback node (`special_file[0]`, `cs4281.c:443-446`).
pub const AUDIO_MINOR: i32 = 0;
/// The capture node (`special_file[1]`, `cs4281.c:448-451`).
pub const ADC_MINOR: i32 = 1;
/// The mixer node (`special_file[2]`, `cs4281.c:453-457`).
pub const MIXER_MINOR: i32 = 2;

/// The three special files C's `drv_init` fills (`cs4281.c:443-457`):
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

/// The channel geometry C's `drv_init` sets (`cs4281.c:429-441`): the
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

/// Builds the service for the binary: the card's face over the mapped
/// register window, the DMA allocator seam (registered `E-DMABUF`), and
/// the special-file table.
///
/// # Safety
///
/// `base` must be the register window's mapped address
/// ([`probe::map_bar`]).
pub unsafe fn init<A: DmaAlloc>(base: *mut u8, dma: A) -> AudioService<Cs4281<MappedWindow>, A> {
    AudioService::new(
        // SAFETY: the caller's contract is this function's contract.
        Cs4281::new(unsafe { MappedWindow::new(base) }),
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
        // The window is exercised over a real allocation, the same
        // volatile path a mapped BAR takes; the card's initialization
        // then runs against those bytes.
        let mut backing = alloc::vec![0u8; 0x1000];
        // The codec's answer bytes: the status register reports a valid
        // transfer and the serial data register reports ready, so the
        // initialization's polls end at once.
        backing[0x0464..0x0468].copy_from_slice(&registers::STS_CODEC_VALID.to_le_bytes());
        backing[0x047c..0x0480].copy_from_slice(&0x03u32.to_le_bytes());
        // SAFETY: `backing` outlives the service.
        let mut service = unsafe { init(backing.as_mut_ptr(), minix_audiodriver::dma::UnboundDma) };
        assert_eq!(service.init(), 0, "the codec answers immediately");
        // The reset's configuration write landed in the window.
        assert_eq!(
            u32::from_le_bytes(backing[0x03e0..0x03e4].try_into().unwrap()),
            0x4281
        );
    }
}

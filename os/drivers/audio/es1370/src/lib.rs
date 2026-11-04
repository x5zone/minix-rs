//! ES1370 (AudioPCI '97) sound card: the register map, the codec, and the
//! card face.
//!
//! C correspondence: `minix3/minix/drivers/audio/es1370/es1370.c` (654
//! lines) with `es1370.h` (the register map), `ak4531.c`/`ak4531.h` (the
//! codec), and `pci_helper.c` (the port macros); the framework it plugs
//! into is `minix3/minix/lib/libaudiodriver/audio_fw.c`. This crate owns
//! the card's register traffic (`ports.rs`), the register map
//! (`registers.rs`), the codec (`codec.rs`), and the `drv_*` face
//! (`device.rs`); the service binary owns the loop and the PCI probe.
//! See document `21-audio-drivers.md` in `rewrite-notes/16-stage-drivers/`.
//!
//! Single-threaded event loop: one message at a time, no shared mutable
//! state across threads.

#![no_std]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod codec;
pub mod device;
pub mod ports;
pub mod probe;
pub mod registers;

use alloc::vec;
use device::Es1370;
use minix_audiodriver::dma::DmaAlloc;
use minix_audiodriver::service::AudioService;
use minix_audiodriver::special::{NO_CHANNEL, SpecialFile};
use minix_audiodriver::subdev::DmaMode;
use ports::KernelPorts;

/// The first playback node (`special_file[0]`, `es1370.c:82-85`).
pub const AUDIO_MINOR: i32 = 0;
/// The capture node (`special_file[1]`, `es1370.c:87-90`).
pub const ADC_MINOR: i32 = 1;
/// The mixer node (`special_file[2]`, `es1370.c:92-95`).
pub const MIXER_MINOR: i32 = 2;
/// The second playback node (`special_file[3]`, `es1370.c:97-100`).
pub const AUDIO2_MINOR: i32 = 3;

/// The four special files C's `drv_init` fills (`es1370.c:82-100`): the
/// two playback nodes, the capture node, and the control-only mixer node.
pub fn special_files() -> alloc::vec::Vec<SpecialFile> {
    vec![
        SpecialFile {
            minor: AUDIO_MINOR,
            read_chan: NO_CHANNEL,
            write_chan: registers::DAC1_CHAN,
            ioctl_chan: registers::DAC1_CHAN,
        },
        SpecialFile {
            minor: ADC_MINOR,
            read_chan: registers::ADC1_CHAN,
            write_chan: NO_CHANNEL,
            ioctl_chan: registers::ADC1_CHAN,
        },
        SpecialFile {
            minor: MIXER_MINOR,
            read_chan: NO_CHANNEL,
            write_chan: NO_CHANNEL,
            ioctl_chan: registers::MIXER,
        },
        SpecialFile {
            minor: AUDIO2_MINOR,
            read_chan: NO_CHANNEL,
            write_chan: registers::DAC2_CHAN,
            ioctl_chan: registers::DAC2_CHAN,
        },
    ]
}

/// The channel geometry C's `drv_init` sets (`es1370.c:60-97`): the two
/// playback channels and the capture channel each run two 32 KiB
/// fragments of a 64 KiB buffer with four extra buffers; the mixer
/// channel is control-only (C's `NO_DMA`).
pub const CHANNEL_GEOMETRY: [(DmaMode, u32, u32, u32); 4] = [
    // DAC1: DmaSize 64 KiB over two fragments.
    (DmaMode::Write, 2, 32 * 1024, 4),
    // ADC1: the same geometry on the capture side.
    (DmaMode::Read, 2, 32 * 1024, 4),
    // The mixer channel: no DMA, only controls.
    (DmaMode::Write, 0, 0, 0),
    // DAC2: the second playback channel.
    (DmaMode::Write, 2, 32 * 1024, 4),
];

/// Builds the service for the binary: the card's face over the real port
/// seam, the DMA allocator seam (registered `E-DMABUF`), and the
/// special-file table.
pub fn init<A: DmaAlloc>(base_port: u16, dma: A) -> AudioService<Es1370<KernelPorts>, A> {
    AudioService::new(
        Es1370::new(KernelPorts { base: base_port }),
        dma,
        special_files(),
        &CHANNEL_GEOMETRY,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use registers::{ADC1_CHAN, DAC1_CHAN, DAC2_CHAN, MIXER};

    #[test]
    fn test_the_special_file_table_matches_c() {
        let files = special_files();
        assert_eq!(files.len(), 4);
        // Minor 0 writes DAC1 and controls it.
        assert_eq!(files[0].write_chan, DAC1_CHAN);
        assert_eq!(files[0].read_chan, NO_CHANNEL);
        assert_eq!(files[0].ioctl_chan, DAC1_CHAN);
        // Minor 1 reads ADC1 and controls it.
        assert_eq!(files[1].read_chan, ADC1_CHAN);
        assert_eq!(files[1].write_chan, NO_CHANNEL);
        assert_eq!(files[1].ioctl_chan, ADC1_CHAN);
        // Minor 2 is the mixer: neither direction, only controls.
        assert_eq!(files[2].read_chan, NO_CHANNEL);
        assert_eq!(files[2].write_chan, NO_CHANNEL);
        assert_eq!(files[2].ioctl_chan, MIXER);
        // Minor 3 writes DAC2 and controls it.
        assert_eq!(files[3].write_chan, DAC2_CHAN);
        assert_eq!(files[3].read_chan, NO_CHANNEL);
        assert_eq!(files[3].ioctl_chan, DAC2_CHAN);
    }

    #[test]
    fn test_the_channel_geometry_matches_c() {
        assert_eq!(CHANNEL_GEOMETRY[0], (DmaMode::Write, 2, 32 * 1024, 4));
        assert_eq!(CHANNEL_GEOMETRY[1], (DmaMode::Read, 2, 32 * 1024, 4));
        assert_eq!(CHANNEL_GEOMETRY[2], (DmaMode::Write, 0, 0, 0));
        assert_eq!(CHANNEL_GEOMETRY[3], (DmaMode::Write, 2, 32 * 1024, 4));
    }

    #[test]
    fn test_the_service_initializes_the_card() {
        let mut service = init(0x0300, minix_audiodriver::dma::UnboundDma);
        // `drv_init` reports four sub-devices, then `drv_init_hw` runs;
        // in a host build the port calls cannot complete, so the card
        // reports the failure instead of pretending to be ready.
        assert_eq!(service.init(), -minix_types::EIO);
    }
}

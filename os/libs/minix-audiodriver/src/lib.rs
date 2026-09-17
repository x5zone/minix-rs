//! Audio driver framework: the fourteen hooks and the DMA fragment ring.
//!
//! C correspondence: `minix3/minix/lib/libaudiodriver/audio_fw.c` (868
//! lines) plus its hook declarations in `minix3/minix/include/minix/
//! audio_fw.h` (the fourteen `drv_*` prototypes), and the per-sub-device
//! runtime record `sub_dev_t` (`audio_fw.h:18-55`).
//!
//! The C library hard-wires the DMA fragment ring (a playback interrupt
//! advances the read cursor and drains the extra buffer; a capture
//! interrupt advances the fill cursor), the 64 KiB DMA boundary rule,
//! and the out-of-data pause path. This crate renders exactly those
//! rules as types: [`subdev::SubDevice`] is the ring, [`hooks::
//! AudioHooks`] is the card-specific half a sound-card driver fills in.
//! PCI negotiation, port traffic, and the interrupt registration stay
//! in the service binary.
//!
//! Single-threaded event loop: one message at a time.

#![no_std]

extern crate alloc;

pub mod hooks;
pub mod special;
pub mod subdev;

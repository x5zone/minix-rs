//! Virtio framework: rings, features, and device life cycle.
//!
//! C correspondence: `minix3/minix/lib/libvirtio/virtio.c` (913 lines)
//! with `virtio_ring.h` (ring layout). This crate owns the index
//! arithmetic, the negotiation, and the setup order; the service crates
//! own ports, memory maps, and interrupts. See document
//! `14-virtio-framework.md` in
//! `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/`.
//!
//! No threads here: the C indirect-descriptor thread pools are a
//! performance optimization for multi-threaded drivers, and this
//! rendering serves single-threaded event loops (the direct-descriptor
//! path, which the C code also uses first).

#![no_std]

extern crate alloc;

pub mod device;
pub mod features;
pub mod ring;

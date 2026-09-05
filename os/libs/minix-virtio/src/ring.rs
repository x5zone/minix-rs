//! Virtqueue rings: descriptors, available ring, used ring.
//!
//! C correspondence: the ring layout in
//! `minix3/minix/lib/libvirtio/virtio_ring.h` (descriptor, available,
//! used structures plus the next, write, indirect, no-notify, and
//! no-interrupt flags) and the free-list plus avail/used index handling
//! in `minix3/minix/lib/libvirtio/virtio.c` (queue state fields).
//!
//! Guest-physical addresses stay opaque integers here; mapping them stays
//! in the service crate. This module owns the index arithmetic: which
//! descriptor is free, which available entry the host has not seen, which
//! used entry the guest has not collected.

/// Descriptor continues through the next field.
///
/// C: `VRING_DESC_F_NEXT 1` (`virtio_ring.h`).
pub const DESC_NEXT: u16 = 1;
/// Descriptor buffer is write-only for the host.
///
/// C: `VRING_DESC_F_WRITE 2` (`virtio_ring.h`).
pub const DESC_WRITE: u16 = 2;
/// Descriptor buffer holds a list of descriptors (indirect).
///
/// C: `VRING_DESC_F_INDIRECT 4` (`virtio_ring.h`).
pub const DESC_INDIRECT: u16 = 4;

/// Host advises the guest not to kick on new buffers (optimization only).
///
/// C: `VRING_USED_F_NO_NOTIFY 1` (`virtio_ring.h`).
pub const USED_NO_NOTIFY: u16 = 1;
/// Guest advises the host not to interrupt on consumed buffers.
///
/// C: `VRING_AVAIL_F_NO_INTERRUPT 1` (`virtio_ring.h`).
pub const AVAIL_NO_INTERRUPT: u16 = 1;

/// Indirect-descriptor negotiation bit.
///
/// C: `VIRTIO_RING_F_INDIRECT_DESC 28` (`virtio_ring.h`).
pub const FEATURE_INDIRECT: u8 = 28;
/// Event-index negotiation bit.
///
/// C: `VIRTIO_RING_F_EVENT_IDX 29` (`virtio_ring.h`).
pub const FEATURE_EVENT_INDEX: u8 = 29;

/// One descriptor: address, length, flags, next.
///
/// C: `struct vring_desc` (`virtio_ring.h`): sixteen bytes on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    /// Guest-physical address (opaque to this crate).
    pub address: u64,
    /// Buffer length in bytes.
    pub length: u32,
    /// Flag bits (next, write, indirect).
    pub flags: u16,
    /// Next descriptor in the chain (or free-list link).
    pub next: u16,
}

impl Descriptor {
    /// True when the chain continues.
    pub const fn has_next(self) -> bool {
        self.flags & DESC_NEXT != 0
    }

    /// True when the host may write (device-to-guest buffer).
    pub const fn is_write(self) -> bool {
        self.flags & DESC_WRITE != 0
    }
}

/// One used-ring entry: which chain completed, how much was written.
///
/// C: `struct vring_used_elem` (`virtio_ring.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsedElement {
    /// Head index of the completed chain.
    pub id: u32,
    /// Bytes the host wrote.
    pub length: u32,
}

/// Virtqueue index state: free list plus available/used cursors.
///
/// C: the `free_num`, `free_head`, `last_used` fields of `struct
/// virtio_queue` (`virtio.c:60-68`) with the free chain threaded through
/// `next` (`virtio.c`, queue init). Chains and buffers stay in the
/// service crate; this type answers "which descriptor" and "which entry".
#[derive(Debug, Clone)]
pub struct QueueState {
    size: u16,
    free_head: u16,
    free_count: u16,
    next: alloc::vec::Vec<u16>,
    avail_index: u16,
    used_index: u16,
    collected: u16,
}

impl QueueState {
    /// Fresh queue of this many descriptors (power of two, like hardware).
    pub fn new(size: u16) -> QueueState {
        let size = size.max(1) as usize;
        let mut next = alloc::vec::Vec::with_capacity(size);
        for i in 0..size {
            next.push(((i + 1) % size) as u16);
        }
        QueueState {
            size: size as u16,
            free_head: 0,
            free_count: size as u16,
            next,
            avail_index: 0,
            used_index: 0,
            collected: 0,
        }
    }

    /// Queue size in descriptors.
    pub const fn size(&self) -> u16 {
        self.size
    }

    /// Free descriptors remaining.
    pub fn free(&self) -> u16 {
        self.free_count
    }

    /// Take one free descriptor head for a new chain; `None` when empty.
    ///
    /// C: taking from the free list before chaining (`virtio_to_queue`
    /// path, `virtio.c`).
    pub fn take_free(&mut self) -> Option<u16> {
        if self.free_count == 0 {
            return None;
        }
        let head = self.free_head;
        self.free_head = self.next[head as usize];
        self.free_count -= 1;
        Some(head)
    }

    /// Return one descriptor to the free list.
    pub fn give_back(&mut self, index: u16) {
        if (index as usize) >= self.next.len() {
            return;
        }
        self.next[index as usize] = self.free_head;
        self.free_head = index;
        self.free_count += 1;
    }

    /// Publish one available entry (guest to host).
    pub fn publish(&mut self) {
        self.avail_index = self.avail_index.wrapping_add(1);
    }

    /// Available entries the host has not yet consumed.
    pub fn pending(&self) -> u16 {
        self.avail_index.wrapping_sub(self.used_index)
    }

    /// Note the host consumed up to this used index.
    pub fn note_used(&mut self, used: u16) {
        self.used_index = used;
    }

    /// Collect one completed entry; `None` when all are collected.
    pub fn collect(&mut self) -> Option<u16> {
        if self.collected == self.used_index {
            return None;
        }
        let id = self.collected;
        self.collected = self.collected.wrapping_add(1);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_flag_helpers() {
        let desc = Descriptor {
            address: 0x1000,
            length: 512,
            flags: DESC_NEXT | DESC_WRITE,
            next: 3,
        };
        assert!(desc.has_next());
        assert!(desc.is_write());
        let plain = Descriptor { flags: 0, ..desc };
        assert!(!plain.has_next());
        assert_eq!(DESC_INDIRECT, 4);
    }

    #[test]
    fn test_free_list_cycles_without_loss() {
        let mut queue = QueueState::new(8);
        assert_eq!(queue.free(), 8);
        let mut taken = alloc::vec::Vec::new();
        for _ in 0..8 {
            taken.push(queue.take_free().unwrap());
        }
        assert_eq!(queue.take_free(), None);
        for index in taken {
            queue.give_back(index);
        }
        assert_eq!(queue.free(), 8);
    }

    #[test]
    fn test_publish_collect_tracks_host_progress() {
        let mut queue = QueueState::new(8);
        queue.publish();
        queue.publish();
        assert_eq!(queue.pending(), 2);
        queue.note_used(1);
        assert_eq!(queue.pending(), 1);
        assert_eq!(queue.collect(), Some(0));
        assert_eq!(queue.collect(), None);
    }

    #[test]
    fn test_constants_match_ring_header() {
        assert_eq!(DESC_NEXT, 1);
        assert_eq!(DESC_WRITE, 2);
        assert_eq!(USED_NO_NOTIFY, 1);
        assert_eq!(AVAIL_NO_INTERRUPT, 1);
        assert_eq!(FEATURE_INDIRECT, 28);
        assert_eq!(FEATURE_EVENT_INDEX, 29);
    }
}

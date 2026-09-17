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
/// C: `struct vring_desc` (`virtio_ring.h:63-75`): sixteen bytes on the
/// wire, exactly this field order. The `#[repr(C)]` makes the type safe
/// to overlay on (or write into) the shared queue memory the service
/// maps; the const size assert pins the contract at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
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

const _: () = assert!(core::mem::size_of::<Descriptor>() == 16);

/// Available-ring header: flags plus free-running index
/// (`struct vring_avail`, `virtio_ring.h:77-80`), followed by `num`
/// u16 ring entries (and, with EVENT_IDX, one more u16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct AvailHeader {
    /// Flag bits (no-interrupt).
    pub flags: u16,
    /// Free-running index of the next available entry.
    pub idx: u16,
}

/// Used-ring header: flags plus free-running index, followed by `num`
/// used elements (`struct vring_used`, `virtio_ring.h:89-94`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct UsedHeader {
    /// Flag bits (no-notify).
    pub flags: u16,
    /// Free-running index of the next used entry.
    pub idx: u16,
}

/// Byte size of one queue's shared memory, per the C layout formula
/// (`vring_size`, `virtio_ring.h:136-145`): descriptor table, then the
/// available ring padded up to `align`, then the used ring.
pub const fn vring_size(num: u16, align: u32) -> u32 {
    let num = num as u32;
    let desc = 16 * num;
    let avail = 2 * (3 + num); // flags + idx + num entries + event idx
    let avail_total = (desc + avail + align - 1) & !(align - 1);
    let used = 6 + 8 * num; // flags + idx + num elements + event idx
    avail_total + used
}

/// Byte offset of the available ring from the queue base
/// (`vring_init`: right after the descriptor table).
pub const fn avail_offset(num: u16) -> u32 {
    16 * num as u32
}

/// Byte offset of the used ring from the queue base (`vring_init`:
/// after the available ring, padded up to `align`).
pub const fn used_offset(num: u16, align: u32) -> u32 {
    let avail_end = avail_offset(num) + 2 * (3 + num as u32);
    (avail_end + align - 1) & !(align - 1)
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

/// Wire chain terminator stored in a chain's last `next` field.
pub const CHAIN_END: u16 = u16::MAX;

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

    /// Used entries the guest has not collected yet.
    pub const fn uncollected(&self) -> u16 {
        self.used_index.wrapping_sub(self.collected)
    }

    /// Mark one used entry handled (advance the collection cursor).
    pub fn note_collected(&mut self) {
        self.collected = self.collected.wrapping_add(1);
    }

    /// Reserve a chain of `count` descriptors linked through `next`;
    /// returns the head index, or `None` when fewer are free.
    ///
    /// C: the chaining half of `virtio_to_queue` — descriptors come off
    /// the free list in order, each pointing at the following one, the
    /// last carrying [`CHAIN_END`] (the wire chain terminator). Arguments
    /// per descriptor are the service's business.
    pub fn take_chain(&mut self, count: usize) -> Option<u16> {
        if count == 0 || count as u16 > self.free_count {
            return None;
        }
        let head = self.free_head;
        // Walk count-1 links: intermediate descriptors are already
        // threaded in free-list order, which IS the chain order.
        let mut tail = head;
        for _ in 1..count {
            tail = self.next[tail as usize];
        }
        // The chain ends here; the free list continues after it.
        self.free_head = self.next[tail as usize];
        self.next[tail as usize] = CHAIN_END;
        self.free_count -= count as u16;
        Some(head)
    }

    /// Return one completed chain to the free list; returns the number of
    /// descriptors freed.
    ///
    /// C: the completion half of `virtio_from_queue` — walking the chain
    /// from the used element's `id` and re-threading the free list
    /// (`virtio.c:627` area). The `id` comes from the used-ring entry the
    /// service reads; a corrupt chain (walking off the table) frees
    /// nothing, matching C's fail-stop posture for impossible states.
    pub fn collect_chain(&mut self, head: u16) -> usize {
        if head as usize >= self.next.len() {
            return 0;
        }
        // Pass one: find the chain tail and its length. A step bound of
        // the table size catches corrupt chains (including a re-collected
        // head that is already on the free list, which is circular) —
        // fail closed, freeing nothing.
        let mut tail = head;
        let mut count = 1usize;
        let mut steps = 0usize;
        while self.next[tail as usize] != CHAIN_END {
            tail = self.next[tail as usize];
            if tail as usize >= self.next.len() {
                return 0;
            }
            steps += 1;
            if steps >= self.next.len() {
                return 0;
            }
            count += 1;
        }
        // Pass two: splice the whole chain in front of the free list.
        self.next[tail as usize] = self.free_head;
        self.free_head = head;
        self.free_count += count as u16;
        count
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
        assert_eq!(queue.uncollected(), 1);
        queue.note_collected();
        assert_eq!(queue.uncollected(), 0);
    }

    #[test]
    fn test_chain_take_and_collect_round_trip() {
        // The completion path must recover EVERY descriptor of a chain,
        // walking from the used-ring head id (virtio.c:627 area) — the
        // old cursor-returning collect leaked the whole chain.
        let mut queue = QueueState::new(8);
        let head_a = queue.take_chain(3).unwrap();
        let head_b = queue.take_chain(2).unwrap();
        assert_eq!(queue.free(), 3);
        // Host completed chain b first (out-of-order completion).
        assert_eq!(queue.collect_chain(head_b), 2);
        assert_eq!(queue.free(), 5);
        assert_eq!(queue.collect_chain(head_a), 3);
        assert_eq!(queue.free(), 8);
    }

    #[test]
    fn test_collect_chain_of_single_descriptor() {
        let mut queue = QueueState::new(4);
        let head = queue.take_chain(1).unwrap();
        assert_eq!(queue.collect_chain(head), 1);
        assert_eq!(queue.free(), 4);
        // A re-collected (stale) head re-threads without corruption: the
        // free list stays exactly four long.
        queue.collect_chain(head);
        assert_eq!(queue.free(), 4);
    }

    #[test]
    fn test_vring_layout_matches_c_formula() {
        // vring_size formula (virtio_ring.h:136-145): descriptor table,
        // avail ring padded to align, then the used ring.
        assert_eq!(avail_offset(8), 128); // 16 * 8
        assert_eq!(avail_offset(16), 256);
        // avail region: flags+idx+8 entries+event = 2*(3+8) = 22 bytes;
        // used starts at align_up(128+22, 4096) = 4096.
        assert_eq!(used_offset(8, 4096), 4096);
        assert_eq!(vring_size(8, 4096), 4096 + 6 + 8 * 8);
        // Small align: avail region ends at 150; align_up(150, 16) = 160.
        assert_eq!(used_offset(8, 16), 160);
        assert_eq!(vring_size(8, 16), 160 + 6 + 64);
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

//! Slab allocator for VM server.
//!
//! Memory allocator for fixed-size objects.
//! Corresponds to Minix3: `minix3/minix/servers/vm/slaballoc.c`

pub(crate) mod cache;
pub(crate) mod stats;
pub(crate) mod mock;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use cache::SlabCache;
pub(crate) use stats::{SlabStats, LeakReport};
pub(crate) use mock::MockPageAllocator;

pub(crate) const PAGE_SIZE: usize = 4096;
pub(crate) const MIN_OBJECT_SIZE: usize = 8;
pub(crate) const MAX_OBJECT_SIZE: usize = PAGE_SIZE / 2;

pub(crate) fn size_to_index(size: usize) -> usize {
    if size <= MIN_OBJECT_SIZE {
        0
    } else {
        let aligned = size.next_power_of_two();
        aligned.trailing_zeros() as usize - MIN_OBJECT_SIZE.trailing_zeros() as usize
    }
}

pub(crate) fn index_to_size(index: usize) -> usize {
    MIN_OBJECT_SIZE << index
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn test_size_conversions() {
        assert_eq!(size_to_index(8), 0);
        assert_eq!(size_to_index(16), 1);
        assert_eq!(size_to_index(32), 2);
        assert_eq!(size_to_index(64), 3);

        assert_eq!(index_to_size(0), 8);
        assert_eq!(index_to_size(1), 16);
        assert_eq!(index_to_size(2), 32);
        assert_eq!(index_to_size(3), 64);
    }
}

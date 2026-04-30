//! Reserved page queue for critical memory allocations.
//!
//! Memory pool reserved for critical paths, ensuring small allocations succeed even under memory pressure.
//! Used for fork, exec, and other operations that cannot fail.

use alloc::vec::Vec;
use super::{AllocFlags, PhysAddr};

const RESERVED_MAGIC: u32 = 0x6e4c74d5;
const MAX_RESERVED_PAGES: usize = 300;
const MAX_RESERVED_QUEUES: usize = 15;

#[derive(Debug, Clone, Copy)]
struct ReservedSlot {
    phys: PhysAddr,
    vir: Option<usize>,
}

impl ReservedSlot {
    const fn empty() -> Self {
        Self { phys: PhysAddr(0), vir: None }
    }
}

#[derive(Debug)]
pub(crate) struct ReservedQueue {
    next: Option<usize>,
    max_available: usize,
    npages: usize,
    mapped: bool,
    n_available: usize,
    alloc_flags: AllocFlags,
    slots: [ReservedSlot; MAX_RESERVED_PAGES],
    magic: u32,
}

impl ReservedQueue {
    const fn new() -> Self {
        Self {
            next: None,
            max_available: 0,
            npages: 0,
            mapped: false,
            n_available: 0,
            alloc_flags: AllocFlags::empty(),
            slots: [ReservedSlot::empty(); MAX_RESERVED_PAGES],
            magic: 0,
        }
    }

    fn sanity_check(&self) {
        assert_eq!(self.magic, RESERVED_MAGIC, "ReservedQueue magic mismatch");
        assert!(self.n_available <= MAX_RESERVED_PAGES);
        assert!(self.n_available <= self.max_available);
    }

    fn is_valid(&self) -> bool {
        self.magic == RESERVED_MAGIC && self.max_available > 0
    }
}

#[derive(Debug)]
pub(crate) struct ReservedQueueManager {
    queues: [ReservedQueue; MAX_RESERVED_QUEUES],
    first_in_use: Option<usize>,
    missing_spares: usize,
}

impl ReservedQueueManager {
    pub(crate) const fn new() -> Self {
        Self {
            queues: [const { ReservedQueue::new() }; MAX_RESERVED_QUEUES],
            first_in_use: None,
            missing_spares: 0,
        }
    }

    pub(crate) fn create_queue(
        &mut self,
        max_available: usize,
        npages: usize,
        mapped: bool,
        alloc_flags: AllocFlags,
    ) -> Option<usize> {
        assert!(max_available > 0 && max_available < MAX_RESERVED_PAGES);
        assert!(npages > 0 && npages < 10);

        let queue_id = (0..MAX_RESERVED_QUEUES).find(|&i| self.queues[i].max_available == 0)?;
        let queue = &mut self.queues[queue_id];

        queue.next = self.first_in_use;
        self.first_in_use = Some(queue_id);

        queue.max_available = max_available;
        queue.npages = npages;
        queue.mapped = mapped;
        queue.alloc_flags = alloc_flags;
        queue.magic = RESERVED_MAGIC;

        self.missing_spares += max_available;
        Some(queue_id)
    }

    pub(crate) fn alloc(&mut self, queue_id: usize) -> Option<(PhysAddr, Option<usize>)> {
        if queue_id >= MAX_RESERVED_QUEUES {
            return None;
        }

        let queue = &mut self.queues[queue_id];
        queue.sanity_check();

        if queue.n_available < 1 {
            return None;
        }

        queue.n_available -= 1;
        self.missing_spares += 1;

        let slot = &queue.slots[queue.n_available];
        let result = (slot.phys, slot.vir);

        queue.sanity_check();
        Some(result)
    }

    fn fill_queue(&mut self, queue_id: usize, phys: PhysAddr, vir: Option<usize>) -> bool {
        if queue_id >= MAX_RESERVED_QUEUES {
            return false;
        }

        let queue = &mut self.queues[queue_id];
        queue.sanity_check();

        if queue.n_available >= queue.max_available {
            return false;
        }

        queue.slots[queue.n_available] = ReservedSlot { phys, vir };
        queue.n_available += 1;
        self.missing_spares -= 1;

        queue.sanity_check();
        true
    }

    pub(crate) fn alloc_cycle<F>(&mut self, mut alloc_fn: F)
    where
        F: FnMut(usize, AllocFlags) -> Option<(PhysAddr, Option<usize>)>,
    {
        self.sanity_check_queues();

        let mut to_fill: Vec<(usize, usize, AllocFlags)> = Vec::new();
        let mut queue_id = self.first_in_use;
        while let Some(id) = queue_id {
            let queue = &self.queues[id];
            if queue.is_valid() {
                let needed = queue.max_available - queue.n_available;
                if needed > 0 {
                    to_fill.push((id, queue.npages, queue.alloc_flags));
                }
            }
            queue_id = queue.next;
        }

        for (id, npages, alloc_flags) in to_fill {
            if self.missing_spares == 0 {
                break;
            }

            let queue = &self.queues[id];
            let needed = queue.max_available - queue.n_available;

            for _ in 0..needed {
                if self.missing_spares == 0 {
                    break;
                }

                if let Some((phys, vir)) = alloc_fn(npages, alloc_flags) {
                    if !self.fill_queue(id, phys, vir) {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        self.sanity_check_queues();
    }

    fn sanity_check_queues(&self) {
        let mut count = 0;
        let mut queue_id = self.first_in_use;

        while let Some(id) = queue_id {
            let queue = &self.queues[id];
            assert!(queue.max_available > 0);
            assert!(queue.max_available >= queue.n_available);
            count += queue.max_available - queue.n_available;
            queue_id = queue.next;
        }

        assert_eq!(count, self.missing_spares);
    }

    pub(crate) fn missing_spares(&self) -> usize {
        self.missing_spares
    }

    pub(crate) fn get_queue_info(&self, queue_id: usize) -> Option<QueueInfo> {
        if queue_id >= MAX_RESERVED_QUEUES {
            return None;
        }

        let queue = &self.queues[queue_id];
        if !queue.is_valid() {
            return None;
        }

        Some(QueueInfo {
            max_available: queue.max_available,
            n_available: queue.n_available,
            npages: queue.npages,
            mapped: queue.mapped,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct QueueInfo {
    pub max_available: usize,
    pub n_available: usize,
    pub npages: usize,
    pub mapped: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_queue() {
        let mut manager = ReservedQueueManager::new();
        let queue_id = manager.create_queue(10, 1, false, AllocFlags::empty());
        assert!(queue_id.is_some());

        let info = manager.get_queue_info(queue_id.unwrap()).unwrap();
        assert_eq!(info.max_available, 10);
        assert_eq!(info.npages, 1);
        assert!(!info.mapped);
    }

    #[test]
    fn test_fill_and_alloc() {
        let mut manager = ReservedQueueManager::new();
        let queue_id = manager.create_queue(5, 1, false, AllocFlags::empty()).unwrap();

        for i in 0..5 {
            assert!(manager.fill_queue(queue_id, PhysAddr(i as u64 * 0x1000), None));
        }

        let info = manager.get_queue_info(queue_id).unwrap();
        assert_eq!(info.n_available, 5);

        let (phys, _) = manager.alloc(queue_id).unwrap();
        assert_eq!(phys, PhysAddr(4 * 0x1000));

        let info = manager.get_queue_info(queue_id).unwrap();
        assert_eq!(info.n_available, 4);
    }

    #[test]
    fn test_alloc_empty_queue() {
        let mut manager = ReservedQueueManager::new();
        let queue_id = manager.create_queue(5, 1, false, AllocFlags::empty()).unwrap();
        assert!(manager.alloc(queue_id).is_none());
    }

    #[test]
    fn test_alloc_cycle() {
        let mut manager = ReservedQueueManager::new();
        let queue_id = manager.create_queue(3, 1, false, AllocFlags::empty()).unwrap();

        assert_eq!(manager.missing_spares(), 3);

        let mut alloc_count = 0;
        manager.alloc_cycle(|_npages, _flags| {
            alloc_count += 1;
            Some((PhysAddr(alloc_count as u64 * 0x1000), None))
        });

        assert_eq!(manager.missing_spares(), 0);
        let info = manager.get_queue_info(queue_id).unwrap();
        assert_eq!(info.n_available, 3);
    }
}

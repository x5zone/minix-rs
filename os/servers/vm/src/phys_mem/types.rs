use core::fmt;
use super::CLICK_SIZE;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PhysAddr(u64);

impl PhysAddr {
    pub(crate) fn new(addr: u64) -> Self {
        debug_assert!(addr % CLICK_SIZE as u64 == 0, "PhysAddr must be page-aligned, got {addr:#x}");
        PhysAddr(addr)
    }

    pub(crate) const fn as_u64(&self) -> u64 {
        self.0
    }

    pub(crate) const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    pub(crate) fn from_page_index(idx: usize) -> Self {
        PhysAddr((idx as u64) * CLICK_SIZE as u64)
    }

    pub(crate) fn page_index(&self) -> usize {
        (self.0 as usize) / CLICK_SIZE
    }

    pub(crate) fn add(&self, offset: usize) -> Self {
        let new_addr = self.0 + offset as u64;
        debug_assert!(new_addr >= self.0, "PhysAddr::add overflow: {:#x} + {:#x}", self.0, offset);
        PhysAddr(new_addr)
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct PageAllocFlags: u32 {
        const CLEAR = 0x01;
        const CONTIG = 0x02;
        const ALIGN64K = 0x04;
        const LOWER16MB = 0x08;
        const LOWER1MB = 0x10;
        const ALIGN16K = 0x40;
    }
}

impl Default for PageAllocFlags {
    fn default() -> Self {
        PageAllocFlags::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllocError {
    OutOfMemory,
    LowMemoryExhausted,
    // ContiguityFailed — Minix3 定义了 PAF_CONTIG 标志，说明曾考虑过非连续物理分配，
    // 但最终 alloc_pages 始终返回连续物理页，PAF_CONTIG 实际上是 no-op。
    // 事实上只有 DMA 等少数硬件需要连续物理地址，普通进程通过页表映射后
    // 虚拟地址连续即可，物理地址是否连续无关紧要。
    // 若未来实现非连续物理分配（scatter-gather），可恢复此变体：
    // ContiguityFailed,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::OutOfMemory => write!(f, "out of memory"),
            AllocError::LowMemoryExhausted => write!(f, "low memory exhausted"),
        }
    }
}

pub(crate) struct AllocParams {
    pub(crate) clicks: usize,
    pub(crate) alloc_clicks: usize,
    pub(crate) align_clicks: usize,
    pub(crate) max_page: usize,
    pub(crate) is_low_mem: bool,
}

impl AllocParams {
    pub(crate) fn compute(clicks: usize, flags: PageAllocFlags, total_pages: usize) -> Self {
        let mut alloc_clicks = clicks;
        let mut align_clicks: usize = 0;

        if flags.contains(PageAllocFlags::ALIGN64K) {
            align_clicks = (64 * 1024) / CLICK_SIZE;
            alloc_clicks += align_clicks;
        } else if flags.contains(PageAllocFlags::ALIGN16K) {
            align_clicks = (16 * 1024) / CLICK_SIZE;
            alloc_clicks += align_clicks;
        }

        let max_page = if flags.contains(PageAllocFlags::LOWER1MB) {
            (1024 * 1024) / CLICK_SIZE
        } else if flags.contains(PageAllocFlags::LOWER16MB) {
            (16 * 1024 * 1024) / CLICK_SIZE
        } else {
            total_pages
        };

        let is_low_mem = flags.intersects(PageAllocFlags::LOWER16MB | PageAllocFlags::LOWER1MB);

        AllocParams {
            clicks,
            alloc_clicks,
            align_clicks,
            max_page,
            is_low_mem,
        }
    }

    pub(crate) fn error_type(&self) -> AllocError {
        if self.is_low_mem {
            AllocError::LowMemoryExhausted
        } else {
            AllocError::OutOfMemory
        }
    }

    pub(crate) fn aligned_result(&self, raw_page: usize) -> (usize, usize) {
        if self.align_clicks > 0 {
            let remainder = raw_page % self.align_clicks;
            if remainder > 0 {
                let leading = self.align_clicks - remainder;
                (raw_page + leading, leading)
            } else {
                (raw_page, 0)
            }
        } else {
            (raw_page, 0)
        }
    }
}

//! Memory region management module.

pub(crate) mod vir_region;
pub(crate) mod page_state;
pub(crate) mod region_map;

pub(crate) use vir_region::{VirRegion, VrFlags, VrParam, VmError, PageAllocFlags};
pub(crate) use page_state::{PageState, PageFrames, PageSlot, PageFlags, PFN_NONE, PAGE_SIZE, PfnAllocator, PfnAllocError};
pub(crate) use region_map::RegionMap;

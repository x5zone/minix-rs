//! Memory type system.

use minix_types::VirBytes;
use crate::vmproc::ActiveProc;

pub(crate) trait MemType: Send + Sync {
    fn name(&self) -> &'static str;

    fn on_new(&self, _region: &mut crate::region::VirRegion) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn on_delete(&self, _region: &mut crate::region::VirRegion) {}

    fn on_reference(
        &self,
        _src: &crate::region::PhysRegion,
        _dst: &mut crate::region::PhysRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn on_unreference(&self, _pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }

    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _pr: &mut crate::region::PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::Handled)
    }

    fn on_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn on_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) {
    }

    fn is_writable(&self, _pr: &crate::region::PhysRegion) -> bool {
        false
    }

    fn on_copy(
        &self,
        _src: &crate::region::VirRegion,
        _dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn region_id(&self, _region: &crate::region::VirRegion) -> u32 {
        0
    }

    fn ref_count(&self, _region: &crate::region::VirRegion) -> i32 {
        0
    }

    fn pt_flags(&self, _region: &crate::region::VirRegion) -> i32 {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemTypeError {
    NoMemory,
    InvalidParam,
    NotSupported,
    IoError,
    CopyFailed,
}

impl core::fmt::Display for MemTypeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoMemory => write!(f, "Out of memory"),
            Self::InvalidParam => write!(f, "Invalid parameter"),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::IoError => write!(f, "IO error"),
            Self::CopyFailed => write!(f, "Copy failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultResult {
    Handled,
    NeedNewPage,
    NeedCow,
    AccessViolation,
}

pub(crate) struct AnonymousMemory;

impl AnonymousMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for AnonymousMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn is_writable(&self, pr: &crate::region::PhysRegion) -> bool {
        if let Some(refcount) = pr.get_refcount() {
            refcount == 1
        } else {
            false
        }
    }

    fn on_unreference(&self, pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        if let Some(refcount) = pr.get_refcount() {
            if refcount == 0 {
                if let Some(_phys) = pr.get_phys_addr() {
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        let refcount = pr.get_refcount().unwrap_or(0);

        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        if !region.is_writable() {
            return Ok(PagefaultResult::AccessViolation);
        }

        Ok(PagefaultResult::NeedCow)
    }

    fn region_id(&self, _region: &crate::region::VirRegion) -> u32 {
        1
    }

    fn ref_count(&self, region: &crate::region::VirRegion) -> i32 {
        let mut count = 0i32;
        for pb in &region.physblocks {
            if let Some(pr) = pb {
                if let Some(rc) = pr.get_refcount() {
                    count += rc as i32;
                }
            }
        }
        count
    }
}

pub(crate) struct DirectPhysical;

impl DirectPhysical {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for DirectPhysical {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for DirectPhysical {
    fn name(&self) -> &'static str {
        "direct physical"
    }

    fn is_writable(&self, _pr: &crate::region::PhysRegion) -> bool {
        true
    }
}

pub(crate) struct SharedMemory;

impl SharedMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    fn is_writable(&self, _pr: &crate::region::PhysRegion) -> bool {
        true
    }
}

pub(crate) static MEM_TYPE_ANON: AnonymousMemory = AnonymousMemory::new();
pub(crate) static MEM_TYPE_DIRECT: DirectPhysical = DirectPhysical::new();
pub(crate) static MEM_TYPE_SHARED: SharedMemory = SharedMemory::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous_memory_name() {
        let anon = AnonymousMemory::new();
        assert_eq!(anon.name(), "anonymous memory");
    }

    #[test]
    fn test_direct_physical_name() {
        let direct = DirectPhysical::new();
        assert_eq!(direct.name(), "direct physical");
    }

    #[test]
    fn test_shared_memory_name() {
        let shared = SharedMemory::new();
        assert_eq!(shared.name(), "shared memory");
    }

    #[test]
    fn test_static_instances() {
        assert_eq!(MEM_TYPE_ANON.name(), "anonymous memory");
        assert_eq!(MEM_TYPE_DIRECT.name(), "direct physical");
        assert_eq!(MEM_TYPE_SHARED.name(), "shared memory");
    }
}

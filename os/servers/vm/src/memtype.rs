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

    fn on_low_shrink(
        &self,
        _region: &mut crate::region::VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    fn on_sanitycheck(
        &self,
        _pr: &crate::region::PhysRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
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
        if pr.get_phys_addr().unwrap_or(0) == 0 {
            return false;
        }
        if let Some(parent) = pr.parent {
            unsafe {
                if (*parent).remaps > 0 {
                    return true;
                }
            }
        }
        if let Some(refcount) = pr.get_refcount() {
            refcount == 1
        } else {
            false
        }
    }

    fn on_unreference(&self, pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        let refcount = pr.get_refcount().unwrap_or(0);
        if refcount == 0 && pr.get_phys_addr().unwrap_or(0) != 0 {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if pr.get_phys_addr().unwrap_or(0) == 0 {
            return Ok(PagefaultResult::NeedNewPage);
        }

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
        let mut mapped = 0i32;
        for pb in &region.physblocks {
            if pb.is_some() {
                mapped += 1;
            }
        }
        mapped
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
        "physical memory mapping"
    }

    fn is_writable(&self, pr: &crate::region::PhysRegion) -> bool {
        pr.get_phys_addr().unwrap_or(0) != 0
    }

    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let crate::region::VrParam::Direct { phys: base_phys } = &region.param {
            if *base_phys == 0 {
                return Err(MemTypeError::InvalidParam);
            }
            if pr.get_phys_addr().unwrap_or(0) != 0 {
                return Ok(PagefaultResult::Handled);
            }
            return Ok(PagefaultResult::NeedNewPage);
        }
        Err(MemTypeError::InvalidParam)
    }

    fn on_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    fn on_unreference(&self, _pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
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

    fn is_writable(&self, pr: &crate::region::PhysRegion) -> bool {
        pr.get_phys_addr().unwrap_or(0) != 0
    }

    fn on_unreference(&self, _pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }

    fn on_copy(
        &self,
        src: &crate::region::VirRegion,
        dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
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
        assert_eq!(direct.name(), "physical memory mapping");
    }

    #[test]
    fn test_shared_memory_name() {
        let shared = SharedMemory::new();
        assert_eq!(shared.name(), "shared memory");
    }

    #[test]
    fn test_static_instances() {
        assert_eq!(MEM_TYPE_ANON.name(), "anonymous memory");
        assert_eq!(MEM_TYPE_DIRECT.name(), "physical memory mapping");
        assert_eq!(MEM_TYPE_SHARED.name(), "shared memory");
    }
}

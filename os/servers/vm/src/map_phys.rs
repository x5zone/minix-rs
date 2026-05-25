//! VM physical memory mapping implementation.
//!
//! Handles VM_MAP_PHYS requests to map physical addresses into
//! virtual address space. Used by device drivers to access
//! hardware registers and DMA buffers.
//!
//! Corresponds to Minix3's `do_map_phys()` in `mmap.c`.
//!
//! PFN index model: uses PageFrames/PageSlot, VR_DIRECT regions.
//!
//! ## Design decisions vs Minix3
//!
//! **No VFS interaction**: Unlike mmap, map_phys is purely synchronous.
//! The region is created with `VR_DIRECT` and `mem_type_directphys`,
//! which handles page faults by returning the pre-configured physical
//! address directly — no page allocation needed.

use minix_types::{Endpoint, VirBytes, PhysBytes, EPERM, EINVAL, ENOMEM, ESRCH};
use crate::vmproc::VmProcTable;
use crate::region::{VirRegion, VrFlags, PageFrames};
use crate::alloc_page::VmPageAllocator;
use crate::memtype::MEM_TYPE_DIRECT;
use crate::region::page_state::PAGE_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapPhysError {
    ProcessNotFound,
    PermissionDenied,
    InvalidLength,
    OutOfMemory,
}

impl MapPhysError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::PermissionDenied => EPERM,
            Self::InvalidLength => EINVAL,
            Self::OutOfMemory => ENOMEM,
        }
    }
}

pub(crate) fn handle_map_phys(
    table: &VmProcTable,
    _page_alloc: &mut VmPageAllocator,
    _frames: &mut PageFrames,
    caller: Endpoint,
    target: Endpoint,
    phys_addr: PhysBytes,
    length: VirBytes,
) -> Result<VirBytes, MapPhysError> {
    if length.0 == 0 {
        return Err(MapPhysError::InvalidLength);
    }

    let mut target = target;
    if target == Endpoint::SELF {
        target = caller;
    }

    let slot = table
        .vm_isokendpt(target)
        .map_err(|_| MapPhysError::ProcessNotFound)?;

    let mut active = table
        .get_active(slot)
        .ok_or(MapPhysError::ProcessNotFound)?;

    map_perm_check(caller, target, phys_addr, length)?;

    // Page-align phys_addr and length
    let offset = phys_addr.0 % PAGE_SIZE;
    let len_aligned = VirBytes(length.0 + offset);
    let startaddr = PhysBytes(phys_addr.0 - offset);

    let aligned_len = VirBytes(((len_aligned.0 + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE);

    // Find a free virtual address range in the mmap region
    // In the full implementation, VM_MMAPBASE/VM_MMAPTOP are runtime values.
    // For now, use a reasonable 64-bit mmap range.
    let mmap_base = VirBytes(0x0000_0001_0000_0000);
    let mmap_top = VirBytes(0x0000_0200_0000_0000);

    let vaddr = active.regions()
        .find_slot(mmap_base, mmap_top, aligned_len)
        .ok_or(MapPhysError::OutOfMemory)?;

    let mut region = VirRegion::with_memtype(
        vaddr,
        aligned_len,
        VrFlags::WRITABLE | VrFlags::DIRECT,
        &MEM_TYPE_DIRECT,
    );
    region.param = crate::region::VrParam::Direct { phys: startaddr };

    active.regions_mut().insert(region);

    Ok(VirBytes(vaddr.0 + offset))
}

fn map_perm_check(
    caller: Endpoint,
    _target: Endpoint,
    _phys_addr: PhysBytes,
    _length: VirBytes,
) -> Result<(), MapPhysError> {
    if caller == Endpoint::TTY || caller == Endpoint::MEM {
        return Ok(());
    }

    // C source uses sys_privquery_mem(target, physaddr, len) — a kernel
    // syscall that queries whether the target process has been granted
    // access to the physical address range by PCI. This syscall is not
    // yet available in the minix-rs kernel. Until it is implemented,
    // all callers other than TTY/MEM are denied.
    Err(MapPhysError::PermissionDenied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::VmProcTable;
    use crate::phys_mem::{BitmapAllocator, PhysAlloc};
    use crate::region::PAGE_SIZE as REGION_PAGE_SIZE;
    use minix_types::UserSlot;

    fn make_frames() -> PageFrames {
        PageFrames::new(PhysBytes(256 * REGION_PAGE_SIZE as u64))
    }

    fn make_page_alloc() -> VmPageAllocator {
        VmPageAllocator::new(PhysAlloc::Bitmap(BitmapAllocator::new_for_test(256)))
    }

    fn init_test_process(slot: UserSlot) -> Endpoint {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        ep
    }

    #[test]
    fn test_map_phys_basic() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let slot = UserSlot::new(60);
        let ep = init_test_process(slot);

        let result = handle_map_phys(table, &mut page_alloc, &mut frames, Endpoint::MEM, ep, PhysBytes(0xB8000), VirBytes(0x1000));
        assert!(result.is_ok());
    }

    #[test]
    fn test_map_phys_zero_length() {
        let table = VmProcTable::get_global();
        let mut page_alloc = make_page_alloc();
        let mut frames = make_frames();
        let result = handle_map_phys(table, &mut page_alloc, &mut frames, Endpoint::MEM, Endpoint::PM, PhysBytes(0xB8000), VirBytes(0));
        assert!(matches!(result, Err(MapPhysError::InvalidLength)));
    }

    #[test]
    fn test_map_phys_error_to_errno() {
        assert_eq!(MapPhysError::PermissionDenied.to_errno(), EPERM);
        assert_eq!(MapPhysError::OutOfMemory.to_errno(), ENOMEM);
        assert_eq!(MapPhysError::InvalidLength.to_errno(), EINVAL);
        assert_eq!(MapPhysError::ProcessNotFound.to_errno(), ESRCH);
    }
}

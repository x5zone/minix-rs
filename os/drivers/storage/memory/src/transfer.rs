//! Transfer plans: what each read or write would move, without moving it.
//!
//! C correspondence: `m_transfer_kmem` and `m_transfer_mem`
//! (`memory.c:189-282`), `m_char_read` and `m_char_write`
//! (`memory.c:287-355`), and `m_block_transfer` (`memory.c:417-477`).
//!
//! The C code copies bytes with kernel copy primitives as it decides. The
//! Rust side splits the two concerns: this module computes the plan (how
//! many bytes, from which backing, with which truncation), and the service
//! crate performs the copies. Splitting keeps the policy testable without
//! a kernel and mirrors the transport split of the block-client library.

use super::device::{DeviceExtent, MemoryMinor};
use minix_types::{ENOMEM, ENXIO, OK};

/// Bytes in one page window for absolute-memory access.
///
/// C walks `/dev/mem` one page at a time (`m_transfer_mem`,
/// `memory.c:218-282`); the window size is the machine page size.
pub const PAGE_WINDOW: u64 = 4096;

/// Clamp a request to a device extent: position past the end means
/// end-of-file (zero bytes), a request crossing the end is shortened.
///
/// C: `if (position >= dv_size) return 0; if (position + size > dv_size)
/// size = dv_size - position;` (repeated in `m_transfer_kmem`,
/// `m_transfer_mem`, and `m_block_transfer`).
pub const fn clamp(position: u64, want: u64, size: u64) -> u64 {
    if position >= size {
        return 0;
    }
    let room = size - position;
    if want > room { room } else { want }
}

/// What a character read would do, decided without copying.
///
/// C: `m_char_read` (`memory.c:287-321`): null reads zero bytes (always at
/// end), zero fills the caller area (length preserved), kernel and
/// absolute memory delegate to their transfer helpers, anything else
/// stops the process (unknown device).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharReadPlan {
    /// Always at end of file: answer zero bytes.
    Eof,
    /// Fill the caller area with zero bytes, this many.
    ZeroFill(u64),
    /// Copy this many bytes from the mapped backing.
    Backed(u64),
    /// Copy through the page window (absolute memory).
    PageWindow(u64),
}

/// Decide a character read: returns the plan for a valid minor, or the
/// negative error for a block-only or unknown minor.
///
/// C: the range plus face check at `memory.c:295` refuses with "no such
/// device"; the switch fills in the per-device behavior.
pub fn char_read_plan(
    minor: u32,
    position: u64,
    want: u64,
    extent: DeviceExtent,
) -> Result<CharReadPlan, i32> {
    let Some(device) = MemoryMinor::decode(minor) else {
        return Err(-no_such_device());
    };
    if !device.is_character_only() {
        return Err(-no_such_device());
    }
    Ok(match device {
        MemoryMinor::Null => CharReadPlan::Eof,
        MemoryMinor::Zero => CharReadPlan::ZeroFill(want),
        MemoryMinor::Kmem => CharReadPlan::Backed(clamp(position, want, extent.size)),
        MemoryMinor::Mem => CharReadPlan::PageWindow(clamp(position, want, extent.size)),
        _ => return Err(-no_such_device()),
    })
}

/// What a character write would do, decided without copying.
///
/// C: `m_char_write` (`memory.c:326-355`): null and zero eat everything
/// (answer the full length), kernel and absolute memory delegate, anything
/// else stops the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharWritePlan {
    /// Swallow everything: answer the full length, store nothing.
    Sink(u64),
    /// Copy this many bytes into the mapped backing.
    Backed(u64),
    /// Copy through the page window (absolute memory).
    PageWindow(u64),
}

/// Decide a character write.
pub fn char_write_plan(
    minor: u32,
    position: u64,
    want: u64,
    extent: DeviceExtent,
) -> Result<CharWritePlan, i32> {
    let Some(device) = MemoryMinor::decode(minor) else {
        return Err(-no_such_device());
    };
    if !device.is_character_only() {
        return Err(-no_such_device());
    }
    Ok(match device {
        MemoryMinor::Null | MemoryMinor::Zero => CharWritePlan::Sink(want),
        MemoryMinor::Kmem => CharWritePlan::Backed(clamp(position, want, extent.size)),
        MemoryMinor::Mem => CharWritePlan::PageWindow(clamp(position, want, extent.size)),
        _ => return Err(-no_such_device()),
    })
}

/// Error for a minor number no face of this driver serves.
///
/// C: the range plus face check answers "no such device"
/// (`memory.c:295,335,365`).
const fn no_such_device() -> i32 {
    ENXIO
}

/// One element of a block scatter-gather walk: how many bytes move for
/// this element and whether the walk ends here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VecStep {
    /// Bytes moved for this vector element.
    pub count: u64,
    /// True when this element is fully consumed (advance to the next).
    pub consumed: bool,
}

/// Walk one block vector element against the remaining device room.
///
/// C: the inner body of `m_block_transfer` (`memory.c:445-475`): each
/// element is truncated to the room left, the running total and position
/// advance, and a fully consumed element moves to the next vector slot.
/// A position past the end finishes the walk with zero (`memory.c:456`);
/// a position whose high half is nonzero is beyond end-of-file and the
/// whole transfer answers zero (`memory.c:442-443`).
pub fn vec_step(element: u64, position: u64, room: u64, high_half_nonzero: bool) -> VecStep {
    if high_half_nonzero || position >= room {
        return VecStep {
            count: 0,
            consumed: false,
        };
    }
    let left = room - position;
    let count = if element > left { left } else { element };
    VecStep {
        count,
        consumed: count == element,
    }
}

/// Physical page mapping behind one trait: the service crate maps real
/// pages, tests map memory.
///
/// C: `vm_map_phys` and `vm_unmap_phys` around a single cached page
/// (`m_transfer_mem`, `memory.c:247-261`). Port numbers and kernel calls
/// never enter this crate; the trait moves bytes between the window and
/// the test backing.
pub trait PageMapper {
    /// Map the page starting at this physical address into the window.
    fn map(&mut self, page_start: u64) -> Result<(), MapperError>;
    /// Forget the current mapping.
    fn unmap(&mut self);
    /// Current window contents.
    fn window(&self) -> &[u8];
    /// Writable window contents.
    fn window_mut(&mut self) -> &mut [u8];
}

/// Mapper failure: only out-of-memory exists in C (`memory.c:254-258`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapperError {
    /// No memory for the mapping (C answers "no memory").
    NoMemory,
}

impl MapperError {
    /// Error code the driver answers for this failure.
    pub const fn code(self) -> i32 {
        match self {
            MapperError::NoMemory => ENOMEM,
        }
    }
}

/// Failing mapper: every map fails (exercises the no-memory path).
#[derive(Debug, Default, Clone, Copy)]
pub struct NullMapper;

impl PageMapper for NullMapper {
    fn map(&mut self, _page_start: u64) -> Result<(), MapperError> {
        Err(MapperError::NoMemory)
    }

    fn unmap(&mut self) {}

    fn window(&self) -> &[u8] {
        &[]
    }

    fn window_mut(&mut self) -> &mut [u8] {
        &mut []
    }
}

/// In-memory mapper for tests: pages live in a vector, the window is one
/// page of scratch space.
#[derive(Debug, Clone)]
pub struct MemMapper {
    pages: alloc::vec::Vec<u8>,
    window: [u8; PAGE_WINDOW as usize],
    mapped: Option<u64>,
}

impl MemMapper {
    /// Backing of this many zeroed pages.
    pub fn new(pages: usize) -> MemMapper {
        MemMapper {
            pages: alloc::vec![0; pages * PAGE_WINDOW as usize],
            window: [0; PAGE_WINDOW as usize],
            mapped: None,
        }
    }

    /// Read back one backing byte (test inspection).
    pub fn peek(&self, offset: u64) -> Option<u8> {
        self.pages.get(offset as usize).copied()
    }
}

impl PageMapper for MemMapper {
    fn map(&mut self, page_start: u64) -> Result<(), MapperError> {
        if !page_start.is_multiple_of(PAGE_WINDOW) {
            return Err(MapperError::NoMemory);
        }
        let start = page_start as usize;
        let end = start + PAGE_WINDOW as usize;
        if end > self.pages.len() {
            return Err(MapperError::NoMemory);
        }
        self.window.copy_from_slice(&self.pages[start..end]);
        self.mapped = Some(page_start);
        Ok(())
    }

    fn unmap(&mut self) {
        if let Some(start) = self.mapped.take() {
            let start = start as usize;
            let end = start + PAGE_WINDOW as usize;
            if end <= self.pages.len() {
                self.pages[start..end].copy_from_slice(&self.window);
            }
        }
    }

    fn window(&self) -> &[u8] {
        &self.window
    }

    fn window_mut(&mut self) -> &mut [u8] {
        &mut self.window
    }
}

/// Single cached page window: the `m_transfer_mem` caching algorithm.
///
/// C keeps one mapped page (`any_mapped`, `pagestart_mapped`,
/// `memory.c:222-224`) and only remaps when the next byte lives on another
/// page. This type renders exactly that policy over any [`PageMapper`]:
/// `select` maps on page change, `flush` writes the window back.
pub struct PageWindow<M> {
    mapper: M,
    mapped: Option<u64>,
}

impl<M: PageMapper> PageWindow<M> {
    /// Fresh window with no page mapped.
    pub fn new(mapper: M) -> PageWindow<M> {
        PageWindow {
            mapper,
            mapped: None,
        }
    }

    /// Ensure the page holding this physical address is mapped.
    pub fn select(&mut self, phys: u64) -> Result<(), MapperError> {
        let start = phys - phys % PAGE_WINDOW;
        if self.mapped == Some(start) {
            return Ok(());
        }
        if self.mapped.is_some() {
            self.mapper.unmap();
            self.mapped = None;
        }
        self.mapper.map(start)?;
        self.mapped = Some(start);
        Ok(())
    }

    /// Write the window back and forget the mapping.
    pub fn flush(&mut self) {
        if self.mapped.is_some() {
            self.mapper.unmap();
            self.mapped = None;
        }
    }

    /// Byte offset of the mapped page, if any.
    pub fn mapped_page(&self) -> Option<u64> {
        self.mapped
    }

    /// Borrow the mapper (window access in tests).
    pub fn mapper(&self) -> &M {
        &self.mapper
    }

    /// Mutably borrow the mapper.
    pub fn mapper_mut(&mut self) -> &mut M {
        &mut self.mapper
    }
}

/// Success marker shared with the device module.
pub const SUCCESS: i32 = OK;

#[cfg(test)]
mod tests {
    use super::super::device::{DeviceExtent, MemoryMinor};
    use super::*;

    fn extent(size: u64) -> DeviceExtent {
        DeviceExtent { base: 0, size }
    }

    #[test]
    fn test_clamp_covers_eof_and_shortening() {
        assert_eq!(clamp(100, 10, 90), 0);
        assert_eq!(clamp(80, 20, 90), 10);
        assert_eq!(clamp(80, 5, 90), 5);
        assert_eq!(clamp(0, 0, 90), 0);
    }

    #[test]
    fn test_null_read_is_always_eof_and_write_is_sink() {
        let ext = extent(0);
        assert_eq!(char_read_plan(3, 0, 100, ext), Ok(CharReadPlan::Eof));
        assert_eq!(
            char_write_plan(3, 0, 100, ext),
            Ok(CharWritePlan::Sink(100))
        );
        assert_eq!(
            char_write_plan(5, 0, 100, ext),
            Ok(CharWritePlan::Sink(100))
        );
    }

    #[test]
    fn test_zero_read_preserves_length() {
        let ext = extent(0);
        assert_eq!(
            char_read_plan(5, 0, 4096, ext),
            Ok(CharReadPlan::ZeroFill(4096))
        );
    }

    #[test]
    fn test_kmem_plans_truncate_to_backing() {
        let ext = extent(100);
        assert_eq!(char_read_plan(2, 90, 20, ext), Ok(CharReadPlan::Backed(10)));
        assert_eq!(char_read_plan(2, 100, 20, ext), Ok(CharReadPlan::Backed(0)));
        assert_eq!(
            char_write_plan(2, 0, 50, ext),
            Ok(CharWritePlan::Backed(50))
        );
    }

    #[test]
    fn test_block_minors_are_refused_on_character_face() {
        let ext = extent(100);
        assert!(char_read_plan(7, 0, 10, ext).is_err());
        assert!(char_write_plan(0, 0, 10, ext).is_err());
        assert!(char_read_plan(99, 0, 10, ext).is_err());
    }

    #[test]
    fn test_vec_step_truncates_and_marks_consumed() {
        assert_eq!(
            vec_step(512, 0, 1024, false),
            VecStep {
                count: 512,
                consumed: true
            }
        );
        assert_eq!(
            vec_step(512, 800, 1024, false),
            VecStep {
                count: 224,
                consumed: false
            }
        );
        assert_eq!(
            vec_step(512, 1024, 1024, false),
            VecStep {
                count: 0,
                consumed: false
            }
        );
        assert_eq!(
            vec_step(512, 0, 1024, true),
            VecStep {
                count: 0,
                consumed: false
            }
        );
    }

    #[test]
    fn test_null_mapper_reports_no_memory() {
        let mut window = PageWindow::new(NullMapper);
        assert_eq!(window.select(0), Err(MapperError::NoMemory));
        assert_eq!(MapperError::NoMemory.code(), ENOMEM);
        assert_eq!(window.mapped_page(), None);
    }

    #[test]
    fn test_page_window_caches_one_page() {
        let mut window = PageWindow::new(MemMapper::new(2));
        window.select(100).unwrap();
        assert_eq!(window.mapped_page(), Some(0));
        window.select(200).unwrap();
        assert_eq!(window.mapped_page(), Some(0));
        window.select(5000).unwrap();
        assert_eq!(window.mapped_page(), Some(4096));
        window.flush();
        assert_eq!(window.mapped_page(), None);
    }

    #[test]
    fn test_mem_mapper_round_trips_bytes() {
        let mut mapper = MemMapper::new(1);
        mapper.map(0).unwrap();
        mapper.window_mut()[7] = 0xAB;
        mapper.unmap();
        assert_eq!(mapper.peek(7), Some(0xAB));
        assert!(mapper.map(4096).is_err());
    }

    #[test]
    fn test_minor_numbers_visible_from_transfer() {
        assert_eq!(MemoryMinor::Null.number(), 3);
        assert_eq!(SUCCESS, OK);
    }
}

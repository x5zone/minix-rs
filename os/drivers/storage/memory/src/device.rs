//! Memory devices: minor numbers, geometry table, and open counts.
//!
//! C correspondence: the device-number defines in
//! `minix3/minix/include/minix/dmap.h:85-92`, the geometry and open-count
//! tables plus `m_is_block` in `minix3/minix/drivers/storage/memory/
//! memory.c:36-46,167-184`, the open and close counters in
//! `m_char_open`, `m_char_close`, `m_block_open`, `m_block_close`
//! (`memory.c:360-401,482-507`), and the RAM-disk resize policy in
//! `m_block_ioctl` (`memory.c:512-599`).

use minix_types::{EBUSY, EINVAL, ENXIO, OK};

/// Number of RAM disks after the fixed devices.
///
/// C: `RAMDISKS 6` (`memory.c:37`).
pub const RAMDISK_COUNT: u32 = 6;

/// Total minor devices: seven fixed plus six RAM disks.
///
/// C: `NR_DEVS (7+RAMDISKS)` (`memory.c:41`): thirteen devices, minors
/// zero through twelve.
pub const MINOR_COUNT: u32 = 7 + RAMDISK_COUNT;

/// Minor device of a memory device.
///
/// C: `RAM_DEV_OLD`, `MEM_DEV`, `KMEM_DEV`, `NULL_DEV`, `BOOT_DEV`,
/// `ZERO_DEV`, `IMGRD_DEV`, `RAM_DEV_FIRST` (`dmap.h:85-92`). The six RAM
/// disks are one variant with an index instead of six constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryMinor {
    /// Old RAM disk compatibility device (minor zero, `/dev/ram`).
    RamOld,
    /// Absolute physical memory (minor one, `/dev/mem`).
    Mem,
    /// Kernel virtual memory (minor two, `/dev/kmem`).
    Kmem,
    /// Data sink (minor three, `/dev/null`).
    Null,
    /// Boot device image (minor four, `/dev/boot`).
    Boot,
    /// Zero byte generator (minor five, `/dev/zero`).
    Zero,
    /// Boot image RAM disk (minor six, `/dev/imgrd`).
    ImageDisk,
    /// Numbered RAM disk (minors seven through twelve, `/dev/ram*`).
    Ram(u32),
}

impl MemoryMinor {
    /// Decode a raw minor number; `None` means out of range.
    pub const fn decode(minor: u32) -> Option<MemoryMinor> {
        match minor {
            0 => Some(MemoryMinor::RamOld),
            1 => Some(MemoryMinor::Mem),
            2 => Some(MemoryMinor::Kmem),
            3 => Some(MemoryMinor::Null),
            4 => Some(MemoryMinor::Boot),
            5 => Some(MemoryMinor::Zero),
            6 => Some(MemoryMinor::ImageDisk),
            n if n >= 7 && n < 7 + RAMDISK_COUNT => Some(MemoryMinor::Ram(n - 7)),
            _ => None,
        }
    }

    /// Raw minor number of this device.
    pub const fn number(self) -> u32 {
        match self {
            MemoryMinor::RamOld => 0,
            MemoryMinor::Mem => 1,
            MemoryMinor::Kmem => 2,
            MemoryMinor::Null => 3,
            MemoryMinor::Boot => 4,
            MemoryMinor::Zero => 5,
            MemoryMinor::ImageDisk => 6,
            MemoryMinor::Ram(index) => 7 + index,
        }
    }

    /// True for character-only devices (never served on the block face).
    ///
    /// C: `m_is_block` (`memory.c:170-184`) returns false for memory,
    /// kernel memory, null, and zero; everything else answers on both
    /// faces. A character request for a block-only minor (or the reverse)
    /// is refused with "no such device".
    pub const fn is_character_only(self) -> bool {
        matches!(
            self,
            MemoryMinor::Mem | MemoryMinor::Kmem | MemoryMinor::Null | MemoryMinor::Zero
        )
    }
}

/// Byte extent of one device: base offset plus size.
///
/// C: `struct device` with `dv_base` and `dv_size` (shared driver header).
/// The table `m_geom[NR_DEVS]` (`memory.c:43`) holds one extent per minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceExtent {
    /// First byte owned by the device.
    pub base: u64,
    /// Number of bytes owned by the device.
    pub size: u64,
}

/// Geometry table plus open counts for all thirteen minors.
///
/// C: `m_geom`, `m_vaddrs`, `openct` (`memory.c:43-46`). Fresh-start
/// initialization sets the image-disk extent from the linked-in image, the
/// memory extent to the full four gigabytes, and every open count to zero
/// (`sef_cb_init_fresh`, `memory.c:126-165`). Backing addresses stay in the
/// service crate (they are virtual addresses, a transport concern); this
/// table owns numbers only.
#[derive(Debug, Clone)]
pub struct DeviceTable {
    extents: [DeviceExtent; MINOR_COUNT as usize],
    opens: [u32; MINOR_COUNT as usize],
}

impl DeviceTable {
    /// Fresh table: memory spans four gigabytes, everything else empty.
    ///
    /// C: `m_geom[MEM_DEV]` set to base zero size `0xffffffff`
    /// (`memory.c:156-157`); all other extents are filled by the service
    /// crate at startup (image disk) or by the resize control (RAM disks).
    pub const fn new() -> DeviceTable {
        DeviceTable {
            extents: [DeviceExtent { base: 0, size: 0 }; MINOR_COUNT as usize],
            opens: [0; MINOR_COUNT as usize],
        }
    }

    /// Start from [`DeviceTable::new`] with the memory extent filled.
    pub fn fresh() -> DeviceTable {
        let mut table = DeviceTable::new();
        table.extents[MemoryMinor::Mem.number() as usize] = DeviceExtent {
            base: 0,
            size: 0xffff_ffff,
        };
        table
    }

    /// Extent of one minor; `None` for an undecodable number.
    pub fn extent(&self, minor: u32) -> Option<DeviceExtent> {
        MemoryMinor::decode(minor).map(|device| self.extents[device.number() as usize])
    }

    /// Replace the extent of one minor (startup fill or resize control).
    pub fn set_extent(&mut self, minor: u32, extent: DeviceExtent) -> bool {
        match MemoryMinor::decode(minor) {
            Some(device) => {
                self.extents[device.number() as usize] = extent;
                true
            }
            None => false,
        }
    }

    /// Record one open; refuses unknown or character-only-on-block minors
    /// the same way on both faces.
    ///
    /// C: `m_char_open` and `m_block_open` both refuse out-of-range minors
    /// with "no such device" (`memory.c:365,485`) and count the rest.
    pub fn open(&mut self, minor: u32, block_face: bool) -> i32 {
        let Some(device) = MemoryMinor::decode(minor) else {
            return -ENXIO;
        };
        if block_face == device.is_character_only() {
            return -ENXIO;
        }
        let slot = &mut self.opens[device.number() as usize];
        *slot = slot.saturating_add(1);
        OK
    }

    /// Record one close; closing an unopened device is invalid.
    ///
    /// C: `m_char_close` and `m_block_close` print a diagnostic and return
    /// "invalid argument" when the count is already zero
    /// (`memory.c:394-397,500-503`).
    pub fn close(&mut self, minor: u32, block_face: bool) -> i32 {
        let Some(device) = MemoryMinor::decode(minor) else {
            return -ENXIO;
        };
        if block_face == device.is_character_only() {
            return -ENXIO;
        }
        let slot = &mut self.opens[device.number() as usize];
        if *slot == 0 {
            return -EINVAL;
        }
        *slot -= 1;
        OK
    }

    /// Open count of one minor (zero for unknown numbers).
    pub fn open_count(&self, minor: u32) -> u32 {
        MemoryMinor::decode(minor)
            .map(|device| self.opens[device.number() as usize])
            .unwrap_or(0)
    }
}

impl Default for DeviceTable {
    fn default() -> Self {
        DeviceTable::fresh()
    }
}

/// Verdict of the RAM-disk resize control.
///
/// C: `m_block_ioctl` answers `MIOCRAMSIZE` (`memory.c:512-599`). Only RAM
/// disks (plus the old compatibility minor and the image disk, which is
/// forced to size zero) accept it; anything else is "invalid argument".
/// A disk already at the wanted size is a no-op success; a disk opened
/// more than once (the control itself holds one open) is busy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeVerdict {
    /// Not a RAM disk at all: refuse.
    NotRamdisk,
    /// Already exactly this size (or image disk forced to zero): success.
    NoChange,
    /// Opened by someone else besides this control: busy.
    Busy,
    /// May tear down the old backing and allocate the new size.
    Resize,
}

/// Decide a RAM-disk resize without touching any backing store.
///
/// `is_image` marks the image-disk minor (forced to zero size);
/// `open_count` includes the control's own open; `current` and `want` are
/// byte sizes. Backing teardown and allocation stay in the service crate.
pub fn resize_policy(
    minor: u32,
    is_image: bool,
    open_count: u32,
    current: u64,
    want: u64,
) -> ResizeVerdict {
    let want = if is_image { 0 } else { want };
    let Some(device) = MemoryMinor::decode(minor) else {
        return ResizeVerdict::NotRamdisk;
    };
    let ramdisk = matches!(
        device,
        MemoryMinor::Ram(_) | MemoryMinor::RamOld | MemoryMinor::ImageDisk
    );
    if !ramdisk {
        return ResizeVerdict::NotRamdisk;
    }
    if current == want {
        return ResizeVerdict::NoChange;
    }
    if open_count != 1 {
        return ResizeVerdict::Busy;
    }
    ResizeVerdict::Resize
}

/// Error for a face mismatch (character request on a block minor or back).
pub const fn face_mismatch() -> i32 {
    -ENXIO
}

/// Error for closing a device nobody opened.
pub const fn close_unopened() -> i32 {
    -EINVAL
}

/// Error for resizing a busy RAM disk.
pub const fn resize_busy() -> i32 {
    -EBUSY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minor_numbers_match_dmap_header() {
        assert_eq!(MemoryMinor::Mem.number(), 1);
        assert_eq!(MemoryMinor::Kmem.number(), 2);
        assert_eq!(MemoryMinor::Null.number(), 3);
        assert_eq!(MemoryMinor::Boot.number(), 4);
        assert_eq!(MemoryMinor::Zero.number(), 5);
        assert_eq!(MemoryMinor::ImageDisk.number(), 6);
        assert_eq!(MemoryMinor::Ram(0).number(), 7);
        assert_eq!(MemoryMinor::Ram(5).number(), 12);
        assert_eq!(MemoryMinor::decode(13), None);
    }

    #[test]
    fn test_character_only_set_matches_is_block() {
        assert!(MemoryMinor::Mem.is_character_only());
        assert!(MemoryMinor::Kmem.is_character_only());
        assert!(MemoryMinor::Null.is_character_only());
        assert!(MemoryMinor::Zero.is_character_only());
        assert!(!MemoryMinor::RamOld.is_character_only());
        assert!(!MemoryMinor::Boot.is_character_only());
        assert!(!MemoryMinor::ImageDisk.is_character_only());
        assert!(!MemoryMinor::Ram(2).is_character_only());
    }

    #[test]
    fn test_open_refuses_wrong_face_and_unknown_minors() {
        let mut table = DeviceTable::fresh();
        assert_eq!(table.open(1, true), -ENXIO);
        assert_eq!(table.open(7, false), -ENXIO);
        assert_eq!(table.open(99, false), -ENXIO);
        assert_eq!(table.open(1, false), OK);
        assert_eq!(table.open(7, true), OK);
    }

    #[test]
    fn test_close_unopened_is_invalid_argument() {
        let mut table = DeviceTable::fresh();
        assert_eq!(table.close(1, false), -EINVAL);
        assert_eq!(table.open(1, false), OK);
        assert_eq!(table.close(1, false), OK);
        assert_eq!(table.close(1, false), -EINVAL);
    }

    #[test]
    fn test_resize_policy_covers_all_branches() {
        assert_eq!(
            resize_policy(1, false, 1, 0, 4096),
            ResizeVerdict::NotRamdisk
        );
        assert_eq!(
            resize_policy(99, false, 1, 0, 4096),
            ResizeVerdict::NotRamdisk
        );
        assert_eq!(
            resize_policy(7, false, 1, 4096, 4096),
            ResizeVerdict::NoChange
        );
        assert_eq!(resize_policy(6, true, 1, 0, 4096), ResizeVerdict::NoChange);
        assert_eq!(resize_policy(7, false, 3, 0, 4096), ResizeVerdict::Busy);
        assert_eq!(resize_policy(7, false, 1, 0, 4096), ResizeVerdict::Resize);
        assert_eq!(resize_policy(0, false, 1, 0, 1024), ResizeVerdict::Resize);
    }

    #[test]
    fn test_fresh_table_spans_four_gigabytes_for_mem() {
        let table = DeviceTable::fresh();
        let extent = table.extent(1).unwrap();
        assert_eq!(extent.base, 0);
        assert_eq!(extent.size, 0xffff_ffff);
        assert_eq!(table.open_count(1), 0);
    }
}

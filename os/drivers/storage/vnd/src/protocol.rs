//! The VND control protocol: ioctl numbers and the structures they carry.
//!
//! C correspondence: `sys/dev/vndvar.h` (the Minix variant of
//! `struct vnd_ioctl` and `struct vnd_user`, plus the four ioctl numbers
//! built with `sys/ioccom.h`'s `_IOC` macro) and
//! `minix3/minix/include/sys/ioc_disk.h` (the two disk ioctls the driver
//! also answers: `DIOCOPENCT`, `DIOCFLUSH`).
//!
//! The numbers are derived here with the same arithmetic the C macro
//! performs — `inout | (len & 0xfff) << 16 | group << 8 | num` — and the
//! tests pin both the arithmetic and the resulting constants, so a change
//! to either half shows up as a test failure rather than as a caller that
//! silently sends a different control code.

/// Direction bit: parameters are copied in (`IOC_IN`).
pub const IOC_IN: u64 = 0x8000_0000;
/// Direction bit: parameters are copied out (`IOC_OUT`).
pub const IOC_OUT: u64 = 0x4000_0000;
/// Parameter length occupies bits 16..27 (`IOCPARM_MASK`, 12 bits on
/// Minix) and the group byte bits 8..15.
pub const IOCPARM_MASK: u64 = 0xfff;

/// Builds one ioctl number (`_IOC(inout, group, num, len)`).
pub const fn ioc(inout: u64, group: u8, num: u64, len: u64) -> u64 {
    inout | ((len & IOCPARM_MASK) << 16) | ((group as u64) << 8) | num
}

/// `struct vndgeom` (`vndvar.h:78-83`): four 32-bit geometry words.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VndGeom {
    /// Bytes per sector (`vng_secsize`).
    pub secsize: u32,
    /// Data sectors per track (`vng_nsectors`).
    pub nsectors: u32,
    /// Tracks per cylinder (`vng_ntracks`).
    pub ntracks: u32,
    /// Cylinders per unit (`vng_ncylinders`).
    pub ncylinders: u32,
}

/// `struct vnd_ioctl` in its Minix shape (`vndvar.h:94-105`): the file
/// descriptor to steal, the flags, the requested geometry, and the
/// device size the driver answers with.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VndIoctl {
    /// File descriptor of the backing file (`vnd_fildes`).
    pub fildes: i32,
    /// Control flags ([`VNDIOF_HASGEOM`] and friends).
    pub flags: i32,
    /// The geometry to emulate (`vnd_geom`).
    pub geom: VndGeom,
    /// Device size in bytes (returned).
    pub size: u64,
}

/// `struct vnd_user` (`vndvar.h:171-175`): the unit number and, when the
/// unit is configured, the device and inode the backing file lives on.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VndUser {
    /// Which VND unit (`vnu_unit`).
    pub unit: i32,
    /// Padding before the 64-bit fields (LP64 layout).
    pub _padding: u32,
    /// The file's device (`vnu_dev`).
    pub dev: u64,
    /// The file's inode (`vnu_ino`).
    pub ino: u64,
}

/// Use the geometry supplied in the request (`VNDIOF_HASGEOM`).
pub const VNDIOF_HASGEOM: i32 = 0x01;
/// Configure the device read-only (`VNDIOF_READONLY`).
pub const VNDIOF_READONLY: i32 = 0x02;
/// Force unconfiguration even while the device is open (`VNDIOF_FORCE`).
pub const VNDIOF_FORCE: i32 = 0x04;

/// Enable the disk (`VNDIOCSET`, `vndvar.h:183`).
pub const VNDIOCSET: u64 = ioc(
    IOC_IN | IOC_OUT,
    b'F',
    0,
    core::mem::size_of::<VndIoctl>() as u64,
);
/// Disable the disk (`VNDIOCCLR`, `vndvar.h:184`).
pub const VNDIOCCLR: u64 = ioc(IOC_IN, b'F', 1, core::mem::size_of::<VndIoctl>() as u64);
/// Report the configured unit (`VNDIOCGET`, `vndvar.h:185`).
pub const VNDIOCGET: u64 = ioc(
    IOC_IN | IOC_OUT,
    b'F',
    3,
    core::mem::size_of::<VndUser>() as u64,
);
/// Report the open count (`DIOCOPENCT`, `ioc_disk.h:15`).
pub const DIOCOPENCT: u64 = ioc(IOC_OUT, b'd', 7, 4);
/// Flush the backing file (`DIOCFLUSH`, `ioc_disk.h:16`).
pub const DIOCFLUSH: u64 = ioc(0, b'd', 8, 0);

/// Where each `struct vnd_ioctl` field sits in the ioctl buffer.
pub mod wire {
    use super::*;

    /// The file-descriptor field.
    pub const FILDES: usize = core::mem::offset_of!(VndIoctl, fildes);
    /// The flags field.
    pub const FLAGS: usize = core::mem::offset_of!(VndIoctl, flags);
    /// The geometry block.
    pub const GEOM: usize = core::mem::offset_of!(VndIoctl, geom);
    /// The size field (returned).
    pub const SIZE: usize = core::mem::offset_of!(VndIoctl, size);
    /// The `struct vnd_user` unit field.
    pub const USER_UNIT: usize = core::mem::offset_of!(VndUser, unit);
    /// The `struct vnd_user` device field.
    pub const USER_DEV: usize = core::mem::offset_of!(VndUser, dev);
    /// The `struct vnd_user` inode field.
    pub const USER_INO: usize = core::mem::offset_of!(VndUser, ino);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_struct_sizes_and_offsets_match_the_c_layout() {
        // vndvar.h (Minix branch): int fildes, int flags, four u32, u64.
        assert_eq!(core::mem::size_of::<VndIoctl>(), 32);
        assert_eq!(wire::FILDES, 0);
        assert_eq!(wire::FLAGS, 4);
        assert_eq!(wire::GEOM, 8);
        assert_eq!(wire::SIZE, 24);
        assert_eq!(core::mem::size_of::<VndGeom>(), 16);
        // LP64: the unit word, then padding, then dev and ino.
        assert_eq!(core::mem::size_of::<VndUser>(), 24);
        assert_eq!(wire::USER_UNIT, 0);
        assert_eq!(wire::USER_DEV, 8);
        assert_eq!(wire::USER_INO, 16);
    }

    #[test]
    fn test_ioctl_numbers_derive_from_the_ioc_macro() {
        // The arithmetic itself.
        assert_eq!(ioc(IOC_IN | IOC_OUT, b'F', 0, 32), 0xC020_4600);
        assert_eq!(ioc(IOC_IN, b'F', 1, 32), 0x8020_4601);
        assert_eq!(ioc(0, b'd', 8, 0), 0x0000_6408);
        // And the constants the driver compares against.
        assert_eq!(VNDIOCSET, 0xC020_4600);
        assert_eq!(VNDIOCCLR, 0x8020_4601);
        assert_eq!(VNDIOCGET, 0xC018_4603);
        assert_eq!(DIOCOPENCT, 0x4004_6407);
        assert_eq!(DIOCFLUSH, 0x0000_6408);
        // Direction and group survive the round trip.
        assert_eq!((VNDIOCSET >> 8) & 0xff, b'F' as u64);
        assert_eq!((VNDIOCSET >> 16) & IOCPARM_MASK, 32);
        assert_eq!((DIOCFLUSH >> 8) & 0xff, b'd' as u64);
    }

    #[test]
    fn test_flags_match_vndvar_header() {
        assert_eq!(VNDIOF_HASGEOM, 0x01);
        assert_eq!(VNDIOF_READONLY, 0x02);
        assert_eq!(VNDIOF_FORCE, 0x04);
    }
}

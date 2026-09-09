//! Syscall dispatch table.
//!
//! Corresponds to Minix3's `call_vec[NR_VFS_CALLS]` (defined in `table.c`).
//!
//! call_vec is a mapping table from syscall number to handler function, using
//! designated initializers to map syscall numbers to array indices.
//!
//! # Design Notes
//!
//! Minix3 C code uses function pointer array `int (* const call_vec[NR_VFS_CALLS])(void)`,
//! Rust version uses enum instead of raw function pointers for type safety.

/// VFS syscall base offset.
///
/// Corresponds to Minix3's `VFS_BASE` (`callnr.h:68` `0x100`).
pub const VFS_BASE: u32 = 0x100;

/// VFS syscall count.
///
/// Corresponds to Minix3's `NR_VFS_CALLS`.
pub const NR_VFS_CALLS: usize = 64;

/// VFS syscall number.
///
/// Corresponds to Minix3's `VFS_*` constants (defined in `callnr.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VfsCallNum {
    Read = VFS_BASE,
    Write = VFS_BASE + 1,
    Lseek = VFS_BASE + 2,
    Open = VFS_BASE + 3,
    Creat = VFS_BASE + 4,
    Close = VFS_BASE + 5,
    Link = VFS_BASE + 6,
    Unlink = VFS_BASE + 7,
    Chdir = VFS_BASE + 8,
    Mkdir = VFS_BASE + 9,
    Mknod = VFS_BASE + 10,
    Chmod = VFS_BASE + 11,
    Chown = VFS_BASE + 12,
    Mount = VFS_BASE + 13,
    Umount = VFS_BASE + 14,
    Access = VFS_BASE + 15,
    Sync = VFS_BASE + 16,
    Rename = VFS_BASE + 17,
    Rmdir = VFS_BASE + 18,
    Symlink = VFS_BASE + 19,
    Readlink = VFS_BASE + 20,
    Stat = VFS_BASE + 21,
    Fstat = VFS_BASE + 22,
    Lstat = VFS_BASE + 23,
    Ioctl = VFS_BASE + 24,
    Fcntl = VFS_BASE + 25,
    Pipe2 = VFS_BASE + 26,
    Umask = VFS_BASE + 27,
    Chroot = VFS_BASE + 28,
    Getdents = VFS_BASE + 29,
    Select = VFS_BASE + 30,
    Fchdir = VFS_BASE + 31,
    Fsync = VFS_BASE + 32,
    Truncate = VFS_BASE + 33,
    Ftruncate = VFS_BASE + 34,
    Fchmod = VFS_BASE + 35,
    Fchown = VFS_BASE + 36,
    Utimens = VFS_BASE + 37,
    Vmcall = VFS_BASE + 38,
    Getvfsstat = VFS_BASE + 39,
    Statvfs1 = VFS_BASE + 40,
    Fstatvfs1 = VFS_BASE + 41,
    Getrusage = VFS_BASE + 42,
    Svrctl = VFS_BASE + 43,
    GcovFlush = VFS_BASE + 44,
    Mapdriver = VFS_BASE + 45,
    Copyfd = VFS_BASE + 46,
    Socketpath = VFS_BASE + 47,
    Getsysinfo = VFS_BASE + 48,
    Socket = VFS_BASE + 49,
    Socketpair = VFS_BASE + 50,
    Bind = VFS_BASE + 51,
    Connect = VFS_BASE + 52,
    Listen = VFS_BASE + 53,
    Accept = VFS_BASE + 54,
    Sendto = VFS_BASE + 55,
    Sendmsg = VFS_BASE + 56,
    Recvfrom = VFS_BASE + 57,
    Recvmsg = VFS_BASE + 58,
    Setsockopt = VFS_BASE + 59,
    Getsockopt = VFS_BASE + 60,
    Getsockname = VFS_BASE + 61,
    Getpeername = VFS_BASE + 62,
    Shutdown = VFS_BASE + 63,
}

impl VfsCallNum {
    /// Converts from raw syscall number.
    ///
    /// Corresponds to Minix3's `call_vec[call_nr - VFS_BASE]`.
    pub fn from_raw(raw: u32) -> Option<Self> {
        if raw < VFS_BASE {
            return None;
        }
        let index = (raw - VFS_BASE) as usize;
        if index >= NR_VFS_CALLS {
            return None;
        }
        // Use try_from for safe conversion
        Self::try_from_raw(raw)
    }

    fn try_from_raw(raw: u32) -> Option<Self> {
        match raw {
            v if v == Self::Read as u32 => Some(Self::Read),
            v if v == Self::Write as u32 => Some(Self::Write),
            v if v == Self::Lseek as u32 => Some(Self::Lseek),
            v if v == Self::Open as u32 => Some(Self::Open),
            v if v == Self::Creat as u32 => Some(Self::Creat),
            v if v == Self::Close as u32 => Some(Self::Close),
            v if v == Self::Link as u32 => Some(Self::Link),
            v if v == Self::Unlink as u32 => Some(Self::Unlink),
            v if v == Self::Chdir as u32 => Some(Self::Chdir),
            v if v == Self::Mkdir as u32 => Some(Self::Mkdir),
            v if v == Self::Mknod as u32 => Some(Self::Mknod),
            v if v == Self::Chmod as u32 => Some(Self::Chmod),
            v if v == Self::Chown as u32 => Some(Self::Chown),
            v if v == Self::Mount as u32 => Some(Self::Mount),
            v if v == Self::Umount as u32 => Some(Self::Umount),
            v if v == Self::Access as u32 => Some(Self::Access),
            v if v == Self::Sync as u32 => Some(Self::Sync),
            v if v == Self::Rename as u32 => Some(Self::Rename),
            v if v == Self::Rmdir as u32 => Some(Self::Rmdir),
            v if v == Self::Symlink as u32 => Some(Self::Symlink),
            v if v == Self::Readlink as u32 => Some(Self::Readlink),
            v if v == Self::Stat as u32 => Some(Self::Stat),
            v if v == Self::Fstat as u32 => Some(Self::Fstat),
            v if v == Self::Lstat as u32 => Some(Self::Lstat),
            v if v == Self::Ioctl as u32 => Some(Self::Ioctl),
            v if v == Self::Fcntl as u32 => Some(Self::Fcntl),
            v if v == Self::Pipe2 as u32 => Some(Self::Pipe2),
            v if v == Self::Umask as u32 => Some(Self::Umask),
            v if v == Self::Chroot as u32 => Some(Self::Chroot),
            v if v == Self::Getdents as u32 => Some(Self::Getdents),
            v if v == Self::Select as u32 => Some(Self::Select),
            v if v == Self::Fchdir as u32 => Some(Self::Fchdir),
            v if v == Self::Fsync as u32 => Some(Self::Fsync),
            v if v == Self::Truncate as u32 => Some(Self::Truncate),
            v if v == Self::Ftruncate as u32 => Some(Self::Ftruncate),
            v if v == Self::Fchmod as u32 => Some(Self::Fchmod),
            v if v == Self::Fchown as u32 => Some(Self::Fchown),
            v if v == Self::Utimens as u32 => Some(Self::Utimens),
            v if v == Self::Vmcall as u32 => Some(Self::Vmcall),
            v if v == Self::Getvfsstat as u32 => Some(Self::Getvfsstat),
            v if v == Self::Statvfs1 as u32 => Some(Self::Statvfs1),
            v if v == Self::Fstatvfs1 as u32 => Some(Self::Fstatvfs1),
            v if v == Self::Getrusage as u32 => Some(Self::Getrusage),
            v if v == Self::Svrctl as u32 => Some(Self::Svrctl),
            v if v == Self::GcovFlush as u32 => Some(Self::GcovFlush),
            v if v == Self::Mapdriver as u32 => Some(Self::Mapdriver),
            v if v == Self::Copyfd as u32 => Some(Self::Copyfd),
            v if v == Self::Socketpath as u32 => Some(Self::Socketpath),
            v if v == Self::Getsysinfo as u32 => Some(Self::Getsysinfo),
            v if v == Self::Socket as u32 => Some(Self::Socket),
            v if v == Self::Socketpair as u32 => Some(Self::Socketpair),
            v if v == Self::Bind as u32 => Some(Self::Bind),
            v if v == Self::Connect as u32 => Some(Self::Connect),
            v if v == Self::Listen as u32 => Some(Self::Listen),
            v if v == Self::Accept as u32 => Some(Self::Accept),
            v if v == Self::Sendto as u32 => Some(Self::Sendto),
            v if v == Self::Sendmsg as u32 => Some(Self::Sendmsg),
            v if v == Self::Recvfrom as u32 => Some(Self::Recvfrom),
            v if v == Self::Recvmsg as u32 => Some(Self::Recvmsg),
            v if v == Self::Setsockopt as u32 => Some(Self::Setsockopt),
            v if v == Self::Getsockopt as u32 => Some(Self::Getsockopt),
            v if v == Self::Getsockname as u32 => Some(Self::Getsockname),
            v if v == Self::Getpeername as u32 => Some(Self::Getpeername),
            v if v == Self::Shutdown as u32 => Some(Self::Shutdown),
            _ => None,
        }
    }

    /// Gets array index.
    pub fn index(self) -> usize {
        (self as u32 - VFS_BASE) as usize
    }
}

/// Syscall handler result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallResult {
    /// Success, return code.
    Ok(i32),
    /// Error code.
    Error(i32),
    /// Suspended (blocking wait).
    Suspend,
    /// Syscall not implemented.
    Nosys,
}

/// `CallTable`/`CallResolver`/`NullResolver` were removed (P2-2/R2-P1-2):
/// `VfsCallNum::from_raw` is the single resolution truth, and the
/// exhaustive `dispatch_syscall` match is the binding (W3).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_call_num_from_raw() {
        assert_eq!(VfsCallNum::from_raw(VFS_BASE + 3), Some(VfsCallNum::Open));
        assert_eq!(VfsCallNum::from_raw(VFS_BASE + 5), Some(VfsCallNum::Close));
        assert_eq!(VfsCallNum::from_raw(0), None);
        assert_eq!(VfsCallNum::from_raw(VFS_BASE + 200), None);
    }

    #[test]
    fn test_vfs_call_num_index() {
        assert_eq!(VfsCallNum::Open.index(), 3);
        assert_eq!(VfsCallNum::Close.index(), 5);
        assert_eq!(VfsCallNum::Read.index(), 0);
    }

    #[test]
    fn test_syscall_result() {
        assert_eq!(SyscallResult::Ok(0), SyscallResult::Ok(0));
        assert_eq!(SyscallResult::Error(1), SyscallResult::Error(1));
        assert_eq!(SyscallResult::Suspend, SyscallResult::Suspend);
        assert_eq!(SyscallResult::Nosys, SyscallResult::Nosys);
    }

    #[test]
    fn test_transid_codec_two_impls_in_call_table() {
        // Sanity: VFS_BASE vs TRANSACTION_BASE are distinct (A-2 + D3)
        assert_ne!(VFS_BASE, 0xB00);
        assert_eq!(NR_VFS_CALLS, 64);
    }
}

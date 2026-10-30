//! The LP64 wire layout of `struct flock` and the lock-type constants.
//!
//! C correspondence: `minix3/sys/sys/fcntl.h:253-259` (the struct),
//! `:203-205` (`F_RDLCK`/`F_UNLCK`/`F_WRLCK`). The consumers are VFS's
//! fcntl lock commands (`F_GETLK`/`F_SETLK`/`F_SETLKW`/`F_FREESP`) —
//! all four copy this struct wholesale between user memory and VFS.

/// Total LP64 size of `struct flock`: two `off_t` (8+8), `pid_t` (4), two
/// `short` (2+2), tail-padded to 8 -> 24 bytes.
pub const FLOCK_SIZE: usize = 24;

/// Field offsets of `struct flock` (LP64; the declaration order is
/// start/len/pid/type/whence, **not** the intuitive type-first order — pin
/// each cell in declaration order).
pub mod flock_off {
    /// `off_t l_start` (start of the range).
    pub const START: usize = 0;
    /// `off_t l_len` (0 = to end of file).
    pub const LEN: usize = 8;
    /// `pid_t l_pid` (lock owner; filled when GETLK reports a conflicting
    /// lock holder).
    pub const PID: usize = 16;
    /// `short l_type`.
    pub const TYPE: usize = 20;
    /// `short l_whence`.
    pub const WHENCE: usize = 22;
}

/// Shared (read) lock (`fcntl.h:203`).
pub const F_RDLCK: i32 = 1;
/// Unlock (`fcntl.h:204`).
pub const F_UNLCK: i32 = 2;
/// Exclusive (write) lock (`fcntl.h:205`).
pub const F_WRLCK: i32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the layout cell by cell (fcntl.h:253-259): one cell off and the
    /// lock type is read out as the length.
    #[test]
    fn test_flock_layout_matches_c_struct() {
        assert_eq!(FLOCK_SIZE, 24);
        assert_eq!(flock_off::START, 0);
        assert_eq!(flock_off::LEN, 8);
        assert_eq!(flock_off::PID, 16);
        assert_eq!(flock_off::TYPE, 20);
        assert_eq!(flock_off::WHENCE, 22);
    }

    /// Lock-type constants (fcntl.h:203-205).
    #[test]
    fn test_lock_type_constants() {
        assert_eq!(F_RDLCK, 1);
        assert_eq!(F_UNLCK, 2);
        assert_eq!(F_WRLCK, 3);
    }
}

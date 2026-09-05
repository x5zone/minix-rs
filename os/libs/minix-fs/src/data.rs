//! Caller data channel: copy-in, copy-out, zero, and name fetching.
//!
//! C correspondence: `minix3/minix/lib/libfsdriver/utility.c` (the three
//! copy helpers plus `fsdriver_getname`). The C helpers branch on
//! `endpt == SELF`: a local pointer is copied with `memcpy`, anything else
//! goes through a grant-based kernel call. This module replaces the branch
//! with a [`DataChannel`] value plus a [`DataBackend`] trait, so tests can
//! substitute a memory backend while production wires the kernel calls.
//!
//! Two integrity rules from the C code are preserved exactly:
//! - Every operation checks `offset + length` against the declared size and
//!   refuses to touch anything past it (`utility.c:17-18` and siblings).
//! - Peek requests pass a null channel; every operation on a null channel
//!   succeeds without doing anything (`utility.c:13-14` and siblings).

use minix_types::{EINVAL, ENAMETOOLONG, Errno};

/// Maximum file name length, including no terminator in the count.
///
/// C: `NAME_MAX` (`minix3/sys/sys/syslimits.h:57`, value 511). The request
/// carries the length *including* the terminating zero byte, so a name
/// buffer of `NAME_MAX + 1` bytes holds the longest legal name.
pub const NAME_MAX: usize = 511;

/// Transport behind a [`DataChannel`]: the two directions plus zeroing.
///
/// Production implements this with the grant-based kernel calls
/// (`sys_safecopyfrom`, `sys_safecopyto`, `sys_safememset`); tests implement
/// it over plain memory. All offsets are relative to the start of the
/// caller's buffer.
pub trait DataBackend {
    /// Copy `out.len()` bytes from the caller at `offset` into `out`.
    fn copy_from(&self, offset: usize, out: &mut [u8]) -> Result<(), Errno>;
    /// Copy `data.len()` bytes to the caller at `offset` from `data`.
    fn copy_to(&mut self, offset: usize, data: &[u8]) -> Result<(), Errno>;
    /// Zero `length` bytes at `offset` in the caller.
    fn fill_zero(&mut self, offset: usize, length: usize) -> Result<(), Errno>;
}

/// Which caller buffer a transfer targets.
///
/// C: `struct fsdriver_data` (`minix3/minix/include/minix/fsdriver.h:19-26`):
/// an endpoint plus either a grant (remote caller) or a local pointer (the
/// server itself, `endpt == SELF`), with a total size used only for the
/// integrity check.
#[derive(Debug)]
pub enum DataChannel<'a, B: DataBackend> {
    /// Peek request: there is no caller buffer. Every operation succeeds
    /// silently. C: `data == NULL`.
    Absent,
    /// Caller buffer reachable through a backend, with a declared size.
    Present {
        /// Transport to the caller buffer.
        backend: &'a mut B,
        /// Declared buffer size; only used for the integrity check.
        size: usize,
    },
}

impl<B: DataBackend> DataChannel<'_, B> {
    /// Integrity check shared by all three operations: `offset + length`
    /// must stay inside the declared size. C: `off + len > data->size`
    /// (`utility.c:17-18`, `utility.c:43-44`, `utility.c:68-69`).
    ///
    /// Overflow-safe: the addition uses checked arithmetic so a hostile
    /// offset cannot wrap around the check.
    fn check_bounds(&self, offset: usize, length: usize) -> Result<(), Errno> {
        if let DataChannel::Present { size, .. } = self {
            let end = offset.checked_add(length).ok_or(Errno::from_i32(EINVAL))?;
            if end > *size {
                return Err(Errno::from_i32(EINVAL));
            }
        }
        Ok(())
    }

    /// Copy data from the caller into local memory.
    ///
    /// C: `fsdriver_copyin` (`utility.c:7-28`). On an absent channel this
    /// does nothing and succeeds.
    pub fn copy_in(&mut self, offset: usize, out: &mut [u8]) -> Result<(), Errno> {
        self.check_bounds(offset, out.len())?;
        if let DataChannel::Present { backend, .. } = self {
            backend.copy_from(offset, out)?;
        }
        Ok(())
    }

    /// Copy data from local memory to the caller.
    ///
    /// C: `fsdriver_copyout` (`utility.c:33-54`). On an absent channel this
    /// does nothing and succeeds.
    pub fn copy_out(&mut self, offset: usize, data: &[u8]) -> Result<(), Errno> {
        self.check_bounds(offset, data.len())?;
        if let DataChannel::Present { backend, .. } = self {
            backend.copy_to(offset, data)?;
        }
        Ok(())
    }

    /// Zero a region in the caller.
    ///
    /// C: `fsdriver_zero` (`utility.c:59-78`). On an absent channel this
    /// does nothing and succeeds.
    pub fn zero(&mut self, offset: usize, length: usize) -> Result<(), Errno> {
        self.check_bounds(offset, length)?;
        if let DataChannel::Present { backend, .. } = self {
            backend.fill_zero(offset, length)?;
        }
        Ok(())
    }
}

/// Fetch and validate a null-terminated name from the caller.
///
/// C: `fsdriver_getname` (`utility.c:83-105`). The rules, in order:
/// 1. The carried length includes the terminator. Zero length is always
///    refused; length one (terminator only) is refused when the caller
///    requires a non-empty name.
/// 2. A length beyond the output buffer is "name too long".
/// 3. The bytes are copied from the caller through the backend.
/// 4. The last byte must be zero; otherwise the name is refused as invalid.
///
/// Returns the name length *without* the terminator.
pub fn fetch_name<B: DataBackend>(
    channel: &mut DataChannel<'_, B>,
    carried_length: usize,
    out: &mut [u8],
    require_non_empty: bool,
) -> Result<usize, Errno> {
    if carried_length == 0 || (require_non_empty && carried_length == 1) {
        return Err(Errno::from_i32(EINVAL));
    }
    if carried_length > out.len() {
        return Err(Errno::from_i32(ENAMETOOLONG));
    }
    channel.copy_in(0, &mut out[..carried_length])?;
    if out[carried_length - 1] != 0 {
        return Err(Errno::from_i32(EINVAL));
    }
    Ok(carried_length - 1)
}

/// In-memory backend over a caller-supplied slice.
///
/// This is the production form of the C `endpt == SELF` path
/// (`utility.c:20-24`): servers that resolve data locally (symbolic link
/// resolution, peek emulation) point the channel at their own buffer instead
/// of a grant. Tests use it the same way, which keeps both paths honest.
#[derive(Debug)]
pub struct MemoryBackend<'a> {
    /// Backing storage standing in for the caller buffer.
    pub storage: &'a mut [u8],
    /// When set, every operation fails with this code (fault injection for
    /// tests; left `None` in production).
    pub fail_with: Option<i32>,
}

impl DataBackend for MemoryBackend<'_> {
    fn copy_from(&self, offset: usize, out: &mut [u8]) -> Result<(), Errno> {
        if let Some(code) = self.fail_with {
            return Err(Errno::from_i32(code));
        }
        out.copy_from_slice(&self.storage[offset..offset + out.len()]);
        Ok(())
    }

    fn copy_to(&mut self, offset: usize, data: &[u8]) -> Result<(), Errno> {
        if let Some(code) = self.fail_with {
            return Err(Errno::from_i32(code));
        }
        self.storage[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn fill_zero(&mut self, offset: usize, length: usize) -> Result<(), Errno> {
        if let Some(code) = self.fail_with {
            return Err(Errno::from_i32(code));
        }
        self.storage[offset..offset + length].fill(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct VecBackend {
        storage: [u8; 64],
        fail_with: Option<i32>,
    }

    impl VecBackend {
        fn new() -> Self {
            Self {
                storage: [0; 64],
                fail_with: None,
            }
        }
    }

    impl DataBackend for VecBackend {
        fn copy_from(&self, offset: usize, out: &mut [u8]) -> Result<(), Errno> {
            if let Some(code) = self.fail_with {
                return Err(Errno::from_i32(code));
            }
            out.copy_from_slice(&self.storage[offset..offset + out.len()]);
            Ok(())
        }

        fn copy_to(&mut self, offset: usize, data: &[u8]) -> Result<(), Errno> {
            if let Some(code) = self.fail_with {
                return Err(Errno::from_i32(code));
            }
            self.storage[offset..offset + data.len()].copy_from_slice(data);
            Ok(())
        }

        fn fill_zero(&mut self, offset: usize, length: usize) -> Result<(), Errno> {
            if let Some(code) = self.fail_with {
                return Err(Errno::from_i32(code));
            }
            self.storage[offset..offset + length].fill(0);
            Ok(())
        }
    }

    #[test]
    fn test_absent_channel_succeeds_silently() {
        let mut channel: DataChannel<'_, VecBackend> = DataChannel::Absent;
        let mut buf = [9u8; 4];
        assert!(channel.copy_in(0, &mut buf).is_ok());
        assert!(channel.copy_out(0, &[1, 2]).is_ok());
        assert!(channel.zero(0, 100).is_ok());
        // Nothing was touched: there is no buffer at all.
        assert_eq!(buf, [9u8; 4]);
    }

    #[test]
    fn test_copy_roundtrip_through_memory() {
        let mut backend = VecBackend::new();
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: 64,
        };
        channel.copy_out(10, &[1, 2, 3, 4]).unwrap();
        let mut buf = [0u8; 4];
        channel.copy_in(10, &mut buf).unwrap();
        assert_eq!(buf, [1, 2, 3, 4]);
        channel.zero(10, 4).unwrap();
        let mut cleared = [9u8; 4];
        channel.copy_in(10, &mut cleared).unwrap();
        assert_eq!(cleared, [0u8; 4]);
    }

    #[test]
    fn test_bounds_check_rejects_overflow_and_overrun() {
        let mut backend = VecBackend::new();
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: 16,
        };
        let mut buf = [0u8; 8];
        // Past the end.
        assert_eq!(channel.copy_in(10, &mut buf).unwrap_err().to_i32(), EINVAL);
        assert_eq!(channel.copy_out(10, &[0; 8]).unwrap_err().to_i32(), EINVAL);
        assert_eq!(channel.zero(10, 8).unwrap_err().to_i32(), EINVAL);
        // Wrapping offset cannot sneak past the check.
        assert_eq!(
            channel.copy_in(usize::MAX, &mut buf).unwrap_err().to_i32(),
            EINVAL
        );
        // Exact fit is fine.
        assert!(channel.copy_in(8, &mut buf).is_ok());
    }

    #[test]
    fn test_backend_error_propagates() {
        let mut backend = VecBackend::new();
        backend.fail_with = Some(minix_types::EIO);
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: 64,
        };
        let mut buf = [0u8; 4];
        assert_eq!(
            channel.copy_in(0, &mut buf).unwrap_err().to_i32(),
            minix_types::EIO
        );
    }

    #[test]
    fn test_fetch_name_rules() {
        // Happy path: "hi" plus terminator, non-empty required.
        let mut backend = VecBackend::new();
        backend.storage[..3].copy_from_slice(b"hi\0");
        let mut channel = DataChannel::Present {
            backend: &mut backend,
            size: 64,
        };
        let mut out = [0u8; 16];
        assert_eq!(fetch_name(&mut channel, 3, &mut out, true).unwrap(), 2);
        assert_eq!(&out[..3], b"hi\0");

        // Zero length always refused.
        let mut backend = VecBackend::new();
        let mut out = [0u8; 16];
        {
            let mut channel = DataChannel::Present {
                backend: &mut backend,
                size: 64,
            };
            assert_eq!(
                fetch_name(&mut channel, 0, &mut out, false)
                    .unwrap_err()
                    .to_i32(),
                EINVAL
            );
        }
        // Bare terminator refused when non-empty is required, accepted
        // otherwise (the lookup path fetches possibly-empty paths).
        backend.storage[0] = 0;
        {
            let mut channel = DataChannel::Present {
                backend: &mut backend,
                size: 64,
            };
            assert_eq!(
                fetch_name(&mut channel, 1, &mut out, true)
                    .unwrap_err()
                    .to_i32(),
                EINVAL
            );
            assert_eq!(fetch_name(&mut channel, 1, &mut out, false).unwrap(), 0);
            // Too long for the buffer.
            assert_eq!(
                fetch_name(&mut channel, 17, &mut out, true)
                    .unwrap_err()
                    .to_i32(),
                ENAMETOOLONG
            );
        }
        // Missing terminator.
        backend.storage[..3].copy_from_slice(b"abc");
        {
            let mut channel = DataChannel::Present {
                backend: &mut backend,
                size: 64,
            };
            assert_eq!(
                fetch_name(&mut channel, 3, &mut out, true)
                    .unwrap_err()
                    .to_i32(),
                EINVAL
            );
        }
    }

    #[test]
    fn test_name_max_matches_c_header() {
        assert_eq!(NAME_MAX, 511);
    }
}

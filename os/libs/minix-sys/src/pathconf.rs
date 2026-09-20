//! Run-time configurable path variables: `pathconf` / `fpathconf`.
//!
//! C correspondence: `minix3/minix/lib/libc/sys/fpathconf.c` (the variable
//! switch: one `fstat` for `_PC_LINK_MAX`, fixed answers from
//! `<limits.h>` for the rest) and `minix3/minix/lib/libc/sys/pathconf.c`
//! (open the path `O_RDONLY`, run the same switch on the descriptor,
//! close). MINIX does not use POSIX's license to raise run-time limits —
//! the header comment in both files says so — so every variable except
//! `_PC_LINK_MAX` is a compile-time constant here.
//!
//! The wire carries nothing new: the only kernel-facing call in the whole
//! mechanism is the existing `fstat` ([`crate::vfs::fstat_via`], VFS
//! `VFS_CALL_FSTAT`). The seam below keeps that honesty testable — the
//! decision logic is exercised against an in-memory file system, and the
//! canned-wire test pins the one request the mechanism sends.
//!
//! Values pinned to the C headers: the `_PC_*` numbers ride
//! `minix3/sys/sys/unistd.h:198-206`; the limit values ride
//! `minix3/sys/sys/syslimits.h:54-68` — note `PIPE_BUF` takes the
//! `__minix` branch there (32768, line 66), not the generic 512; the
//! three POSIX option answers ride `minix3/sys/sys/unistd.h:95/:119/:159`
//! (`_POSIX_VDISABLE` is the character `\377`, i.e. 255).

use crate::ipc::IpcTransport;
use crate::vfs;
use crate::{Errno, Fd, Stat};

/// Interrogate `_PC_LINK_MAX` (`unistd.h:198`).
pub const PC_LINK_MAX: i32 = 1;
/// Interrogate `_PC_MAX_CANON` (`unistd.h:199`).
pub const PC_MAX_CANON: i32 = 2;
/// Interrogate `_PC_MAX_INPUT` (`unistd.h:200`).
pub const PC_MAX_INPUT: i32 = 3;
/// Interrogate `_PC_NAME_MAX` (`unistd.h:201`).
pub const PC_NAME_MAX: i32 = 4;
/// Interrogate `_PC_PATH_MAX` (`unistd.h:202`).
pub const PC_PATH_MAX: i32 = 5;
/// Interrogate `_PC_PIPE_BUF` (`unistd.h:203`).
pub const PC_PIPE_BUF: i32 = 6;
/// Interrogate `_PC_CHOWN_RESTRICTED` (`unistd.h:204`).
pub const PC_CHOWN_RESTRICTED: i32 = 7;
/// Interrogate `_PC_NO_TRUNC` (`unistd.h:205`).
pub const PC_NO_TRUNC: i32 = 8;
/// Interrogate `_PC_VDISABLE` (`unistd.h:206`).
pub const PC_VDISABLE: i32 = 9;

/// Max hard-link count (C: `LINK_MAX`, `syslimits.h:54`).
const LINK_MAX: i64 = 32767;
/// Max canonical terminal input line (C: `MAX_CANON`, `syslimits.h:55`).
const MAX_CANON: i64 = 255;
/// Max terminal input queue (C: `MAX_INPUT`, `syslimits.h:56`).
const MAX_INPUT: i64 = 255;
/// Max file-name bytes (C: `NAME_MAX`, `syslimits.h:57`).
const NAME_MAX: i64 = 511;
/// Max pathname bytes (C: `PATH_MAX`, `syslimits.h:64`).
const PATH_MAX: i64 = 1024;
/// Max atomic pipe write (C: `PIPE_BUF`, `syslimits.h:66` — the
/// `__minix` branch; the generic NetBSD value 512 does not apply).
const PIPE_BUF: i64 = 32768;
/// Chown restricted to superuser (C: `_POSIX_CHOWN_RESTRICTED`,
/// `unistd.h:95`).
const POSIX_CHOWN_RESTRICTED: i64 = 1;
/// Long names are truncated rather than erroring (C: `_POSIX_NO_TRUNC`,
/// `unistd.h:119`).
const POSIX_NO_TRUNC: i64 = 1;
/// Disable character for terminal flags (C: `_POSIX_VDISABLE`,
/// `unistd.h:159` — `'\377'` as `unsigned char`).
const POSIX_VDISABLE: i64 = 255;

/// File-type bits of `st_mode` (C: `S_IFMT` 0o170000, `sys/stat.h`).
const MODE_TYPE: u32 = 0o170000;
/// Directory file type (C: `S_IFDIR` 0o040000, `sys/stat.h`).
const MODE_DIR: u32 = 0o040000;

/// The one-call seam behind the variable switch.
///
/// The mechanism is libc-composed: the only syscall it can make is
/// `fstat` (`fpathconf.c:28-32`), so the seam carries exactly that face.
/// Production wires it to the VFS wire; tests substitute an in-memory
/// file system.
pub(crate) trait FpathconfSyscalls {
    /// C: `fstat(fd, &st)` — status of an open descriptor.
    fn fstat(&self, fd: Fd, out: &mut Stat) -> Result<(), Errno>;
}

/// Production seam: the status call goes out over the VFS wire.
struct WireFpathconfSyscalls<'a, T: IpcTransport> {
    transport: &'a T,
}

impl<T: IpcTransport> FpathconfSyscalls for WireFpathconfSyscalls<'_, T> {
    fn fstat(&self, fd: Fd, out: &mut Stat) -> Result<(), Errno> {
        // SAFETY: `out` lives in the caller's frame and the wire call is
        // synchronous — the library reads its own buffers, matching the C
        // `fstat` wrapper shape.
        vfs::fstat_via(self.transport, fd, out as *mut Stat as u64)
    }
}

/// The variable switch (`fpathconf.c:24-58`).
///
/// `_PC_LINK_MAX` stats the descriptor first and answers 1 for a
/// directory (C `fpathconf.c:29-35`: "no links to directories"); a
/// failed stat propagates its errno (the C -1/errno pair). Every other
/// variable is a header constant. An unknown variable is `EINVAL`
/// (`fpathconf.c:55-57`).
fn fpathconf_with(sys: &impl FpathconfSyscalls, fd: Fd, name: i32) -> Result<i64, Errno> {
    match name {
        PC_LINK_MAX => {
            let mut st: Stat = unsafe { core::mem::zeroed() };
            sys.fstat(fd, &mut st)?;
            if st.st_mode & MODE_TYPE == MODE_DIR {
                Ok(1)
            } else {
                Ok(LINK_MAX)
            }
        }
        PC_MAX_CANON => Ok(MAX_CANON),
        PC_MAX_INPUT => Ok(MAX_INPUT),
        PC_NAME_MAX => Ok(NAME_MAX),
        PC_PATH_MAX => Ok(PATH_MAX),
        PC_PIPE_BUF => Ok(PIPE_BUF),
        PC_CHOWN_RESTRICTED => Ok(POSIX_CHOWN_RESTRICTED),
        PC_NO_TRUNC => Ok(POSIX_NO_TRUNC),
        PC_VDISABLE => Ok(POSIX_VDISABLE),
        _ => Err(Errno::EINVAL),
    }
}

/// `fpathconf` over an explicit transport (C: `fpathconf`).
pub fn fpathconf_via<T: IpcTransport>(transport: &T, fd: Fd, name: i32) -> Result<i64, Errno> {
    fpathconf_with(&WireFpathconfSyscalls { transport }, fd, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory file system: one descriptor classed as directory or
    /// file, one optional failure for the stat itself.
    struct FakeFs {
        dir: bool,
        stat_errno: Option<Errno>,
    }

    impl FakeFs {
        fn regular() -> Self {
            Self {
                dir: false,
                stat_errno: None,
            }
        }
        fn directory() -> Self {
            Self {
                dir: true,
                stat_errno: None,
            }
        }
    }

    impl FpathconfSyscalls for FakeFs {
        fn fstat(&self, _fd: Fd, out: &mut Stat) -> Result<(), Errno> {
            if let Some(errno) = self.stat_errno {
                return Err(errno);
            }
            out.st_mode = if self.dir { MODE_DIR } else { 0o100644 };
            Ok(())
        }
    }

    /// `_PC_LINK_MAX` stats the descriptor: a directory answers 1, a
    /// regular file answers `LINK_MAX` 32767 (`fpathconf.c:29-35`).
    #[test]
    fn test_link_max_directories_answer_one_files_answer_limit() {
        let fd: Fd = 4;
        assert_eq!(fpathconf_with(&FakeFs::directory(), fd, PC_LINK_MAX), Ok(1));
        assert_eq!(
            fpathconf_with(&FakeFs::regular(), fd, PC_LINK_MAX),
            Ok(32767)
        );
        // A failed stat propagates (C `fpathconf.c:31` returns -1 with
        // the errno set; here the errno rides the Err).
        let broken = FakeFs {
            stat_errno: Some(Errno::ENOENT),
            dir: false,
        };
        assert_eq!(fpathconf_with(&broken, fd, PC_LINK_MAX), Err(Errno::ENOENT));
    }

    /// Every non-stat variable is the pinned header constant, and the
    /// decision never touches the descriptor (`fpathconf.c:37-53`).
    #[test]
    fn test_constant_variables_match_c_headers() {
        let fd: Fd = 0; // never stat-ted; any value behaves the same
        let sys = FakeFs {
            stat_errno: Some(Errno::EIO),
            dir: false,
        };
        assert_eq!(fpathconf_with(&sys, fd, PC_MAX_CANON), Ok(255));
        assert_eq!(fpathconf_with(&sys, fd, PC_MAX_INPUT), Ok(255));
        assert_eq!(fpathconf_with(&sys, fd, PC_NAME_MAX), Ok(511));
        assert_eq!(fpathconf_with(&sys, fd, PC_PATH_MAX), Ok(1024));
        // The __minix branch of `syslimits.h:66-68` — 32768, not 512.
        assert_eq!(fpathconf_with(&sys, fd, PC_PIPE_BUF), Ok(32768));
        assert_eq!(fpathconf_with(&sys, fd, PC_CHOWN_RESTRICTED), Ok(1));
        assert_eq!(fpathconf_with(&sys, fd, PC_NO_TRUNC), Ok(1));
        // `unistd.h:159`: the disable character `\377`.
        assert_eq!(fpathconf_with(&sys, fd, PC_VDISABLE), Ok(255));
    }

    /// Unknown variables refuse with EINVAL and never reach the seam
    /// (`fpathconf.c:55-57`).
    #[test]
    fn test_unknown_variable_is_einval() {
        let sys = FakeFs::regular();
        assert_eq!(fpathconf_with(&sys, 3, 0), Err(Errno::EINVAL));
        assert_eq!(fpathconf_with(&sys, 3, 10), Err(Errno::EINVAL));
        assert_eq!(fpathconf_with(&sys, 3, -1), Err(Errno::EINVAL));
    }

    /// The `_PC_*` argument numbers themselves (`unistd.h:198-206`) — a
    /// drifted number would silently interrogate the wrong variable.
    #[test]
    fn test_variable_numbers_match_c() {
        assert_eq!(PC_LINK_MAX, 1);
        assert_eq!(PC_MAX_CANON, 2);
        assert_eq!(PC_MAX_INPUT, 3);
        assert_eq!(PC_NAME_MAX, 4);
        assert_eq!(PC_PATH_MAX, 5);
        assert_eq!(PC_PIPE_BUF, 6);
        assert_eq!(PC_CHOWN_RESTRICTED, 7);
        assert_eq!(PC_NO_TRUNC, 8);
        assert_eq!(PC_VDISABLE, 9);
    }
}

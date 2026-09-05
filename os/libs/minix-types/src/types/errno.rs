//! POSIX errno constants.
//!
//! Corresponds to Minix3's `<sys/errno.h>`.
//!
//! # Notes
//!
//! These values follow Minix3's errno numbering, which differs from Linux
//! in some cases (e.g., `ENOSYS = 78` in Minix3 vs `38` in Linux).

/// Minix3 成功码。
///
/// 对应 minix3/sys/sys/errno.h:190 `#define OK 0`。PM 用 `OK` 作为回复码。
pub const OK: i32 = 0;

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const ESRCH: i32 = 3;
pub const EINTR: i32 = 4;
pub const EIO: i32 = 5;
pub const ENXIO: i32 = 6;
pub const E2BIG: i32 = 7;
pub const ENOEXEC: i32 = 8;
pub const EBADF: i32 = 9;
pub const ECHILD: i32 = 10;
pub const EDEADLK: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EACCES: i32 = 13;
pub const EFAULT: i32 = 14;
pub const ENOTBLK: i32 = 15;
pub const EBUSY: i32 = 16;
pub const EEXIST: i32 = 17;
pub const EXDEV: i32 = 18;
pub const ENODEV: i32 = 19;
pub const ENOTDIR: i32 = 20;
pub const EISDIR: i32 = 21;
pub const EINVAL: i32 = 22;
pub const ENFILE: i32 = 23;
pub const EMFILE: i32 = 24;
pub const ENOTTY: i32 = 25;
pub const ETXTBSY: i32 = 26;
pub const EFBIG: i32 = 27;
pub const ENOSPC: i32 = 28;
pub const ESPIPE: i32 = 29;
pub const EROFS: i32 = 30;
pub const EMLINK: i32 = 31;
pub const EPIPE: i32 = 32;
pub const EDOM: i32 = 33;
pub const ERANGE: i32 = 34;
pub const EAGAIN: i32 = 35;
pub const EINPROGRESS: i32 = 36;
pub const EALREADY: i32 = 37;
pub const ENOTSOCK: i32 = 38;
pub const EDESTADDRREQ: i32 = 39;
pub const EMSGSIZE: i32 = 40;
pub const EPROTOTYPE: i32 = 41;
pub const ENOPROTOOPT: i32 = 42;
pub const EPROTONOSUPPORT: i32 = 43;
pub const ESOCKTNOSUPPORT: i32 = 44;
pub const EOPNOTSUPP: i32 = 45;
pub const EPFNOSUPPORT: i32 = 46;
pub const EAFNOSUPPORT: i32 = 47;
pub const EADDRINUSE: i32 = 48;
pub const EADDRNOTAVAIL: i32 = 49;
pub const ENETDOWN: i32 = 50;
pub const ENETUNREACH: i32 = 51;
pub const ENETRESET: i32 = 52;
pub const ECONNABORTED: i32 = 53;
pub const ECONNRESET: i32 = 54;
pub const ENOBUFS: i32 = 55;
pub const EISCONN: i32 = 56;
pub const ENOTCONN: i32 = 57;
pub const ESHUTDOWN: i32 = 58;
pub const ETOOMANYREFS: i32 = 59;
pub const ETIMEDOUT: i32 = 60;
pub const ECONNREFUSED: i32 = 61;
pub const ELOOP: i32 = 62;
pub const ENAMETOOLONG: i32 = 63;
pub const EHOSTDOWN: i32 = 64;
pub const EHOSTUNREACH: i32 = 65;
pub const ENOTEMPTY: i32 = 66;
pub const EPROCLIM: i32 = 67;
pub const EUSERS: i32 = 68;
pub const EDQUOT: i32 = 69;
pub const ESTALE: i32 = 70;
pub const EREMOTE: i32 = 71;
pub const EBADRPC: i32 = 72;
pub const ERPCMISMATCH: i32 = 73;
pub const EPROGUNAVAIL: i32 = 74;
pub const EPROGMISMATCH: i32 = 75;
pub const EPROCUNAVAIL: i32 = 76;
pub const ENOLCK: i32 = 77;
pub const ENOSYS: i32 = 78;

/// Pseudo-code: don't send a reply. C: `EDONTREPLY` — sys/errno.h:199.
/// Not a real errno: handlers return it to suppress the main-loop reply
/// (main.c:124-129, 06-rs-main-loop.md).
/// Service restarted. C: `ERESTART` — sys/errno.h:196 (`_SYSTEM` 下为 -200；
/// minix-types 采用用户态正数约定，见 03-stage-rs/99-rs-global-concepts.md §errno 符号约定).
/// RS 用它作 `r_init_err` 的默认值（manager.c:1828，clone_slot）。
pub const ERESTART: i32 = 200;
/// Source or destination is not ready. C: `ENOTREADY` — sys/errno.h:197.
/// The kernel answers a system call with this status when the other end is
/// not ready yet; `_kernel_call` (minix/lib/libsys/kernel_call.c) retries the
/// call with a growing delay instead of failing. Like the other 200-range
/// codes, minix-types uses the positive user-space convention.
pub const ENOTREADY: i32 = 201;
/// Source or destination is not alive. C: `EDEADSRCDST` — sys/errno.h:198.
pub const EDEADSRCDST: i32 = 202;
pub const EDONTREPLY: i32 = 203;
/// Generic error. C: `EGENERIC` — sys/errno.h:200.
pub const EGENERIC: i32 = 204;
/// Invalid packet size for some protocol. C: `EPACKSIZE` — sys/errno.h:201.
pub const EPACKSIZE: i32 = 205;
/// Urgent data present. C: `EURG` — sys/errno.h:202.
pub const EURG: i32 = 206;
/// No urgent data present. C: `ENOURG` — sys/errno.h:203.
pub const ENOURG: i32 = 207;
/// Can't send message due to deadlock. C: `ELOCKED` — sys/errno.h:204.
pub const ELOCKED: i32 = 208;
/// Illegal system call number. C: `EBADCALL` — sys/errno.h:205.
pub const EBADCALL: i32 = 209;
/// No permission for system call. C: `ECALLDENIED` — sys/errno.h:206.
pub const ECALLDENIED: i32 = 210;
/// IPC trap not allowed. C: `ETRAPDENIED` — sys/errno.h:207.
pub const ETRAPDENIED: i32 = 211;
/// Destination cannot handle request. C: `EBADREQUEST` — sys/errno.h:208.
pub const EBADREQUEST: i32 = 212;
/// Bad mode in ioctl. C: `EBADMODE` — sys/errno.h:209.
pub const EBADMODE: i32 = 213;
/// No such connection. C: `ENOCONN` — sys/errno.h:210.
pub const ENOCONN: i32 = 214;
/// Specified endpoint is not alive. C: `EDEADEPT` — sys/errno.h:211.
pub const EDEADEPT: i32 = 215;
/// Specified endpoint is bad (a task, not a process). C: `EBADEPT` — sys/errno.h:212.
pub const EBADEPT: i32 = 216;
/// Requested CPU does not work. C: `EBADCPU` — sys/errno.h:213.
pub const EBADCPU: i32 = 217;
pub const EFTYPE: i32 = 79;
pub const EAUTH: i32 = 80;
pub const ENEEDAUTH: i32 = 81;
pub const EIDRM: i32 = 82;
pub const ENOMSG: i32 = 83;
pub const EOVERFLOW: i32 = 84;
pub const EILSEQ: i32 = 85;
pub const ENOTSUP: i32 = 86;
/// Operation canceled. C: `ECANCELED` — sys/errno.h:156.
pub const ECANCELED: i32 = 87;
/// Bad or corrupt message. C: `EBADMSG` — sys/errno.h:159.
pub const EBADMSG: i32 = 88;
/// No message available. C: `ENODATA` — sys/errno.h:162.
pub const ENODATA: i32 = 89;
/// No STREAM resources. C: `ENOSR` — sys/errno.h:163.
pub const ENOSR: i32 = 90;
/// Not a STREAM. C: `ENOSTR` — sys/errno.h:164.
pub const ENOSTR: i32 = 91;
/// STREAM ioctl timeout. C: `ETIME` — sys/errno.h:165.
pub const ETIME: i32 = 92;
/// Attribute not found. C: `ENOATTR` — sys/errno.h:168.
pub const ENOATTR: i32 = 93;
/// Multihop attempted. C: `EMULTIHOP` — sys/errno.h:171.
pub const EMULTIHOP: i32 = 94;
/// Link has been severed. C: `ENOLINK` — sys/errno.h:172.
pub const ENOLINK: i32 = 95;
/// Protocol error. C: `EPROTO` — sys/errno.h:173.
pub const EPROTO: i32 = 96;
/// Must equal largest errno. C: `ELAST` — sys/errno.h:175 (same value as `EPROTO`).
pub const ELAST: i32 = 96;

/// Type-safe errno value (ARCH A-12, 03-stage-rs/99-rs-global-concepts.md).
///
/// Wraps the Minix3 errno number under the user-space positive convention
/// (the same values as the module-level `i32` constants, which remain the
/// wire form). Convert at the message boundary with [`Errno::to_i32`] /
/// [`Errno::from_i32`] — the Redox `mux/demux` pattern
/// (`Ok(v) => v, Err(e) => e.to_i32()`).
///
/// The RS server is the first consumer (A-12); the constant set grows as
/// other crates convert from bare `i32` errno. `EDONTREPLY` is included
/// because RS uses it as an internal reply-suppression sentinel
/// (main.c:124-129), not as a wire errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Errno(i32);

impl Errno {
    /// C: `EPERM` — sys/errno.h:1.
    pub const EPERM: Errno = Errno(EPERM);
    /// C: `E2BIG` — sys/errno.h:7.
    pub const E2BIG: Errno = Errno(E2BIG);
    /// C: `ENOEXEC` — sys/errno.h:8.
    pub const ENOEXEC: Errno = Errno(ENOEXEC);
    /// C: `ESRCH` — sys/errno.h:3.
    pub const ESRCH: Errno = Errno(ESRCH);
    /// C: `EBUSY` — sys/errno.h:16.
    pub const EBUSY: Errno = Errno(EBUSY);
    /// C: `EINVAL` — sys/errno.h:22.
    pub const EINVAL: Errno = Errno(EINVAL);
    /// C: `ENOENT` — sys/errno.h:2.
    pub const ENOENT: Errno = Errno(ENOENT);
    /// C: `EIO` — sys/errno.h:5.
    pub const EIO: Errno = Errno(EIO);
    /// C: `EEXIST` — sys/errno.h:17.
    pub const EEXIST: Errno = Errno(EEXIST);
    /// C: `ENAMETOOLONG` — sys/errno.h:63.
    pub const ENAMETOOLONG: Errno = Errno(ENAMETOOLONG);
    /// C: `ENOTDIR` — sys/errno.h:20.
    pub const ENOTDIR: Errno = Errno(ENOTDIR);
    /// C: `ENODEV` — sys/errno.h:19.
    pub const ENODEV: Errno = Errno(ENODEV);
    /// C: `ENOMEM` — sys/errno.h:12.
    pub const ENOMEM: Errno = Errno(ENOMEM);
    /// C: `ENOSYS` — sys/errno.h:78.
    pub const ENOSYS: Errno = Errno(ENOSYS);
    /// C: `ERESTART` — sys/errno.h:196 (positive user-space convention).
    pub const ERESTART: Errno = Errno(ERESTART);
    /// C: `ENOTREADY` — sys/errno.h:197 (positive user-space convention).
    pub const ENOTREADY: Errno = Errno(ENOTREADY);
    /// C: `EDONTREPLY` — sys/errno.h:199 (reply-suppression sentinel).
    pub const EDONTREPLY: Errno = Errno(EDONTREPLY);
    /// C: `EGENERIC` — sys/errno.h:200.
    pub const EGENERIC: Errno = Errno(EGENERIC);
    /// C: `EDEADEPT` — sys/errno.h:211.
    pub const EDEADEPT: Errno = Errno(EDEADEPT);
    /// C: `EBADEPT` — sys/errno.h:212.
    pub const EBADEPT: Errno = Errno(EBADEPT);
    /// C: `EBADCPU` — sys/errno.h:213.
    pub const EBADCPU: Errno = Errno(EBADCPU);

    /// Wraps a raw errno value (wire form).
    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }

    /// The raw errno value (wire form).
    pub const fn to_i32(self) -> i32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_errno_values_match_c() {
        // C: sys/errno.h — values are shared with the i32 constants.
        assert_eq!(Errno::EPERM.to_i32(), EPERM);
        assert_eq!(Errno::EINVAL.to_i32(), EINVAL);
        assert_eq!(Errno::ENOSYS.to_i32(), ENOSYS);
        assert_eq!(Errno::EDEADEPT.to_i32(), EDEADEPT);
        assert_eq!(Errno::EBADEPT.to_i32(), EBADEPT);
        assert_eq!(EBADEPT, 216);
        assert_eq!(Errno::EDONTREPLY.to_i32(), EDONTREPLY);
        assert_eq!(Errno::EDEADEPT.to_i32(), EDEADEPT);
    }

    #[test]
    fn test_errno_roundtrip() {
        assert_eq!(Errno::from_i32(22).to_i32(), 22);
        assert_eq!(Errno::from_i32(78), Errno::ENOSYS);
        assert_eq!(Errno::from_i32(201), Errno::ENOTREADY);
        assert_eq!(ENOTREADY, 201);
    }
}

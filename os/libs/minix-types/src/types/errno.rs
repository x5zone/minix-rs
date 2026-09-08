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
    /// C: `EAGAIN` — sys/errno.h:11.
    pub const EAGAIN: Errno = Errno(EAGAIN);
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
    /// C: `EFAULT` — sys/errno.h:14.
    pub const EFAULT: Errno = Errno(EFAULT);
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

    /// The C error-name face — the `strerror` equivalent as a static
    /// identifier ("EPERM"), or `None` for a value without a named
    /// constant.
    ///
    /// E-8: RS's diagnostic面 (error.c `rs_strerror`/`init_strerror`/
    /// `lu_strerror`) layers its contextual descriptions on top of this
    /// table instead of keeping a second errno→name map; the match arms
    /// use the constants themselves, so a value rename or removal breaks
    /// compilation here rather than drifting.
    pub fn name(self) -> Option<&'static str> {
        match self.0 {
            EPERM => Some("EPERM"),
            ENOENT => Some("ENOENT"),
            ESRCH => Some("ESRCH"),
            EINTR => Some("EINTR"),
            EIO => Some("EIO"),
            ENXIO => Some("ENXIO"),
            E2BIG => Some("E2BIG"),
            ENOEXEC => Some("ENOEXEC"),
            EBADF => Some("EBADF"),
            ECHILD => Some("ECHILD"),
            EDEADLK => Some("EDEADLK"),
            ENOMEM => Some("ENOMEM"),
            EACCES => Some("EACCES"),
            EFAULT => Some("EFAULT"),
            ENOTBLK => Some("ENOTBLK"),
            EBUSY => Some("EBUSY"),
            EEXIST => Some("EEXIST"),
            EXDEV => Some("EXDEV"),
            ENODEV => Some("ENODEV"),
            ENOTDIR => Some("ENOTDIR"),
            EISDIR => Some("EISDIR"),
            EINVAL => Some("EINVAL"),
            ENFILE => Some("ENFILE"),
            EMFILE => Some("EMFILE"),
            ENOTTY => Some("ENOTTY"),
            ETXTBSY => Some("ETXTBSY"),
            EFBIG => Some("EFBIG"),
            ENOSPC => Some("ENOSPC"),
            ESPIPE => Some("ESPIPE"),
            EROFS => Some("EROFS"),
            EMLINK => Some("EMLINK"),
            EPIPE => Some("EPIPE"),
            EDOM => Some("EDOM"),
            ERANGE => Some("ERANGE"),
            EAGAIN => Some("EAGAIN"),
            EINPROGRESS => Some("EINPROGRESS"),
            EALREADY => Some("EALREADY"),
            ENOTSOCK => Some("ENOTSOCK"),
            EDESTADDRREQ => Some("EDESTADDRREQ"),
            EMSGSIZE => Some("EMSGSIZE"),
            EPROTOTYPE => Some("EPROTOTYPE"),
            ENOPROTOOPT => Some("ENOPROTOOPT"),
            EPROTONOSUPPORT => Some("EPROTONOSUPPORT"),
            ESOCKTNOSUPPORT => Some("ESOCKTNOSUPPORT"),
            EOPNOTSUPP => Some("EOPNOTSUPP"),
            EPFNOSUPPORT => Some("EPFNOSUPPORT"),
            EAFNOSUPPORT => Some("EAFNOSUPPORT"),
            EADDRINUSE => Some("EADDRINUSE"),
            EADDRNOTAVAIL => Some("EADDRNOTAVAIL"),
            ENETDOWN => Some("ENETDOWN"),
            ENETUNREACH => Some("ENETUNREACH"),
            ENETRESET => Some("ENETRESET"),
            ECONNABORTED => Some("ECONNABORTED"),
            ECONNRESET => Some("ECONNRESET"),
            ENOBUFS => Some("ENOBUFS"),
            EISCONN => Some("EISCONN"),
            ENOTCONN => Some("ENOTCONN"),
            ESHUTDOWN => Some("ESHUTDOWN"),
            ETOOMANYREFS => Some("ETOOMANYREFS"),
            ETIMEDOUT => Some("ETIMEDOUT"),
            ECONNREFUSED => Some("ECONNREFUSED"),
            ELOOP => Some("ELOOP"),
            ENAMETOOLONG => Some("ENAMETOOLONG"),
            EHOSTDOWN => Some("EHOSTDOWN"),
            EHOSTUNREACH => Some("EHOSTUNREACH"),
            ENOTEMPTY => Some("ENOTEMPTY"),
            EPROCLIM => Some("EPROCLIM"),
            EUSERS => Some("EUSERS"),
            EDQUOT => Some("EDQUOT"),
            ESTALE => Some("ESTALE"),
            EREMOTE => Some("EREMOTE"),
            EBADRPC => Some("EBADRPC"),
            ERPCMISMATCH => Some("ERPCMISMATCH"),
            EPROGUNAVAIL => Some("EPROGUNAVAIL"),
            EPROGMISMATCH => Some("EPROGMISMATCH"),
            EPROCUNAVAIL => Some("EPROCUNAVAIL"),
            ENOLCK => Some("ENOLCK"),
            ENOSYS => Some("ENOSYS"),
            ERESTART => Some("ERESTART"),
            ENOTREADY => Some("ENOTREADY"),
            EDEADSRCDST => Some("EDEADSRCDST"),
            EDONTREPLY => Some("EDONTREPLY"),
            EGENERIC => Some("EGENERIC"),
            EPACKSIZE => Some("EPACKSIZE"),
            EURG => Some("EURG"),
            ENOURG => Some("ENOURG"),
            ELOCKED => Some("ELOCKED"),
            EBADCALL => Some("EBADCALL"),
            ECALLDENIED => Some("ECALLDENIED"),
            ETRAPDENIED => Some("ETRAPDENIED"),
            EBADREQUEST => Some("EBADREQUEST"),
            EBADMODE => Some("EBADMODE"),
            ENOCONN => Some("ENOCONN"),
            EDEADEPT => Some("EDEADEPT"),
            EBADEPT => Some("EBADEPT"),
            EBADCPU => Some("EBADCPU"),
            EFTYPE => Some("EFTYPE"),
            EAUTH => Some("EAUTH"),
            ENEEDAUTH => Some("ENEEDAUTH"),
            EIDRM => Some("EIDRM"),
            ENOMSG => Some("ENOMSG"),
            EOVERFLOW => Some("EOVERFLOW"),
            EILSEQ => Some("EILSEQ"),
            ENOTSUP => Some("ENOTSUP"),
            ECANCELED => Some("ECANCELED"),
            EBADMSG => Some("EBADMSG"),
            ENODATA => Some("ENODATA"),
            ENOSR => Some("ENOSR"),
            ENOSTR => Some("ENOSTR"),
            ETIME => Some("ETIME"),
            ENOATTR => Some("ENOATTR"),
            EMULTIHOP => Some("EMULTIHOP"),
            ENOLINK => Some("ENOLINK"),
            EPROTO => Some("EPROTO"),
            _ => None,
        }
    }
}

/// The C `strerror` face of an errno: the identifier ("EPERM") for known
/// values, `Errno(N)` otherwise. The RS server's contextual descriptions
/// (init/lu_strerror — error.c:48/:56) compose on top of this.
impl core::fmt::Display for Errno {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            EPERM => f.write_str("EPERM"),
            ENOENT => f.write_str("ENOENT"),
            ESRCH => f.write_str("ESRCH"),
            EINTR => f.write_str("EINTR"),
            EIO => f.write_str("EIO"),
            ENXIO => f.write_str("ENXIO"),
            E2BIG => f.write_str("E2BIG"),
            ENOEXEC => f.write_str("ENOEXEC"),
            EBADF => f.write_str("EBADF"),
            ECHILD => f.write_str("ECHILD"),
            EDEADLK => f.write_str("EDEADLK"),
            ENOMEM => f.write_str("ENOMEM"),
            EACCES => f.write_str("EACCES"),
            EFAULT => f.write_str("EFAULT"),
            ENOTBLK => f.write_str("ENOTBLK"),
            EBUSY => f.write_str("EBUSY"),
            EEXIST => f.write_str("EEXIST"),
            EXDEV => f.write_str("EXDEV"),
            ENODEV => f.write_str("ENODEV"),
            ENOTDIR => f.write_str("ENOTDIR"),
            EISDIR => f.write_str("EISDIR"),
            EINVAL => f.write_str("EINVAL"),
            ENFILE => f.write_str("ENFILE"),
            EMFILE => f.write_str("EMFILE"),
            ENOTTY => f.write_str("ENOTTY"),
            ETXTBSY => f.write_str("ETXTBSY"),
            EFBIG => f.write_str("EFBIG"),
            ENOSPC => f.write_str("ENOSPC"),
            ESPIPE => f.write_str("ESPIPE"),
            EROFS => f.write_str("EROFS"),
            EMLINK => f.write_str("EMLINK"),
            EPIPE => f.write_str("EPIPE"),
            EDOM => f.write_str("EDOM"),
            ERANGE => f.write_str("ERANGE"),
            EAGAIN => f.write_str("EAGAIN"),
            EINPROGRESS => f.write_str("EINPROGRESS"),
            EALREADY => f.write_str("EALREADY"),
            ENOTSOCK => f.write_str("ENOTSOCK"),
            EDESTADDRREQ => f.write_str("EDESTADDRREQ"),
            EMSGSIZE => f.write_str("EMSGSIZE"),
            EPROTOTYPE => f.write_str("EPROTOTYPE"),
            ENOPROTOOPT => f.write_str("ENOPROTOOPT"),
            EPROTONOSUPPORT => f.write_str("EPROTONOSUPPORT"),
            ESOCKTNOSUPPORT => f.write_str("ESOCKTNOSUPPORT"),
            EOPNOTSUPP => f.write_str("EOPNOTSUPP"),
            EPFNOSUPPORT => f.write_str("EPFNOSUPPORT"),
            EAFNOSUPPORT => f.write_str("EAFNOSUPPORT"),
            EADDRINUSE => f.write_str("EADDRINUSE"),
            EADDRNOTAVAIL => f.write_str("EADDRNOTAVAIL"),
            ENETDOWN => f.write_str("ENETDOWN"),
            ENETUNREACH => f.write_str("ENETUNREACH"),
            ENETRESET => f.write_str("ENETRESET"),
            ECONNABORTED => f.write_str("ECONNABORTED"),
            ECONNRESET => f.write_str("ECONNRESET"),
            ENOBUFS => f.write_str("ENOBUFS"),
            EISCONN => f.write_str("EISCONN"),
            ENOTCONN => f.write_str("ENOTCONN"),
            ESHUTDOWN => f.write_str("ESHUTDOWN"),
            ETOOMANYREFS => f.write_str("ETOOMANYREFS"),
            ETIMEDOUT => f.write_str("ETIMEDOUT"),
            ECONNREFUSED => f.write_str("ECONNREFUSED"),
            ELOOP => f.write_str("ELOOP"),
            ENAMETOOLONG => f.write_str("ENAMETOOLONG"),
            EHOSTDOWN => f.write_str("EHOSTDOWN"),
            EHOSTUNREACH => f.write_str("EHOSTUNREACH"),
            ENOTEMPTY => f.write_str("ENOTEMPTY"),
            EPROCLIM => f.write_str("EPROCLIM"),
            EUSERS => f.write_str("EUSERS"),
            EDQUOT => f.write_str("EDQUOT"),
            ESTALE => f.write_str("ESTALE"),
            EREMOTE => f.write_str("EREMOTE"),
            EBADRPC => f.write_str("EBADRPC"),
            ERPCMISMATCH => f.write_str("ERPCMISMATCH"),
            EPROGUNAVAIL => f.write_str("EPROGUNAVAIL"),
            EPROGMISMATCH => f.write_str("EPROGMISMATCH"),
            EPROCUNAVAIL => f.write_str("EPROCUNAVAIL"),
            ENOLCK => f.write_str("ENOLCK"),
            ENOSYS => f.write_str("ENOSYS"),
            ERESTART => f.write_str("ERESTART"),
            ENOTREADY => f.write_str("ENOTREADY"),
            EDEADSRCDST => f.write_str("EDEADSRCDST"),
            EDONTREPLY => f.write_str("EDONTREPLY"),
            EGENERIC => f.write_str("EGENERIC"),
            EPACKSIZE => f.write_str("EPACKSIZE"),
            EURG => f.write_str("EURG"),
            ENOURG => f.write_str("ENOURG"),
            ELOCKED => f.write_str("ELOCKED"),
            EBADCALL => f.write_str("EBADCALL"),
            ECALLDENIED => f.write_str("ECALLDENIED"),
            ETRAPDENIED => f.write_str("ETRAPDENIED"),
            EBADREQUEST => f.write_str("EBADREQUEST"),
            EBADMODE => f.write_str("EBADMODE"),
            ENOCONN => f.write_str("ENOCONN"),
            EDEADEPT => f.write_str("EDEADEPT"),
            EBADEPT => f.write_str("EBADEPT"),
            EBADCPU => f.write_str("EBADCPU"),
            EFTYPE => f.write_str("EFTYPE"),
            EAUTH => f.write_str("EAUTH"),
            ENEEDAUTH => f.write_str("ENEEDAUTH"),
            EIDRM => f.write_str("EIDRM"),
            ENOMSG => f.write_str("ENOMSG"),
            EOVERFLOW => f.write_str("EOVERFLOW"),
            EILSEQ => f.write_str("EILSEQ"),
            ENOTSUP => f.write_str("ENOTSUP"),
            ECANCELED => f.write_str("ECANCELED"),
            EBADMSG => f.write_str("EBADMSG"),
            ENODATA => f.write_str("ENODATA"),
            ENOSR => f.write_str("ENOSR"),
            ENOSTR => f.write_str("ENOSTR"),
            ETIME => f.write_str("ETIME"),
            ENOATTR => f.write_str("ENOATTR"),
            EMULTIHOP => f.write_str("EMULTIHOP"),
            ENOLINK => f.write_str("ENOLINK"),
            EPROTO => f.write_str("EPROTO"),
            other => write!(f, "Errno({other})"),
        }
    }
}


/// D2 (todo §3): unified "error type → errno" mapping channel.
///
/// Each crate's local error enum implements this trait so the mapping
/// logic lives on the error type itself, replacing scattered hand-written
/// `xxx_error_to_errno` free functions. Consumers convert via
/// `err.to_errno().to_i32()` when a raw `i32` reply is needed.
///
/// The inherent `to_errno(&self) -> i32` methods that already exist on
/// some enums keep working (compat); new trait impls delegate to them.
pub trait ToErrno {
    fn to_errno(&self) -> Errno;
}

#[cfg(test)]
mod to_errno_trait_tests {
    use super::*;

    // PmError / KernelError 的 trait 委托与既有固有方法一致性测试。
    #[test]
    fn test_to_errno_trait_delegates_for_pm_error() {
        use crate::ipc::PmError;
        fn assert_te<T: ToErrno>(e: &T, want: Errno) {
            assert_eq!(e.to_errno(), want);
        }
        assert_te(&PmError::ProcTableFull, Errno::EAGAIN);
        assert_te(&PmError::OutOfMemory, Errno::ENOMEM);
        assert_te(&PmError::InvalidEndpoint, Errno::ESRCH);
        assert_te(&PmError::NotImplemented, Errno::ENOSYS);
    }

    #[test]
    fn test_to_errno_trait_delegates_for_kernel_error() {
        use crate::ipc::KernelError;
        fn assert_te<T: ToErrno>(e: &T, want: Errno) {
            assert_eq!(e.to_errno(), want);
        }
        assert_te(&KernelError::SlotInUse, Errno::EINVAL);
        assert_te(&KernelError::InternalError, Errno::EIO);
        assert_te(&KernelError::NotImplemented, Errno::ENOSYS);
    }

    #[test]
    fn test_to_errno_ebadcpu_associated_const() {
        // SchedProcError::BadCpu 的映射目标（kernel 侧 impl 使用）。
        assert_eq!(Errno::EBADCPU.to_i32(), EBADCPU);
        assert_eq!(EBADCPU, 217);
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

    #[test]
    fn test_errno_display_and_name() {
        // E-8: the strerror face — known values print the C identifier,
        // unknown values degrade to `Errno(N)`, and name()/Display agree.
        extern crate alloc;
        use alloc::string::ToString as _;
        assert_eq!(Errno::ENOSYS.to_string(), "ENOSYS");
        assert_eq!(Errno::EINVAL.to_string(), "EINVAL");
        assert_eq!(Errno::ERESTART.to_string(), "ERESTART");
        assert_eq!(Errno::EGENERIC.to_string(), "EGENERIC");
        assert_eq!(Errno::EDEADEPT.to_string(), "EDEADEPT");
        assert_eq!(Errno::from_i32(9999).to_string(), "Errno(9999)");
        for e in [
            Errno::EPERM,
            Errno::EINVAL,
            Errno::ENOSYS,
            Errno::ERESTART,
            Errno::EDONTREPLY,
        ] {
            assert_eq!(e.name(), Some(e.to_string().as_str()));
        }
        assert_eq!(Errno::from_i32(9999).name(), None);
    }
}

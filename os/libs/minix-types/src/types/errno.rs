//! POSIX errno constants.
//!
//! Corresponds to Minix3's `<sys/errno.h>`.
//!
//! # Notes
//!
//! These values follow Minix3's errno numbering, which differs from Linux
//! in some cases (e.g., `ENOSYS = 78` in Minix3 vs `38` in Linux).

/// Minix3 success code.
///
/// Corresponds to minix3/sys/sys/errno.h:190 `#define OK 0`. PM uses `OK` as a
/// reply code.
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
/// Service restarted. C: `ERESTART` — sys/errno.h:196 (under `_SYSTEM` it is
/// -200; minix-types uses the positive user-space convention, see
/// 03-stage-rs/99-rs-global-concepts.md §errno sign convention).
/// RS uses it as the default for `r_init_err` (manager.c:1828, clone_slot).
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
    /// C: `EBADF` — sys/errno.h:9 (bad descriptor; `dup2` range rejects).
    pub const EBADF: Errno = Errno(EBADF);
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
    /// RS's diagnostic surface (error.c `rs_strerror`/`init_strerror`/
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

/// C's `sys_nerr`: the length of `sys_errlist`. `strerror(3)` only indexes the
/// table when the number is below it, otherwise it uses `"Unknown error: %u"`
/// (minix3/lib/libc/compat/gen/compat_errlist.c:153,
/// minix3/lib/libc/string/strerror_r.c:66).
pub const SYS_NERR: usize = 80;

/// C `strerror(3)`'s sentence table: the index is the errno number.
///
/// The source of truth is `minix3/lib/libc/compat/gen/compat_errlist.c:52-151`
/// (the only definition of `sys_errlist` in this tree), not the same-named
/// table in glibc or Linux — the wording differs: 34 is
/// `Result too large or too small` in Minix3 while glibc writes
/// `Numerical result out of range`; 35 `Resource temporarily unavailable`
/// serves both `EAGAIN` and `EWOULDBLOCK` (C has only one string; the alias is
/// marked by a comment); 47 `Address family not supported by protocol family`
/// is its own entry and is not the same sentence as 46's
/// `Protocol family not supported`.
///
/// Every entry was cross-checked against the numbers in
/// `minix3/sys/sys/errno.h`: a script extracts the 80 strings line by line and
/// asserts that each annotated line satisfies
/// `strings[annotation number] == the line's string` (79 annotated lines, zero
/// mismatches; number 0 has no name in the table header and is recorded as
/// `ENOERROR` following C convention).
const SYS_ERRLIST: [&str; SYS_NERR] = [
    /*  0 ENOERROR               */ "Undefined error: 0",
    /*  1 EPERM                  */ "Operation not permitted",
    /*  2 ENOENT                 */ "No such file or directory",
    /*  3 ESRCH                  */ "No such process",
    /*  4 EINTR                  */ "Interrupted system call",
    /*  5 EIO                    */ "Input/output error",
    /*  6 ENXIO                  */ "Device not configured",
    /*  7 E2BIG                  */ "Argument list too long",
    /*  8 ENOEXEC                */ "Exec format error",
    /*  9 EBADF                  */ "Bad file descriptor",
    /* 10 ECHILD                 */ "No child processes",
    /* 11 EDEADLK                */ "Resource deadlock avoided",
    /* 12 ENOMEM                 */ "Cannot allocate memory",
    /* 13 EACCES                 */ "Permission denied",
    /* 14 EFAULT                 */ "Bad address",
    /* 15 ENOTBLK                */ "Block device required",
    /* 16 EBUSY                  */ "Device busy",
    /* 17 EEXIST                 */ "File exists",
    /* 18 EXDEV                  */ "Cross-device link",
    /* 19 ENODEV                 */ "Operation not supported by device",
    /* 20 ENOTDIR                */ "Not a directory",
    /* 21 EISDIR                 */ "Is a directory",
    /* 22 EINVAL                 */ "Invalid argument",
    /* 23 ENFILE                 */ "Too many open files in system",
    /* 24 EMFILE                 */ "Too many open files",
    /* 25 ENOTTY                 */ "Inappropriate ioctl for device",
    /* 26 ETXTBSY                */ "Text file busy",
    /* 27 EFBIG                  */ "File too large",
    /* 28 ENOSPC                 */ "No space left on device",
    /* 29 ESPIPE                 */ "Illegal seek",
    /* 30 EROFS                  */ "Read-only file system",
    /* 31 EMLINK                 */ "Too many links",
    /* 32 EPIPE                  */ "Broken pipe",
    /* 33 EDOM                   */ "Numerical argument out of domain",
    /* 34 ERANGE                 */ "Result too large or too small",
    /* 35 EAGAIN                 */ "Resource temporarily unavailable",
    /* 36 EINPROGRESS            */ "Operation now in progress",
    /* 37 EALREADY               */ "Operation already in progress",
    /* 38 ENOTSOCK               */ "Socket operation on non-socket",
    /* 39 EDESTADDRREQ           */ "Destination address required",
    /* 40 EMSGSIZE               */ "Message too long",
    /* 41 EPROTOTYPE             */ "Protocol wrong type for socket",
    /* 42 ENOPROTOOPT            */ "Protocol option not available",
    /* 43 EPROTONOSUPPORT        */ "Protocol not supported",
    /* 44 ESOCKTNOSUPPORT        */ "Socket type not supported",
    /* 45 EOPNOTSUPP             */ "Operation not supported",
    /* 46 EPFNOSUPPORT           */ "Protocol family not supported",
    /* 47 EAFNOSUPPORT           */ "Address family not supported by protocol family",
    /* 48 EADDRINUSE             */ "Address already in use",
    /* 49 EADDRNOTAVAIL          */ "Can't assign requested address",
    /* 50 ENETDOWN               */ "Network is down",
    /* 51 ENETUNREACH            */ "Network is unreachable",
    /* 52 ENETRESET              */ "Network dropped connection on reset",
    /* 53 ECONNABORTED           */ "Software caused connection abort",
    /* 54 ECONNRESET             */ "Connection reset by peer",
    /* 55 ENOBUFS                */ "No buffer space available",
    /* 56 EISCONN                */ "Socket is already connected",
    /* 57 ENOTCONN               */ "Socket is not connected",
    /* 58 ESHUTDOWN              */ "Can't send after socket shutdown",
    /* 59 ETOOMANYREFS           */ "Too many references: can't splice",
    /* 60 ETIMEDOUT              */ "Operation timed out",
    /* 61 ECONNREFUSED           */ "Connection refused",
    /* 62 ELOOP                  */ "Too many levels of symbolic links",
    /* 63 ENAMETOOLONG           */ "File name too long",
    /* 64 EHOSTDOWN              */ "Host is down",
    /* 65 EHOSTUNREACH           */ "No route to host",
    /* 66 ENOTEMPTY              */ "Directory not empty",
    /* 67 EPROCLIM               */ "Too many processes",
    /* 68 EUSERS                 */ "Too many users",
    /* 69 EDQUOT                 */ "Disc quota exceeded",
    /* 70 ESTALE                 */ "Stale NFS file handle",
    /* 71 EREMOTE                */ "Too many levels of remote in path",
    /* 72 EBADRPC                */ "RPC struct is bad",
    /* 73 ERPCMISMATCH           */ "RPC version wrong",
    /* 74 EPROGUNAVAIL           */ "RPC prog. not avail",
    /* 75 EPROGMISMATCH          */ "Program version wrong",
    /* 76 EPROCUNAVAIL           */ "Bad procedure for program",
    /* 77 ENOLCK                 */ "No locks available",
    /* 78 ENOSYS                 */ "Function not implemented",
    /* 79 EFTYPE                 */ "Inappropriate file type or format",
];

impl Errno {
    /// The sentence C `strerror(3)` returns (`sys_errlist[errno]`); `None`
    /// when the number is outside the table.
    ///
    /// This is a different thing from [`Errno::name`]: `name()` gives the
    /// identifier (`"EINVAL"`), this gives the sentence for readers
    /// (`"Invalid argument"`). C's `err(3)`/`warn(3)` append the latter —
    /// the shape in `minix3/lib/libc/gen/verrc.c` is `progname: ` + the
    /// caller's format string + `": "` + `strerror(code)` + newline.
    pub fn message(self) -> Option<&'static str> {
        // C converts `int num` to `unsigned int` before comparing
        // (strerror_r.c:56/:66), so a negative number wraps to a very large
        // value here and falls straight outside the table.
        let errnum = self.0 as u32;
        if (errnum as usize) < SYS_NERR {
            Some(SYS_ERRLIST[errnum as usize])
        } else {
            None
        }
    }
}

/// The full shape of C `strerror(3)`: take the sentence inside the table and
/// C's `Unknown error: %u` outside it
/// (minix3/lib/libc/string/strerror_r.c:55/:78). The `%u` argument is
/// `unsigned int`, so negative numbers print as the wrapped-around large value
/// — that is C's own behavior, not an invention of this tree.
pub struct Strerror(pub Errno);

impl core::fmt::Display for Strerror {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0.message() {
            Some(text) => f.write_str(text),
            None => write!(f, "Unknown error: {}", self.0.to_i32() as u32),
        }
    }
}

/// Minix3's `perror(3)` line: `<caller's string>: <strerror sentence>` — note
/// that `perror` in this table has **no program-name prefix**
/// (`minix3/lib/libc/stdio/perror.c` prints
/// `fprintf(stderr, "%s%s%s\n", s, separator, buf)`, and `separator` is the
/// empty string when the passed string is empty), which is a different thing
/// from the prefixed `err(3)`/`warn(3)` ([`err_line`]). Both live here so the
/// difference of "which function prefixes the program name" has exactly one
/// answer.
pub fn perror_line(label: &str, code: Errno, out: &mut [u8]) -> usize {
    use core::fmt::Write as _;
    struct Sink<'a>(&'a mut [u8], &'a mut usize);
    impl core::fmt::Write for Sink<'_> {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            for byte in text.as_bytes() {
                if *self.1 < self.0.len() {
                    self.0[*self.1] = *byte;
                }
                *self.1 += 1;
            }
            Ok(())
        }
    }
    let separator = if label.is_empty() { "" } else { ": " };
    let mut written = 0usize;
    //[SD19-gate] The local Sink's write_str always returns Ok(()) (silently
    // truncating on overflow), so write! has no failure surface; C's
    // counterpart rendering is (void)strerror_r (perror.c:66).
    #[allow(clippy::let_underscore_must_use)]
    let _ = write!(
        Sink(out, &mut written),
        "{label}{separator}{std}",
        std = Strerror(code)
    );
    written
}

/// C's `getprogname()`: the program name is the last segment of `argv[0]`
/// taken at run time — `minix3/lib/csu/common/crt0-common.c:156-165` scans to
/// the last `/` and takes the part after it, using the whole string when there
/// is no slash. The `err(3)`/`warn(3)`/`usage()` lines all start with it, so
/// this is part of the contract rather than decoration.
pub fn prog_name(argv0: &str) -> &str {
    match argv0.rfind('/') {
        Some(slash) => &argv0[slash + 1..],
        None => argv0,
    }
}

/// Rendering of C's `err(3)`/`warn(3)` line: `prog: label: <sentence>`.
///
/// The shape comes verbatim from `minix3/lib/libc/gen/verrc.c` —
/// `getprogname()`, `": "`, the caller's string, `": "`, `strerror(code)`,
/// newline (the caller supplies the newline, because C's `vwarn` splits it the
/// same way). The sentence comes from the `SYS_ERRLIST` table; outside the
/// table [`Strerror`] emits C's `Unknown error: %u`. It lives here instead of
/// in each command's shared code so all consumers share one authority — C also
/// has only one `verrc`.
pub fn err_line(prog: &str, label: &str, code: Errno, out: &mut [u8]) -> usize {
    use core::fmt::Write as _;
    struct Sink<'a>(&'a mut [u8], &'a mut usize);
    impl core::fmt::Write for Sink<'_> {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            for byte in text.as_bytes() {
                if *self.1 < self.0.len() {
                    self.0[*self.1] = *byte;
                }
                *self.1 += 1;
            }
            Ok(())
        }
    }
    let mut written = 0usize;
    //[SD19-gate] The local Sink's write_str always returns Ok(()) (silently
    // truncating on overflow), so write! has no failure surface; C's verrc.c
    // rendering likewise does not check the write result.
    #[allow(clippy::let_underscore_must_use)]
    let _ = write!(
        Sink(out, &mut written),
        "{prog}: {label}: {std}",
        std = Strerror(code)
    );
    written
}

/// The identifier face of an errno: `"EPERM"` for known values, `Errno(N)`
/// otherwise. The RS server's contextual descriptions (init/lu_strerror —
/// error.c:48/:56) compose on top of this. C's `strerror(3)` sentence lives
/// in [`Errno::message`] / [`Strerror`], not here.
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

/// Status word on the server reply channel: folds this tree's **positive**
/// errno constants into the **negative** sign used on the wire.
///
/// Minix3 services are compiled with `_SYSTEM` (hardwired in the service
/// headers: `minix3/minix/servers/ds/inc.h:6`, `servers/ipc/inc.h:1`,
/// `servers/is/inc.h:7`, `servers/rs/inc.h:7`, `servers/pm/pm.h:4`,
/// `servers/sched/sched.h:4`, `servers/devman/devman.h:3`,
/// `servers/vfs/fs.h:7`, `servers/vm/vm.h:5`), so the error constants
/// themselves carry a negative sign ("get OK and negative error codes"); a
/// service therefore writes back a negative error code in `m.m_type = result`,
/// and the user side `_taskcall` treats `m_type < 0` as an error. This tree's
/// errno values are positive constants (`ESRCH == 3`), so every `_SYSTEM`
/// service must fold the sign once at the reply boundary — VFS established
/// this discipline in `queue_reply` (`servers/vfs/src/main_loop.rs`) and DS in
/// `dispatch::reply_status` (`servers/ds/src/dispatch.rs`); this function is
/// their single-source home.
///
/// **Exception**: MIB does not define `_SYSTEM`
/// (`minix3/minix/servers/mib/Makefile` has no such definition and no
/// `inc.h`), so its error codes are positive on the wire — MIB and its client
/// pair must not use this function (there is already a precedent in the repo:
/// the positive-errno check in `sysctl_via`).
///
/// `0` (OK) and already-negative values pass through unchanged (no double
/// negation); non-negative success payloads (fds, byte counts) are unaffected.
pub const fn reply_status(result: i32) -> i32 {
    if result > 0 { -result } else { result }
}

#[cfg(test)]
mod to_errno_trait_tests {
    use super::*;

    // Consistency test between the trait delegation and the existing
    // inherent methods for PmError / KernelError.
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
        // The mapping target of SchedProcError::BadCpu (used by the
        // kernel-side impl).
        assert_eq!(Errno::EBADCPU.to_i32(), EBADCPU);
        assert_eq!(EBADCPU, 217);
    }

    /// Reply-boundary sign folding: `_SYSTEM` service error codes are
    /// negative on the wire; OK and already-negative values are untouched. The
    /// criteria are pinned to the C constants (EPERM=1, ESRCH=3, ELOCKED=208).
    #[test]
    fn test_reply_status_folds_positive_errno() {
        assert_eq!(reply_status(0), 0); // OK
        assert_eq!(reply_status(EPERM), -1);
        assert_eq!(reply_status(ESRCH), -3);
        assert_eq!(reply_status(ELOCKED), -208);
        assert_eq!(reply_status(-EPERM), -EPERM);
        // Every positive number is folded as an error code: callers must not
        // route success payloads (fds, byte counts) through here.
        assert_eq!(reply_status(7), -7);
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

#[cfg(test)]
mod strerror_face_tests {
    extern crate alloc;
    use super::{EBADF, ENOENT, Errno, SYS_ERRLIST, SYS_NERR, Strerror};
    use alloc::string::ToString as _;

    /// The table length equals C's `sys_nerr`, and there is no empty string
    /// (all 80 C entries carry text; an empty string would make `err(3)` print
    /// a bare colon tail).
    #[test]
    fn table_is_c_sys_nerr_long_with_no_empty_entry() {
        assert_eq!(SYS_ERRLIST.len(), SYS_NERR);
        assert_eq!(SYS_NERR, 80);
        assert!(SYS_ERRLIST.iter().all(|text| !text.is_empty()));
    }

    /// The wording follows Minix3's table, not the glibc/Linux one: 6 is
    /// `Device not configured` in Minix3 (glibc writes
    /// `No such device or address`), 19 is
    /// `Operation not supported by device` (glibc writes `No such device`), 34
    /// is `Result too large or too small` (glibc writes
    /// `Numerical result out of range`). 46 and 47 are two sentences, not one
    /// alias.
    #[test]
    fn wording_follows_the_minix3_table_not_glibc() {
        assert_eq!(Errno::from_i32(6).message(), Some("Device not configured"));
        assert_eq!(
            Errno::from_i32(19).message(),
            Some("Operation not supported by device")
        );
        assert_eq!(
            Errno::from_i32(34).message(),
            Some("Result too large or too small")
        );
        assert_eq!(
            Errno::from_i32(46).message(),
            Some("Protocol family not supported")
        );
        assert_eq!(
            Errno::from_i32(47).message(),
            Some("Address family not supported by protocol family")
        );
    }

    /// The sentence face and the identifier face are two different things: the
    /// same `EINVAL` gives `Invalid argument` on one side and `EINVAL` on the
    /// other. A shell that mixes them up prints the uppercase code in the
    /// `err(3)` suffix.
    #[test]
    fn message_and_name_are_two_different_faces() {
        assert_eq!(Errno::EINVAL.message(), Some("Invalid argument"));
        assert_eq!(Errno::EINVAL.name(), Some("EINVAL"));
        assert_eq!(
            Errno::from_i32(ENOENT).message(),
            Some("No such file or directory")
        );
        assert_eq!(
            Errno::from_i32(EBADF).message(),
            Some("Bad file descriptor")
        );
        assert_ne!(
            Errno::EINVAL.message().unwrap(),
            Errno::EINVAL.name().unwrap()
        );
    }

    /// The shape of the `err(3)` line is given verbatim by `verrc.c`: the
    /// program name, colon-space, the caller's string, colon-space, the
    /// sentence. Numbers outside the table are rendered as C's
    /// `Unknown error: %u`; sentences and identifiers must not be mixed up
    /// (`EINVAL`'s sentence is `Invalid argument`, not `EINVAL`).
    #[test]
    fn err_line_renders_c_verrc_shape() {
        extern crate alloc;
        use super::{EINVAL, EISDIR, ENOENT, EPERM, err_line};
        use alloc::string::ToString as _;
        let mut out = [0u8; 96];
        let len = err_line("hostname", "sethostname", Errno::from_i32(EINVAL), &mut out);
        assert_eq!(&out[..len], b"hostname: sethostname: Invalid argument");
        let len = err_line("ppt", "fgets", Errno::from_i32(EISDIR), &mut out);
        assert_eq!(&out[..len], b"ppt: fgets: Is a directory");
        let len = err_line(
            "factor",
            "stdin",
            Errno::from_i32(4294967295u32 as i32),
            &mut out,
        );
        assert_eq!(&out[..len], b"factor: stdin: Unknown error: 4294967295");
        // Truncation happens only when the buffer is too small; the length is
        // still reported as the required byte count (callers use it to tell
        // whether everything was written).
        let mut small = [0u8; 8];
        let len = err_line("rm", "unlink", Errno::from_i32(ENOENT), &mut small);
        assert_eq!(&small[..], b"rm: unli");
        assert!(len > small.len());
        assert_eq!(
            Strerror(Errno::from_i32(EPERM)).to_string(),
            "Operation not permitted"
        );

        // Minix3's `perror` does not prefix the program name:
        // `perror("unlink")` yields `unlink: <sentence>`, and with an empty
        // string even the colon-space is omitted (`perror.c`'s `separator`
        // rule).
        let mut line = [0u8; 96];
        let len = super::perror_line("unlink", Errno::from_i32(ENOENT), &mut line);
        assert_eq!(&line[..len], b"unlink: No such file or directory");
        let len = super::perror_line("", Errno::from_i32(ENOENT), &mut line);
        assert_eq!(&line[..len], b"No such file or directory");

        // The program name is taken as the last segment, not the whole
        // `argv[0]`: that is how C's run time splits it.
        assert_eq!(super::prog_name("/bin/mkfifo"), "mkfifo");
        assert_eq!(super::prog_name("mkfifo"), "mkfifo");
        assert_eq!(super::prog_name(""), "");
    }

    /// The table boundary matches C's `unsigned` comparison: number 0 has a
    /// string (`Undefined error: 0`), 79 is the last entry, and 80 falls
    /// outside; a negative number first wraps to a large `unsigned` value, so
    /// it also falls outside, and `Strerror` prints C's `Unknown error: %u`
    /// wrapped reading.
    #[test]
    fn table_edges_and_unknown_shape_follow_c_unsigned_compare() {
        assert_eq!(Errno::from_i32(0).message(), Some("Undefined error: 0"));
        assert_eq!(
            Errno::from_i32(79).message(),
            Some("Inappropriate file type or format")
        );
        assert_eq!(Errno::from_i32(80).message(), None);
        assert_eq!(Errno::from_i32(-1).message(), None);
        assert_eq!(
            Strerror(Errno::from_i32(80)).to_string(),
            "Unknown error: 80"
        );
        assert_eq!(
            Strerror(Errno::from_i32(-1)).to_string(),
            "Unknown error: 4294967295"
        );
        assert_eq!(
            Strerror(Errno::EINVAL).to_string(),
            "Invalid argument",
            "表内的号不该走 Unknown error 分支"
        );
    }
}

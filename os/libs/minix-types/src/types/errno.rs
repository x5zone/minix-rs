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

/// C 的 `sys_nerr`：`sys_errlist` 的表长。`strerror(3)` 只在号小于它时查表，
/// 否则走 `"Unknown error: %u"`（minix3/lib/libc/compat/gen/compat_errlist.c:153、
/// minix3/lib/libc/string/strerror_r.c:66）。
pub const SYS_NERR: usize = 80;

/// C `strerror(3)` 的句子表：下标就是 errno 号。
///
/// 真源是 `minix3/lib/libc/compat/gen/compat_errlist.c:52-151`（本树里 `sys_errlist`
/// 只有这一份定义），不是 glibc 或 Linux 的同名表——两者措辞不同：34 号在 Minix3 里是
/// `Result too large or too small`，而 glibc 写 `Numerical result out of range`；
/// 35 号 `Resource temporarily unavailable` 同时服务 `EAGAIN` 与 `EWOULDBLOCK`（C 只给
/// 一份串，别名靠注释标出）；47 号 `Address family not supported by protocol family`
/// 独立成项，与 46 号的 `Protocol family not supported` 不是一句。
///
/// 表项逐条与 `minix3/sys/sys/errno.h` 的号对过：脚本按行提取 80 项串，并断言每个带
/// 注记的行满足 `strings[注记号] == 该行的串`（79 条带注记行零失配；0 号在表头没有名字，
/// 按 C 惯例记作 `ENOERROR`）。
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
    /// C `strerror(3)` 返回的那句话（`sys_errlist[errno]`）；号在表外时 `None`。
    ///
    /// 这与 [`Errno::name`] 是两件事：`name()` 给标识符（`"EINVAL"`），这里给读者读的
    /// 句子（`"Invalid argument"`）。C 的 `err(3)`/`warn(3)` 缀的是后者——
    /// `minix3/lib/libc/gen/verrc.c` 的形状是 `progname: ` + 调用者的格式串 + `": "` +
    /// `strerror(code)` + 换行。
    pub fn message(self) -> Option<&'static str> {
        // C 把 `int num` 先转成 `unsigned int` 再比大小（strerror_r.c:56/:66），
        // 所以负数在这里绕成一个很大的数、直接落到表外。
        let errnum = self.0 as u32;
        if (errnum as usize) < SYS_NERR {
            Some(SYS_ERRLIST[errnum as usize])
        } else {
            None
        }
    }
}

/// C `strerror(3)` 的完整形状：表内取句子，表外取 C 的 `Unknown error: %u`
/// （minix3/lib/libc/string/strerror_r.c:55/:78）。`%u` 的实参是 `unsigned int`，
/// 因此负数会打成绕回后的大数——这是 C 自己的行为，不是本树的发明。
pub struct Strerror(pub Errno);

impl core::fmt::Display for Strerror {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0.message() {
            Some(text) => f.write_str(text),
            None => write!(f, "Unknown error: {}", self.0.to_i32() as u32),
        }
    }
}

/// Minix3 的 `perror(3)` 那一行：`<调用者给的串>: <strerror 的句子>`——注意这张表里的
/// `perror` **不带程序名前缀**（`minix3/lib/libc/stdio/perror.c` 打的是
/// `fprintf(stderr, "%s%s%s\n", s, separator, buf)`，`separator` 在串为空时是空串），
/// 与带前缀的 `err(3)`/`warn(3)`（[`err_line`]）是两件事。把两者都放在这里，是为了让
/// 「哪一个函数缀程序名」这个差别只有一处答案。
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
    //[SD19-gate] 本地 Sink 的 write_str 恒回 Ok(())（超容量静默截断），
    // write! 无失败面；C 对同类渲染是 (void)strerror_r（perror.c:66）。
    #[allow(clippy::let_underscore_must_use)]
    let _ = write!(
        Sink(out, &mut written),
        "{label}{separator}{std}",
        std = Strerror(code)
    );
    written
}

/// C 的 `getprogname()`：程序名是运行时刻从 `argv[0]` 取的最后一段——
/// `minix3/lib/csu/common/crt0-common.c:156-165` 扫到最后一个 `/` 就取其后面那段，
/// 没有斜杠则整段照用。`err(3)`/`warn(3)`/`usage()` 的那几行都以它开头，所以这是契约
/// 的一部分而不是装饰。
pub fn prog_name(argv0: &str) -> &str {
    match argv0.rfind('/') {
        Some(slash) => &argv0[slash + 1..],
        None => argv0,
    }
}

/// C 的 `err(3)`/`warn(3)` 那一行的渲染：`prog: label: <句子>`。
///
/// 形状逐字来自 `minix3/lib/libc/gen/verrc.c`——`getprogname()`、`": "`、调用者给的串、
/// `": "`、`strerror(code)`、换行（换行由调用者补，因为 C 的 `vwarn` 也这样分）。句子出自
/// `SYS_ERRLIST` 那张表，表外由 [`Strerror`] 出 C 的 `Unknown error: %u`。放在这里而不是
/// 各命令的共享件里，是为了让所有消费者共用一个权威——C 也只有一个 `verrc`。
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
    //[SD19-gate] 本地 Sink 的 write_str 恒回 Ok(())（超容量静默截断），
    // write! 无失败面；C verrc.c 的渲染同样不检查写入结果。
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

/// 服务器回复通道上的状态字：把本树**正号** errno 常量折成线序上的**负号**。
///
/// Minix3 的服务以 `_SYSTEM` 编译（各服务头里写死：
/// `minix3/minix/servers/ds/inc.h:6`、`servers/ipc/inc.h:1`、`servers/is/inc.h:7`、
/// `servers/rs/inc.h:7`、`servers/pm/pm.h:4`、`servers/sched/sched.h:4`、
/// `servers/devman/devman.h:3`、`servers/vfs/fs.h:7`、`servers/vm/vm.h:5`），
/// 错码常量本身带负号（“get OK and negative error codes”）；服务
/// `m.m_type = result` 写回的因此是负错码，而用户侧 `_taskcall` 按
/// `m_type < 0` 判错。本树 errno 是正号常量（`ESRCH == 3`），所以每个
/// `_SYSTEM` 服务都必须在回复边界折一次符号——这一纪律已分别在 VFS 的
/// `queue_reply`（`servers/vfs/src/main_loop.rs`）与 DS 的
/// `dispatch::reply_status`（`servers/ds/src/dispatch.rs`）立过，本函数是
/// 它们的单源归位。
///
/// **例外**：MIB 不定义 `_SYSTEM`（`minix3/minix/servers/mib/Makefile` 无此定义，
/// 也无 `inc.h`），它的错码线序上是正数——MIB 与其客户端对不能套本函数
/// （仓内已有先例：`sysctl_via` 的正 errno 判定）。
///
/// `0`（OK）与已负值原样透传（不会二次取负）；非负成功载荷（fd、字节数）
/// 不受影响。
pub const fn reply_status(result: i32) -> i32 {
    if result > 0 { -result } else { result }
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

    /// 回复边界折号：`_SYSTEM` 服务的错码在线序上是负数，OK/已负值不动。
    /// 判据锈在 C 常量上（EPERM=1、ESRCH=3、ELOCKED=208）。
    #[test]
    fn test_reply_status_folds_positive_errno() {
        assert_eq!(reply_status(0), 0); // OK
        assert_eq!(reply_status(EPERM), -1);
        assert_eq!(reply_status(ESRCH), -3);
        assert_eq!(reply_status(ELOCKED), -208);
        assert_eq!(reply_status(-EPERM), -EPERM);
        // 正数一律当错码折：调用方不得把成功载荷（fd、字节数）走这里。
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

    /// 表长与 C 的 `sys_nerr` 同数，且没有空串（C 的 80 项里每一项都有文字，
    /// 空串会让 `err(3)` 打出光秃秃的冒号尾巴）。
    #[test]
    fn table_is_c_sys_nerr_long_with_no_empty_entry() {
        assert_eq!(SYS_ERRLIST.len(), SYS_NERR);
        assert_eq!(SYS_NERR, 80);
        assert!(SYS_ERRLIST.iter().all(|text| !text.is_empty()));
    }

    /// 措辞以 Minix3 的表为纲，不是 glibc/Linux 的那一张：6 号在 Minix3 里是
    /// `Device not configured`（glibc 写 `No such device or address`），19 号是
    /// `Operation not supported by device`（glibc 写 `No such device`），34 号是
    /// `Result too large or too small`（glibc 写 `Numerical result out of range`）。
    /// 46 与 47 是两句话，不是一个别名。
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

    /// 句子面与标识符面是两件事：同一个 `EINVAL` 一边给 `Invalid argument`，
    /// 一边给 `EINVAL`。把两者混起来的壳会在 `err(3)` 后缀里打出大写代号。
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

    /// `err(3)` 那一行的形状是 `verrc.c` 逐字给的：程序名、冒号空格、调用者的串、
    /// 冒号空格、句子。表外的号在渲染里就换成 C 的 `Unknown error: %u`，句子与代号
    /// 不许混（`EINVAL` 的句子是 `Invalid argument`，不是 `EINVAL`）。
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
        // 截断只发生在缓冲区不足时，长度仍按需要的字节数报（调用者据此判断写没写全）。
        let mut small = [0u8; 8];
        let len = err_line("rm", "unlink", Errno::from_i32(ENOENT), &mut small);
        assert_eq!(&small[..], b"rm: unli");
        assert!(len > small.len());
        assert_eq!(
            Strerror(Errno::from_i32(EPERM)).to_string(),
            "Operation not permitted"
        );

        // Minix3 的 `perror` 不缀程序名：`perror("unlink")` 出的是 `unlink: <句子>`，
        // 空串时连那个冒号空格都没有（`perror.c` 的 `separator` 规则）。
        let mut line = [0u8; 96];
        let len = super::perror_line("unlink", Errno::from_i32(ENOENT), &mut line);
        assert_eq!(&line[..len], b"unlink: No such file or directory");
        let len = super::perror_line("", Errno::from_i32(ENOENT), &mut line);
        assert_eq!(&line[..len], b"No such file or directory");

        // 程序名取末段，而不是整个 `argv[0]`：C 的运行时刻就是这么切的。
        assert_eq!(super::prog_name("/bin/mkfifo"), "mkfifo");
        assert_eq!(super::prog_name("mkfifo"), "mkfifo");
        assert_eq!(super::prog_name(""), "");
    }

    /// 表边界与 C 的 `unsigned` 比较同形：0 号有串（`Undefined error: 0`），79 号是
    /// 最后一项，80 号落表外；负号先绕成 `unsigned` 的大数，因此也落表外，而
    /// `Strerror` 打出 C 那句 `Unknown error: %u` 的绕回读数。
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

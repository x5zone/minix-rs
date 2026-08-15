//! POSIX errno constants.
//!
//! Corresponds to Minix3's `<sys/errno.h>`.
//!
//! # Notes
//!
//! These values follow Minix3's errno numbering, which differs from Linux
//! in some cases (e.g., `ENOSYS = 78` in Minix3 vs `38` in Linux).

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
pub const EDONTREPLY: i32 = 203;
/// Generic error. C: `EGENERIC` — sys/errno.h:200.
pub const EGENERIC: i32 = 204;
/// Specified endpoint is not alive. C: `EDEADEPT` — sys/errno.h:211.
pub const EDEADEPT: i32 = 215;
pub const EFTYPE: i32 = 79;
pub const EAUTH: i32 = 80;
pub const ENEEDAUTH: i32 = 81;
pub const EIDRM: i32 = 82;
pub const ENOMSG: i32 = 83;
pub const EOVERFLOW: i32 = 84;
pub const EILSEQ: i32 = 85;
pub const ENOTSUP: i32 = 86;

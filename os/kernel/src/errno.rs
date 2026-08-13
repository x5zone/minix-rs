//! Minix3 error codes — single source of truth for the kernel crate.
//!
//! # Minix3 C Source Mapping
//!
//! All values are transcribed from `minix3/sys/sys/errno.h` (line numbers
//! preserved in doc comments for cross-reference). The `(_SIGN N)` macro in
//! C expands to `-N` for user-space visibility; kernel-internal `i32`
//! constants keep the positive value and negate at the IPC boundary
//! (matches `KcallResult::Ok(errno)` semantics, see 13-syscall-dispatch.md).
//!
//! # Design Decision
//!
//! Previously each `syscall_*.rs` file defined its own local `const EINVAL`
//! etc., leading to duplicated values that drifted out of sync with Minix3
//! (e.g. `misc.rs` had `EBUSY = 27` but `errno.h:58` defines `EBUSY = 16`;
//! `ENOSYS` was `38` in three files but `78` in `errno.h:137`; `ELOOP` was
//! `40` in two files but `62` in `errno.h:114`). This module centralizes
//! the definitions so a future errno addition or correction only touches
//! one file.
//!
//! # Usage
//!
//! ```ignore
//! use crate::errno::*;
//! // ...
//! KcallResult::Ok(EBUSY)  // return EBUSY to caller
//! ```

// ── Success ──

/// Success. C: `#define OK 0` (minix/ipc.h, lib.h)
pub const OK: i32 = 0;

// ── Standard errno (errno.h:42-96) ──

/// Operation not permitted. C: `errno.h:42`
pub const EPERM: i32 = 1;
/// No such file or directory. C: `errno.h:43`
pub const ENOENT: i32 = 2;
/// No such process. C: `errno.h:44`
pub const ESRCH: i32 = 3;
/// Interrupted system call. C: `errno.h:45`
pub const EINTR: i32 = 4;
/// Input/output error. C: `errno.h:46`
pub const EIO: i32 = 5;
/// Device not configured. C: `errno.h:47`
pub const ENXIO: i32 = 6;
/// Argument list too long. C: `errno.h:48`
pub const E2BIG: i32 = 7;
/// Exec format error. C: `errno.h:49`
pub const ENOEXEC: i32 = 8;
/// Bad file descriptor. C: `errno.h:50`
pub const EBADF: i32 = 9;
/// No child processes. C: `errno.h:51`
pub const ECHILD: i32 = 10;
/// Resource deadlock avoided. C: `errno.h:52`
pub const EDEADLK: i32 = 11;
/// Cannot allocate memory. C: `errno.h:54`
pub const ENOMEM: i32 = 12;
/// Permission denied. C: `errno.h:55`
pub const EACCES: i32 = 13;
/// Bad address. C: `errno.h:56`
pub const EFAULT: i32 = 14;
/// Block device required. C: `errno.h:57`
pub const ENOTBLK: i32 = 15;
/// Device busy. C: `errno.h:58`
pub const EBUSY: i32 = 16;
/// File exists. C: `errno.h:59`
pub const EEXIST: i32 = 17;
/// Cross-device link. C: `errno.h:60`
pub const EXDEV: i32 = 18;
/// Operation not supported by device. C: `errno.h:61`
pub const ENODEV: i32 = 19;
/// Not a directory. C: `errno.h:62`
pub const ENOTDIR: i32 = 20;
/// Is a directory. C: `errno.h:63`
pub const EISDIR: i32 = 21;
/// Invalid argument. C: `errno.h:64`
pub const EINVAL: i32 = 22;
/// Too many open files in system. C: `errno.h:65`
pub const ENFILE: i32 = 23;
/// Too many open files. C: `errno.h:66`
pub const EMFILE: i32 = 24;
/// Inappropriate ioctl for device. C: `errno.h:67`
pub const ENOTTY: i32 = 25;
/// Text file busy. C: `errno.h:68`
pub const ETXTBSY: i32 = 26;
/// File too large. C: `errno.h:69`
pub const EFBIG: i32 = 27;
/// No space left on device. C: `errno.h:70`
pub const ENOSPC: i32 = 28;
/// Illegal seek. C: `errno.h:71`
pub const ESPIPE: i32 = 29;
/// Read-only file system. C: `errno.h:72`
pub const EROFS: i32 = 30;
/// Too many links. C: `errno.h:73`
pub const EMLINK: i32 = 31;
/// Broken pipe. C: `errno.h:74`
pub const EPIPE: i32 = 32;
/// Numerical argument out of domain. C: `errno.h:77`
pub const EDOM: i32 = 33;
/// Result too large or too small. C: `errno.h:78`
pub const ERANGE: i32 = 34;
/// Resource temporarily unavailable. C: `errno.h:81`
pub const EAGAIN: i32 = 35;
/// Operation now in progress. C: `errno.h:83`
pub const EINPROGRESS: i32 = 36;
/// Operation already in progress. C: `errno.h:84`
pub const EALREADY: i32 = 37;
/// Socket operation on non-socket. C: `errno.h:87`
pub const ENOTSOCK: i32 = 38;
/// Destination address required. C: `errno.h:88`
pub const EDESTADDRREQ: i32 = 39;
/// Message too long. C: `errno.h:89`
pub const EMSGSIZE: i32 = 40;
/// Protocol wrong type for socket. C: `errno.h:90`
pub const EPROTOTYPE: i32 = 41;
/// Protocol option not available. C: `errno.h:91`
pub const ENOPROTOOPT: i32 = 42;
/// Protocol not supported. C: `errno.h:92`
pub const EPROTONOSUPPORT: i32 = 43;
/// Socket type not supported. C: `errno.h:93`
pub const ESOCKTNOSUPPORT: i32 = 44;
/// Operation not supported. C: `errno.h:94`
pub const EOPNOTSUPP: i32 = 45;
/// Protocol family not supported. C: `errno.h:95`
pub const EPFNOSUPPORT: i32 = 46;
/// Address family not supported by protocol family. C: `errno.h:96`
pub const EAFNOSUPPORT: i32 = 47;
/// Address already in use. C: `errno.h:97`
pub const EADDRINUSE: i32 = 48;
/// Can't assign requested address. C: `errno.h:98`
pub const EADDRNOTAVAIL: i32 = 49;
/// Network is down. C: `errno.h:101`
pub const ENETDOWN: i32 = 50;
/// Network is unreachable. C: `errno.h:102`
pub const ENETUNREACH: i32 = 51;
/// Network dropped connection on reset. C: `errno.h:103`
pub const ENETRESET: i32 = 52;
/// Software caused connection abort. C: `errno.h:104`
pub const ECONNABORTED: i32 = 53;
/// Connection reset by peer. C: `errno.h:105`
pub const ECONNRESET: i32 = 54;
/// No buffer space available. C: `errno.h:106`
pub const ENOBUFS: i32 = 55;
/// Socket is already connected. C: `errno.h:107`
pub const EISCONN: i32 = 56;
/// Socket is not connected. C: `errno.h:108`
pub const ENOTCONN: i32 = 57;
/// Can't send after socket shutdown. C: `errno.h:109`
pub const ESHUTDOWN: i32 = 58;
/// Too many references: can't splice. C: `errno.h:110`
pub const ETOOMANYREFS: i32 = 59;
/// Operation timed out. C: `errno.h:111`
pub const ETIMEDOUT: i32 = 60;
/// Connection refused. C: `errno.h:112`
pub const ECONNREFUSED: i32 = 61;
/// Too many levels of symbolic links. C: `errno.h:114`
pub const ELOOP: i32 = 62;
/// File name too long. C: `errno.h:115`
pub const ENAMETOOLONG: i32 = 63;
/// Host is down. C: `errno.h:118`
pub const EHOSTDOWN: i32 = 64;
/// No route to host. C: `errno.h:119`
pub const EHOSTUNREACH: i32 = 65;
/// Directory not empty. C: `errno.h:120`
pub const ENOTEMPTY: i32 = 66;
/// Too many processes. C: `errno.h:123`
pub const EPROCLIM: i32 = 67;
/// Too many users. C: `errno.h:124`
pub const EUSERS: i32 = 68;
/// Disc quota exceeded. C: `errno.h:125`
pub const EDQUOT: i32 = 69;
/// Stale NFS file handle. C: `errno.h:128`
pub const ESTALE: i32 = 70;
/// Too many levels of remote in path. C: `errno.h:129`
pub const EREMOTE: i32 = 71;
/// RPC struct is bad. C: `errno.h:130`
pub const EBADRPC: i32 = 72;
/// RPC version wrong. C: `errno.h:131`
pub const ERPCMISMATCH: i32 = 73;
/// RPC prog. not avail. C: `errno.h:132`
pub const EPROGUNAVAIL: i32 = 74;
/// Program version wrong. C: `errno.h:133`
pub const EPROGMISMATCH: i32 = 75;
/// Bad procedure for program. C: `errno.h:134`
pub const EPROCUNAVAIL: i32 = 76;
/// No locks available. C: `errno.h:136`
pub const ENOLCK: i32 = 77;
/// Function not implemented. C: `errno.h:137`
pub const ENOSYS: i32 = 78;
/// Inappropriate file type or format. C: `errno.h:139`
pub const EFTYPE: i32 = 79;
/// Authentication error. C: `errno.h:140`
pub const EAUTH: i32 = 80;
/// Need authenticator. C: `errno.h:141`
pub const ENEEDAUTH: i32 = 81;
/// Identifier removed. C: `errno.h:144`
pub const EIDRM: i32 = 82;
/// No message of desired type. C: `errno.h:145`
pub const ENOMSG: i32 = 83;
/// Value too large to be stored in data type. C: `errno.h:146`
pub const EOVERFLOW: i32 = 84;
/// Illegal byte sequence. C: `errno.h:149`
pub const EILSEQ: i32 = 85;
/// Not supported. C: `errno.h:153`
pub const ENOTSUP: i32 = 86;
/// Operation canceled. C: `errno.h:156`
pub const ECANCELED: i32 = 87;
/// Bad or Corrupt message. C: `errno.h:159`
pub const EBADMSG: i32 = 88;
/// No message available. C: `errno.h:162`
pub const ENODATA: i32 = 89;
/// No STREAM resources. C: `errno.h:163`
pub const ENOSR: i32 = 90;
/// Not a STREAM. C: `errno.h:164`
pub const ENOSTR: i32 = 91;
/// STREAM ioctl timeout. C: `errno.h:165`
pub const ETIME: i32 = 92;
/// Attribute not found. C: `errno.h:168`
pub const ENOATTR: i32 = 93;
/// Multihop attempted. C: `errno.h:171`
pub const EMULTIHOP: i32 = 94;
/// Link has been severed. C: `errno.h:172`
pub const ENOLINK: i32 = 95;
/// Protocol error. C: `errno.h:173`
pub const EPROTO: i32 = 96;

// ── Minix3-specific errno (errno.h:196-217) ──

/// Service restarted. C: `errno.h:196`
pub const ERESTART: i32 = 200;
/// Source or destination is not ready. C: `errno.h:197`
pub const ENOTREADY: i32 = 201;
/// Source or destination is not alive. C: `errno.h:198`
pub const EDEADSRCDST: i32 = 202;
/// Pseudo-code: don't send a reply. C: `errno.h:199`
pub const EDONTREPLY: i32 = 203;
/// Generic error. C: `errno.h:200`
pub const EGENERIC: i32 = 204;
/// Invalid packet size for some protocol. C: `errno.h:201`
pub const EPACKSIZE: i32 = 205;
/// Urgent data present. C: `errno.h:202`
pub const EURG: i32 = 206;
/// No urgent data present. C: `errno.h:203`
pub const ENOURG: i32 = 207;
/// Can't send message due to deadlock. C: `errno.h:204`
pub const ELOCKED: i32 = 208;
/// Illegal system call number. C: `errno.h:205`
pub const EBADCALL: i32 = 209;
/// No permission for system call. C: `errno.h:206`
pub const ECALLDENIED: i32 = 210;
/// IPC trap not allowed. C: `errno.h:207`
pub const ETRAPDENIED: i32 = 211;
/// Destination cannot handle request. C: `errno.h:208`
pub const EBADREQUEST: i32 = 212;
/// Badmode in ioctl. C: `errno.h:209`
pub const EBADMODE: i32 = 213;
/// No such connection. C: `errno.h:210`
pub const ENOCONN: i32 = 214;
/// Specified endpoint is not alive. C: `errno.h:211`
pub const EDEADEPT: i32 = 215;
/// Specified endpoint is bad. C: `errno.h:212`
pub const EBADEPT: i32 = 216;
/// Requested CPU does not work. C: `errno.h:213`
pub const EBADCPU: i32 = 217;

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-crate consistency: this module is the kernel's single source
    /// of truth; `minix-types` carries a POSIX subset for shared IPC types.
    /// Same-named constants must never drift (2026-08-13 audit: 86/86
    /// identical — this test locks that invariant for the core subset).
    #[test]
    fn errno_values_match_minix_types() {
        for (kernel, shared) in [
            (EPERM, minix_types::EPERM),
            (ENOENT, minix_types::ENOENT),
            (ESRCH, minix_types::ESRCH),
            (EINTR, minix_types::EINTR),
            (EIO, minix_types::EIO),
            (ENXIO, minix_types::ENXIO),
            (E2BIG, minix_types::E2BIG),
            (ENOEXEC, minix_types::ENOEXEC),
            (EBADF, minix_types::EBADF),
            (ECHILD, minix_types::ECHILD),
            (EDEADLK, minix_types::EDEADLK),
            (ENOMEM, minix_types::ENOMEM),
            (EACCES, minix_types::EACCES),
            (EFAULT, minix_types::EFAULT),
            (EBUSY, minix_types::EBUSY),
            (EINVAL, minix_types::EINVAL),
            (EAGAIN, minix_types::EAGAIN),
            (ENOSYS, minix_types::ENOSYS),
            (ELOOP, minix_types::ELOOP),
        ] {
            assert_eq!(kernel, shared, "errno drift: kernel vs minix-types");
        }
    }

    /// Verify key errno values match Minix3 `minix3/sys/sys/errno.h`.
    /// Guards against the drift bugs that motivated this module
    /// (previously `EBUSY=27`, `ENOSYS=38`, `ELOOP=40` in various files).
    #[test]
    fn errno_values_match_minix3() {
        // Standard errno — errno.h:42-96
        assert_eq!(EPERM, 1, "errno.h:42");
        assert_eq!(ENOENT, 2, "errno.h:43");
        assert_eq!(ESRCH, 3, "errno.h:44");
        assert_eq!(EINTR, 4, "errno.h:45");
        assert_eq!(EIO, 5, "errno.h:46");
        assert_eq!(ENXIO, 6, "errno.h:47");
        assert_eq!(E2BIG, 7, "errno.h:48");
        assert_eq!(ENOEXEC, 8, "errno.h:49");
        assert_eq!(EBADF, 9, "errno.h:50");
        assert_eq!(ECHILD, 10, "errno.h:51");
        assert_eq!(EDEADLK, 11, "errno.h:52");
        assert_eq!(ENOMEM, 12, "errno.h:54");
        assert_eq!(EACCES, 13, "errno.h:55");
        assert_eq!(EFAULT, 14, "errno.h:56");
        assert_eq!(ENOTBLK, 15, "errno.h:57");
        assert_eq!(EBUSY, 16, "errno.h:58");
        assert_eq!(EEXIST, 17, "errno.h:59");
        assert_eq!(EXDEV, 18, "errno.h:60");
        assert_eq!(ENODEV, 19, "errno.h:61");
        assert_eq!(ENOTDIR, 20, "errno.h:62");
        assert_eq!(EISDIR, 21, "errno.h:63");
        assert_eq!(EINVAL, 22, "errno.h:64");
        assert_eq!(ENFILE, 23, "errno.h:65");
        assert_eq!(EMFILE, 24, "errno.h:66");
        assert_eq!(ENOTTY, 25, "errno.h:67");
        assert_eq!(ETXTBSY, 26, "errno.h:68");
        assert_eq!(EFBIG, 27, "errno.h:69");
        assert_eq!(ENOSPC, 28, "errno.h:70");
        assert_eq!(ESPIPE, 29, "errno.h:71");
        assert_eq!(EROFS, 30, "errno.h:72");
        assert_eq!(EMLINK, 31, "errno.h:73");
        assert_eq!(EPIPE, 32, "errno.h:74");
        assert_eq!(EDOM, 33, "errno.h:77");
        assert_eq!(ERANGE, 34, "errno.h:78");
        assert_eq!(EAGAIN, 35, "errno.h:81");
        assert_eq!(EINPROGRESS, 36, "errno.h:83");
        assert_eq!(EALREADY, 37, "errno.h:84");
        assert_eq!(ENOTSOCK, 38, "errno.h:87");
        assert_eq!(EDESTADDRREQ, 39, "errno.h:88");
        assert_eq!(EMSGSIZE, 40, "errno.h:89");
        assert_eq!(EPROTOTYPE, 41, "errno.h:90");
        assert_eq!(ENOPROTOOPT, 42, "errno.h:91");
        assert_eq!(EPROTONOSUPPORT, 43, "errno.h:92");
        assert_eq!(ESOCKTNOSUPPORT, 44, "errno.h:93");
        assert_eq!(EOPNOTSUPP, 45, "errno.h:94");
        assert_eq!(EPFNOSUPPORT, 46, "errno.h:95");
        assert_eq!(EAFNOSUPPORT, 47, "errno.h:96");
        assert_eq!(EADDRINUSE, 48, "errno.h:97");
        assert_eq!(EADDRNOTAVAIL, 49, "errno.h:98");
        assert_eq!(ENETDOWN, 50, "errno.h:101");
        assert_eq!(ENETUNREACH, 51, "errno.h:102");
        assert_eq!(ENETRESET, 52, "errno.h:103");
        assert_eq!(ECONNABORTED, 53, "errno.h:104");
        assert_eq!(ECONNRESET, 54, "errno.h:105");
        assert_eq!(ENOBUFS, 55, "errno.h:106");
        assert_eq!(EISCONN, 56, "errno.h:107");
        assert_eq!(ENOTCONN, 57, "errno.h:108");
        assert_eq!(ESHUTDOWN, 58, "errno.h:109");
        assert_eq!(ETOOMANYREFS, 59, "errno.h:110");
        assert_eq!(ETIMEDOUT, 60, "errno.h:111");
        assert_eq!(ECONNREFUSED, 61, "errno.h:112");
        assert_eq!(ELOOP, 62, "errno.h:114");
        assert_eq!(ENAMETOOLONG, 63, "errno.h:115");
        assert_eq!(EHOSTDOWN, 64, "errno.h:118");
        assert_eq!(EHOSTUNREACH, 65, "errno.h:119");
        assert_eq!(ENOTEMPTY, 66, "errno.h:120");
        assert_eq!(EPROCLIM, 67, "errno.h:123");
        assert_eq!(EUSERS, 68, "errno.h:124");
        assert_eq!(EDQUOT, 69, "errno.h:125");
        assert_eq!(ESTALE, 70, "errno.h:128");
        assert_eq!(EREMOTE, 71, "errno.h:129");
        assert_eq!(EBADRPC, 72, "errno.h:130");
        assert_eq!(ERPCMISMATCH, 73, "errno.h:131");
        assert_eq!(EPROGUNAVAIL, 74, "errno.h:132");
        assert_eq!(EPROGMISMATCH, 75, "errno.h:133");
        assert_eq!(EPROCUNAVAIL, 76, "errno.h:134");
        assert_eq!(ENOLCK, 77, "errno.h:136");
        assert_eq!(ENOSYS, 78, "errno.h:137");
        assert_eq!(EFTYPE, 79, "errno.h:139");
        assert_eq!(EAUTH, 80, "errno.h:140");
        assert_eq!(ENEEDAUTH, 81, "errno.h:141");
        assert_eq!(EIDRM, 82, "errno.h:144");
        assert_eq!(ENOMSG, 83, "errno.h:145");
        assert_eq!(EOVERFLOW, 84, "errno.h:146");
        assert_eq!(EILSEQ, 85, "errno.h:149");
        assert_eq!(ENOTSUP, 86, "errno.h:153");
        assert_eq!(ECANCELED, 87, "errno.h:156");
        assert_eq!(EBADMSG, 88, "errno.h:159");
        assert_eq!(ENODATA, 89, "errno.h:162");
        assert_eq!(ENOSR, 90, "errno.h:163");
        assert_eq!(ENOSTR, 91, "errno.h:164");
        assert_eq!(ETIME, 92, "errno.h:165");
        assert_eq!(ENOATTR, 93, "errno.h:168");
        assert_eq!(EMULTIHOP, 94, "errno.h:171");
        assert_eq!(ENOLINK, 95, "errno.h:172");
        assert_eq!(EPROTO, 96, "errno.h:173");

        // Minix3-specific errno — errno.h:196-217
        assert_eq!(ERESTART, 200, "errno.h:196");
        assert_eq!(ENOTREADY, 201, "errno.h:197");
        assert_eq!(EDEADSRCDST, 202, "errno.h:198");
        assert_eq!(EDONTREPLY, 203, "errno.h:199");
        assert_eq!(EGENERIC, 204, "errno.h:200");
        assert_eq!(EPACKSIZE, 205, "errno.h:201");
        assert_eq!(EURG, 206, "errno.h:202");
        assert_eq!(ENOURG, 207, "errno.h:203");
        assert_eq!(ELOCKED, 208, "errno.h:204");
        assert_eq!(EBADCALL, 209, "errno.h:205");
        assert_eq!(ECALLDENIED, 210, "errno.h:206");
        assert_eq!(ETRAPDENIED, 211, "errno.h:207");
        assert_eq!(EBADREQUEST, 212, "errno.h:208");
        assert_eq!(EBADMODE, 213, "errno.h:209");
        assert_eq!(ENOCONN, 214, "errno.h:210");
        assert_eq!(EDEADEPT, 215, "errno.h:211");
        assert_eq!(EBADEPT, 216, "errno.h:212");
        assert_eq!(EBADCPU, 217, "errno.h:213");

        // Previously-buggy values — regression guard
        assert_eq!(EBUSY, 16, "was 27 in misc.rs (EFBIG's value)");
        assert_eq!(ENOSYS, 78, "was 38 in misc/syscall_device/syscall_signal (ENOTSOCK's value)");
        assert_eq!(ELOOP, 62, "was 40 in syscall_copy/grant (EMSGSIZE's value)");
    }
}

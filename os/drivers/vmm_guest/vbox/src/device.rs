//! Device result-code translation and the host-time drift rule.
//!
//! C correspondence: `minix3/minix/drivers/vmm_guest/vbox/err.c` (the
//! 203-entry `codes[]` table and `convert_err`) and the
//! `convert_result`/`vbox_update_time` pair (`hgcm.c:44-68`,
//! `vbox.c:57-81`).
//!
//! The VirtualBox result codes are a separate error space from Minix3's
//! errno values: `convert_result` maps the seven HGCM transport codes by
//! hand, treats every non-negative code as success, and delegates the rest
//! to the table. The table is reproduced entry for entry from `err.c`
//! (index `-code`), including its deliberate quirks: codes `-51` and
//! `-123` and `-201` are success in VirtualBox's space, and any code
//! outside the table (including all positive codes) becomes
//! [`minix_types::EGENERIC`].

use minix_types::{
    E2BIG, EACCES, EAGAIN, EBADF, EBUSY, ECANCELED, EDONTREPLY, EEXIST, EFAULT, EFBIG, EGENERIC,
    EILSEQ, EINTR, EINVAL, EIO, EISDIR, ELOOP, ENAMETOOLONG, ENFILE, ENOENT, ENOLCK, ENOMEM,
    ENOSPC, ENOSYS, ENOTDIR, ENOTEMPTY, ENOTTY, ENXIO, EOPNOTSUPP, EOVERFLOW, EPERM, ERANGE, EROFS,
    ESPIPE, ESRCH, ETIMEDOUT, ETXTBSY, EXDEV, OK,
};

use crate::vmmdev::{
    ERR_HGCM_ASYNC_EXEC, ERR_HGCM_DENIED, ERR_HGCM_INTERNAL, ERR_HGCM_INVALID_ADDR,
    ERR_HGCM_INVALID_ID, ERR_HGCM_NOT_FOUND,
};

/// How often the driver asks the host for the time, in seconds
/// (`DEFAULT_INTERVAL`).
pub const DEFAULT_INTERVAL: i64 = 1;

/// The smallest host-time lead that triggers a clock update, in seconds
/// (`DEFAULT_DRIFT`).
pub const DEFAULT_DRIFT: i64 = 2;

/// The VirtualBox result-code table (`codes[]`, `err.c`): index `-code`,
/// so entry zero is success and the last entry maps `-202`.
pub const ERR_TABLE: [i32; 203] = [
    OK,           // 0: success
    EGENERIC,     // -1: general failure
    EINVAL,       // -2: invalid parameter
    EINVAL,       // -3: invalid magic
    EBADF,        // -4: invalid handle
    ENOLCK,       // -5: lock failed
    EFAULT,       // -6: invalid pointer
    EGENERIC,     // -7: patching IDT failed
    ENOMEM,       // -8: memory allocation failed
    EEXIST,       // -9: already loaded
    EPERM,        // -10: permission denied
    EINVAL,       // -11: version mismatch
    ENOSYS,       // -12: function not implemented
    EGENERIC,     // -13:
    EGENERIC,     // -14:
    EGENERIC,     // -15:
    EGENERIC,     // -16:
    EGENERIC,     // -17:
    EGENERIC,     // -18: not equal
    EINVAL,       // -19: not a symlink
    ENOMEM,       // -20: temporary memory allocation failed
    EINVAL,       // -21: invalid file mode
    EINVAL,       // -22: incorrect call order
    EGENERIC,     // -23: no TLS available
    EGENERIC,     // -24: failed to set TLS
    EGENERIC,     // -25:
    ENOMEM,       // -26: contiguous memory allocation failed
    ENOMEM,       // -27: no memory available for page table
    EGENERIC,     // -28:
    ESRCH,        // -29: thread is dead
    EINVAL,       // -30: thread is not waitable
    EGENERIC,     // -31: page table not present
    EINVAL,       // -32: invalid context
    EBUSY,        // -33: timer is busy
    EGENERIC,     // -34: address conflict
    EGENERIC,     // -35: unresolved error
    ENOTTY,       // -36: invalid function
    EINVAL,       // -37: not supported
    EACCES,       // -38: access denied
    EINTR,        // -39: interrupted
    ETIMEDOUT,    // -40: timeout
    E2BIG,        // -41: buffer overflow
    E2BIG,        // -42: too much data
    EAGAIN,       // -43: max thread number reached
    EAGAIN,       // -44: max process number reached
    EGENERIC,     // -45: signal refused
    EBUSY,        // -46: signal already pending
    EINVAL,       // -47: invalid signal
    EGENERIC,     // -48: state changed
    EINVAL,       // -49: invalid UUID format
    ESRCH,        // -50: process not found
    OK,           // -51: waited-for process is still running
    EAGAIN,       // -52: try again
    EGENERIC,     // -53: generic parse error
    ERANGE,       // -54: value out of range
    EOVERFLOW,    // -55: value too big
    EGENERIC,     // -56: no digits in string
    ERANGE,       // -57: minus sign in unsigned value
    EILSEQ,       // -58: character translation failed
    EGENERIC,     // -59: encountered unicode byte order mark
    EGENERIC,     // -60: encountered unicode surrogate
    EILSEQ,       // -61: invalid UTF8 encoding
    EILSEQ,       // -62: invalid UTF16 encoding
    EGENERIC,     // -63: no UTF16 for character
    ENOMEM,       // -64: string memory allocation failed
    ENOMEM,       // -65: UTF16 string memory allocation failed
    ENOMEM,       // -66: code point array allocation failed
    EBUSY,        // -67: cannot free in-use memory
    EGENERIC,     // -68: timer already active
    EGENERIC,     // -69: timer already suspended
    ECANCELED,    // -70: operation cancelled
    ENOMEM,       // -71: failed to initialize memory object
    ENOMEM,       // -72: low physical memory allocation failed
    ENOMEM,       // -73: physical memory allocation failed
    EGENERIC,     // -74: address too big
    EGENERIC,     // -75: memory mapping failed
    EGENERIC,     // -76: trailing characters
    EGENERIC,     // -77: trailing spaces
    ESRCH,        // -78: not found
    EGENERIC,     // -79: invalid state
    ENOMEM,       // -80: out of resources
    ENFILE,       // -81: no more handles
    EGENERIC,     // -82: preemption disabled
    EGENERIC,     // -83: end of string
    EGENERIC,     // -84: page count out of range
    EGENERIC,     // -85: object destroyed
    EGENERIC,     // -86: dangling objects
    EGENERIC,     // -87: invalid Base64 encoding
    EGENERIC,     // -88: return triggered by callback
    EGENERIC,     // -89: authentication failure
    EGENERIC,     // -90: not a power of two
    EGENERIC,     // -91: ignored
    EGENERIC,     // -92: concurrent access not allowed
    EGENERIC,     // -93: invalid reference sharing
    EGENERIC,     // -94:
    EGENERIC,     // -95: no change
    ENOMEM,       // -96: executable memory allocation failed
    EINVAL,       // -97: unsupported alignment
    EGENERIC,     // -98: duplicate
    EGENERIC,     // -99: missing
    EIO,          // -100: I/O error
    ENXIO,        // -101: open failed
    ENOENT,       // -102: file not found
    ENOTDIR,      // -103: path not found (may also mean ENOENT)
    EINVAL,       // -104: invalid name
    EEXIST,       // -105: already exists
    ENFILE,       // -106: too many open files
    EIO,          // -107: seek error
    EINVAL,       // -108: negative seek
    ESPIPE,       // -109: seek on device
    EGENERIC,     // -110: end of file
    EIO,          // -111: generic read error
    EIO,          // -112: generic write error
    EROFS,        // -113: write protected
    ETXTBSY,      // -114: sharing violation
    ENOLCK,       // -115: file lock failed
    EAGAIN,       // -116: file lock violation
    EIO,          // -117: cannot create file
    EIO,          // -118: cannot delete directory
    EXDEV,        // -119: not the same device
    ENAMETOOLONG, // -120: file name too long
    ENXIO,        // -121: media not present
    EIO,          // -122: media not recognized
    OK,           // -123: nothing to unlocked
    EGENERIC,     // -124: lock lost
    ENOTEMPTY,    // -125: directory not empty
    ENOTDIR,      // -126: not a directory
    EISDIR,       // -127: is a directory
    EFBIG,        // -128: file too big
    EGENERIC,     // -129: no asynchronous I/O request
    EGENERIC,     // -130: asynchronous I/O in progress
    EGENERIC,     // -131: asynchronous I/O completed
    EGENERIC,     // -132: asynchronous I/O busy
    EGENERIC,     // -133: asynchronous I/O limit exceeded
    EGENERIC,     // -134: asynchronous I/O canceled
    EGENERIC,     // -135: asynchronous I/O not submitted
    EGENERIC,     // -136: asynchronous I/O not prepared
    EGENERIC,     // -137: asynchronous I/O out of resources
    EBUSY,        // -138: device or resource busy
    EGENERIC,     // -139: not a file
    EGENERIC,     // -140: is a file
    EGENERIC,     // -141: unexpected file type
    EGENERIC,     // -142: missing path root specification
    EGENERIC,     // -143: path is relative
    EGENERIC,     // -144: path is not relative
    EGENERIC,     // -145:
    EGENERIC,     // -146:
    EGENERIC,     // -147:
    EGENERIC,     // -148:
    EGENERIC,     // -149:
    EIO,          // -150: disk I/O error
    ENXIO,        // -151: invalid drive number
    ENOSPC,       // -152: disk full
    EIO,          // -153: disk changed
    EGENERIC,     // -154: drive locked
    ENXIO,        // -155: invalid disk format
    ELOOP,        // -156: too many symlinks
    EOPNOTSUPP,   // -157: can not set symlink file times
    EOPNOTSUPP,   // -158: can not change symlink owner
    EGENERIC,     // -159:
    EGENERIC,     // -160:
    EGENERIC,     // -161:
    EGENERIC,     // -162:
    EGENERIC,     // -163:
    EGENERIC,     // -164:
    EGENERIC,     // -165:
    EGENERIC,     // -166:
    EGENERIC,     // -167:
    EGENERIC,     // -168:
    EGENERIC,     // -169:
    EGENERIC,     // -170:
    EGENERIC,     // -171:
    EGENERIC,     // -172:
    EGENERIC,     // -173:
    EGENERIC,     // -174:
    EGENERIC,     // -175:
    EGENERIC,     // -176:
    EGENERIC,     // -177:
    EGENERIC,     // -178:
    EGENERIC,     // -179:
    EGENERIC,     // -180:
    EGENERIC,     // -181:
    EGENERIC,     // -182:
    EGENERIC,     // -183:
    EGENERIC,     // -184:
    EGENERIC,     // -185:
    EGENERIC,     // -186:
    EGENERIC,     // -187:
    EGENERIC,     // -188:
    EGENERIC,     // -189:
    EGENERIC,     // -190:
    EGENERIC,     // -191:
    EGENERIC,     // -192:
    EGENERIC,     // -193:
    EGENERIC,     // -194:
    EGENERIC,     // -195:
    EGENERIC,     // -196:
    EGENERIC,     // -197:
    EGENERIC,     // -198:
    EGENERIC,     // -199:
    EGENERIC,     // -200: search error
    OK,           // -201: no more files
    ENFILE,       // -202: no more search handles available
];

/// Translates a VirtualBox result code outside the HGCM transport set
/// (`convert_err`, `err.c:257-270`).
///
/// The index arithmetic is unsigned like the C original: a non-negative
/// `code` wraps to a huge index and falls through to [`EGENERIC`], and so
/// does any code beyond the table.
pub fn convert_err(code: i32) -> i32 {
    let index = code.wrapping_neg() as u32;
    if (index as usize) < ERR_TABLE.len() {
        ERR_TABLE[index as usize]
    } else {
        EGENERIC
    }
}

/// Translates a VirtualBox result code into a POSIX error code
/// (`convert_result`, `hgcm.c:44-68`).
///
/// The seven HGCM transport codes are matched first — two of them are
/// *positive* in VirtualBox's space, so the success shortcut must come
/// after them. [`ERR_HGCM_ASYNC_EXEC`] maps to [`EDONTREPLY`], the
/// protocol's "no reply yet" marker: the request completes later through
/// an interrupt.
pub fn convert_result(result: i32) -> i32 {
    match result {
        ERR_HGCM_NOT_FOUND => ESRCH,
        ERR_HGCM_DENIED => EPERM,
        ERR_HGCM_INVALID_ADDR => EFAULT,
        ERR_HGCM_ASYNC_EXEC => EDONTREPLY,
        ERR_HGCM_INTERNAL => EGENERIC,
        ERR_HGCM_INVALID_ID => EINVAL,
        _ => {
            // Positive codes are success codes.
            if result >= 0 { OK } else { convert_err(result) }
        }
    }
}

/// The drift rule (`vbox_update_time`, `vbox.c:70-76`): advance the
/// system clock only when the host time leads the current time by at
/// least `drift` seconds.
///
/// C computes `ntime - otime >= drift` in signed seconds; a host clock
/// behind the guest's therefore never moves time backward.
pub fn should_advance(current: i64, host: i64, drift: i64) -> bool {
    host - current >= drift
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_length_and_sampled_entries_match_err_c() {
        assert_eq!(ERR_TABLE.len(), 203);
        // Entry zero is success; the three success codes in the middle of
        // the table are preserved (-51, -123, -201).
        assert_eq!(ERR_TABLE[0], OK);
        assert_eq!(ERR_TABLE[51], OK);
        assert_eq!(ERR_TABLE[123], OK);
        assert_eq!(ERR_TABLE[201], OK);
        // Sampled entries against err.c: -1 general, -2 invalid, -6
        // invalid pointer, -29 thread dead, -70 cancelled, -100 I/O, -101
        // open failed, -120 name too long, -152 disk full, -157/-158 the
        // two unsupported codes, -202 no more search handles.
        assert_eq!(ERR_TABLE[1], EGENERIC);
        assert_eq!(ERR_TABLE[2], EINVAL);
        assert_eq!(ERR_TABLE[6], EFAULT);
        assert_eq!(ERR_TABLE[29], ESRCH);
        assert_eq!(ERR_TABLE[70], ECANCELED);
        assert_eq!(ERR_TABLE[100], EIO);
        assert_eq!(ERR_TABLE[101], ENXIO);
        assert_eq!(ERR_TABLE[120], ENAMETOOLONG);
        assert_eq!(ERR_TABLE[152], ENOSPC);
        assert_eq!(ERR_TABLE[157], EOPNOTSUPP);
        assert_eq!(ERR_TABLE[158], EOPNOTSUPP);
        assert_eq!(ERR_TABLE[202], ENFILE);
    }

    #[test]
    fn test_convert_err_unsigned_index_like_c() {
        assert_eq!(convert_err(0), OK);
        assert_eq!(convert_err(-1), EGENERIC);
        assert_eq!(convert_err(-2), EINVAL);
        assert_eq!(convert_err(-201), OK);
        // Beyond the table: general failure.
        assert_eq!(convert_err(-203), EGENERIC);
        assert_eq!(convert_err(-2900), EGENERIC);
        // Positive codes wrap to a huge unsigned index, exactly like the
        // C `unsigned int index = -code` line.
        assert_eq!(convert_err(5), EGENERIC);
        assert_eq!(convert_err(i32::MIN), EGENERIC);
    }

    #[test]
    fn test_convert_result_maps_the_hgcm_transport_codes() {
        assert_eq!(convert_result(ERR_HGCM_NOT_FOUND), ESRCH);
        assert_eq!(convert_result(ERR_HGCM_DENIED), EPERM);
        assert_eq!(convert_result(ERR_HGCM_INVALID_ADDR), EFAULT);
        assert_eq!(convert_result(ERR_HGCM_ASYNC_EXEC), EDONTREPLY);
        assert_eq!(convert_result(ERR_HGCM_INTERNAL), EGENERIC);
        assert_eq!(convert_result(ERR_HGCM_INVALID_ID), EINVAL);
        // The positive codes must not be swallowed by the success
        // shortcut: 2901 and 2903 are matched above it.
        assert_ne!(convert_result(ERR_HGCM_DENIED), OK);
        assert_ne!(convert_result(ERR_HGCM_ASYNC_EXEC), OK);
        // Other positives are success; negatives go through the table.
        assert_eq!(convert_result(0), OK);
        assert_eq!(convert_result(7), OK);
        assert_eq!(convert_result(-2), EINVAL);
        assert_eq!(convert_result(-100), EIO);
    }

    #[test]
    fn test_drift_rule_only_moves_forward() {
        assert!(should_advance(100, 102, DEFAULT_DRIFT));
        assert!(should_advance(100, 130, DEFAULT_DRIFT));
        // A one-second lead is below the default drift.
        assert!(!should_advance(100, 101, DEFAULT_DRIFT));
        // A host clock behind the guest's never moves time backward.
        assert!(!should_advance(100, 99, DEFAULT_DRIFT));
        assert!(!should_advance(100, 0, DEFAULT_DRIFT));
    }
}

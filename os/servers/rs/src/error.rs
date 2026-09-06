//! RS-contextual error descriptions — the `error.c` port.
//!
//! C: `minix3/minix/servers/rs/error.c` — the `errentry` tables plus three
//! lookup functions. The errno→*name* face is [`minix_types::Errno`]'s
//! `Display`/`name` (E-8: one shared table instead of C's separate
//! `strerror` call); this module only layers the two RS-contextual
//! *description* tables on top:
//!
//! - [`init_strerror`] — init-type failures (consumed by the rs_verbose
//!   init path, main.c:442-449);
//! - [`lu_strerror`] — live-update failures (consumed by the rs_verbose
//!   update path, update.c diagnostics).
//!
//! The consumer face is the 19 wiring's diagnostic seam (no_std RS has no
//! printf; output goes through the kernel diagctl).

use minix_types::Errno;

/// C: `init_strerror` — error.c:48-50, table at :12-15.
pub fn init_strerror(errnum: Errno) -> &'static str {
    match errnum {
        Errno::ENOSYS => "service does not support the requested initialization type",
        Errno::ERESTART => "service requested an initialization reset",
        other => fallback(other),
    }
}

/// C: `lu_strerror` — error.c:56-58, table at :20-25.
pub fn lu_strerror(errnum: Errno) -> &'static str {
    match errnum {
        Errno::ENOSYS => "service does not support live update",
        Errno::EINVAL => "service does not support the required state",
        Errno::EBUSY => "service is not able to prepare for the update now",
        Errno::EGENERIC => "generic error occurred while preparing for the update",
        other => fallback(other),
    }
}

/// C's `rs_strerror` miss branch — `strerror(-errnum)` (error.c:44). The
/// shared errno name table replaces the per-crate `strerror` call; a value
/// without a named constant degrades like C's "Unknown error".
fn fallback(errnum: Errno) -> &'static str {
    errnum.name().unwrap_or("unknown error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_strerror_table() {
        // C: error.c:12-15 — exactly two entries, then the name fallback.
        assert_eq!(
            init_strerror(Errno::ENOSYS),
            "service does not support the requested initialization type"
        );
        assert_eq!(
            init_strerror(Errno::ERESTART),
            "service requested an initialization reset"
        );
        // Miss → strerror equivalent = the shared name face (error.c:44).
        assert_eq!(init_strerror(Errno::EINVAL), "EINVAL");
        assert_eq!(init_strerror(Errno::from_i32(9999)), "unknown error");
    }

    #[test]
    fn test_lu_strerror_table() {
        // C: error.c:20-25 — four entries, then the name fallback.
        assert_eq!(
            lu_strerror(Errno::ENOSYS),
            "service does not support live update"
        );
        assert_eq!(
            lu_strerror(Errno::EINVAL),
            "service does not support the required state"
        );
        assert_eq!(
            lu_strerror(Errno::EBUSY),
            "service is not able to prepare for the update now"
        );
        assert_eq!(
            lu_strerror(Errno::EGENERIC),
            "generic error occurred while preparing for the update"
        );
        assert_eq!(lu_strerror(Errno::ERESTART), "ERESTART");
        // The same errno carries different RS descriptions per context —
        // ENOSYS is an init-table entry AND an lu-table entry with
        // different texts (error.c:13 vs :21).
        assert_ne!(init_strerror(Errno::ENOSYS), lu_strerror(Errno::ENOSYS));
    }
}

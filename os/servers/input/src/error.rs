//! Input server errors: every failure maps to a Minix3 errno number.
//!
//! C: `minix3/minix/servers/input/input.c` returns bare `int` errno values
//! (`ENXIO`, `EBUSY`, `EINVAL`, ...). Passing integers around makes it easy to
//! return the wrong number or to invent a new one; this enum names each failure
//! the 01-04 scope can produce and pins it to its C errno in exactly one place
//! ([`InputError::to_errno`]). Later documents (06-08) extend the set with the
//! handler errors (`EIO`, `EAGAIN`, `EINTR`, `ENOTTY`); the mapping discipline
//! stays the same.
//!
//! Corresponding document: `03-input-device-structs.md` (the lookup failures)
//! and `02-chardriver-framework.md` (reply-suppression sentinels live in
//! `minix_types`, not here — see below).

use minix_types::{EAGAIN, EBUSY, EINTR, EINVAL, EIO, ENOTTY, ENXIO};

/// Failure of an input-server operation in the 01-04 scope.
///
/// Each variant documents the C behavior it mirrors and the errno it becomes
/// on the wire. No variant maps to an invented number: the `to_errno` test
/// below locks every mapping against `minix_types` (which follows
/// `minix3/sys/sys/errno.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// No device owns this minor number.
    ///
    /// C: `input_map` returns `NULL` (`input.c:60-61`); the caller answers
    /// `ENXIO` (`input.c:90-91`, `input_open`). Wire value unchanged.
    UnknownMinor,
    /// A device-table index outside `0..INPUT_DEV_MAX`.
    ///
    /// C: `input_revmap` calls `panic` on such an index (`input.c:79`).
    /// Rust never panics on caller input (architecture evolution A-7): the
    /// lookup returns `None`, and callers that must answer the caller use
    /// `EINVAL` ("invalid argument" is exactly what a bad index is).
    InvalidDeviceIndex,
    /// The device exists but currently has no driver behind it.
    ///
    /// C: `input_open` answers `ENXIO` when the device is not active
    /// (`input.c:93-94`): either nobody owns it and it is not one of the two
    /// always-open multiplexer devices. Same wire number as `UnknownMinor`
    /// (C deliberately does not tell the caller which one failed), but a
    /// different cause internally. First used by the open handler
    /// (document 06).
    DeviceNotActive,
    /// The device is already opened by someone else.
    ///
    /// C: `input_open` answers `EBUSY` (`input.c:96-97`). First used by the
    /// open handler (document 06); declared here so the whole crate shares one
    /// error vocabulary from the start.
    DeviceBusy,
    /// The device was never opened.
    ///
    /// C: `input_close` answers `EINVAL` (`input.c:115-118`). First used by
    /// the close handler (document 06); see `DeviceBusy`.
    NotOpened,
    /// A suspended read exists but the cancel request does not match it.
    ///
    /// C: `input_cancel` answers `EDONTREPLY` for a mismatch (`input.c`
    /// cancel path, document 08). `EDONTREPLY` is not a real errno but the
    /// reply-suppression sentinel (`sys/errno.h:199`); it is listed here so
    /// every non-`OK` handler outcome has a name.
    CancelMismatch,
    /// The caller asked for a non-blocking read with an empty buffer.
    ///
    /// C: `input_read` answers `EAGAIN` (document 07). Listed here for the
    /// same vocabulary reason as `DeviceBusy`.
    WouldBlock,
    /// An input/output failure talking to a driver or a grant.
    ///
    /// C: `EIO` (documents 07/11). Listed here for the same reason.
    InputOutput,
    /// The suspended call was interrupted by a cancel that matched it.
    ///
    /// C: `input_cancel` answers `EINTR` to the woken reader (document 08).
    Interrupted,
    /// The control request is unknown to this device.
    ///
    /// C: `input_ioctl` answers `ENOTTY` for anything but `KIOCSLEDS`
    /// (document 08).
    NotATypewriterControl,
}

impl InputError {
    /// The Minix3 errno number sent on the wire for this failure.
    pub const fn to_errno(self) -> i32 {
        match self {
            InputError::UnknownMinor => ENXIO,
            InputError::DeviceNotActive => ENXIO,
            InputError::InvalidDeviceIndex => EINVAL,
            InputError::DeviceBusy => EBUSY,
            InputError::NotOpened => EINVAL,
            InputError::CancelMismatch => minix_types::EDONTREPLY,
            InputError::WouldBlock => EAGAIN,
            InputError::InputOutput => EIO,
            InputError::Interrupted => EINTR,
            InputError::NotATypewriterControl => ENOTTY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_numbers_match_minix3_errno() {
        // C: `minix3/sys/sys/errno.h` values via `minix_types`.
        assert_eq!(InputError::UnknownMinor.to_errno(), 6); // ENXIO
        assert_eq!(InputError::DeviceNotActive.to_errno(), 6); // ENXIO
        assert_eq!(InputError::InvalidDeviceIndex.to_errno(), 22); // EINVAL
        assert_eq!(InputError::DeviceBusy.to_errno(), 16); // EBUSY
        assert_eq!(InputError::NotOpened.to_errno(), 22); // EINVAL
        assert_eq!(InputError::CancelMismatch.to_errno(), 203); // EDONTREPLY
        assert_eq!(InputError::WouldBlock.to_errno(), 35); // EAGAIN
        assert_eq!(InputError::InputOutput.to_errno(), 5); // EIO
        assert_eq!(InputError::Interrupted.to_errno(), 4); // EINTR
        assert_eq!(InputError::NotATypewriterControl.to_errno(), 25); // ENOTTY
    }

    #[test]
    fn test_two_causes_share_enxio_like_c() {
        // C answers ENXIO both for "no device owns this minor" and for "the
        // device exists but has no driver" (`input_open`, `input.c:90-94`):
        // distinct causes, one wire number. The enum keeps the causes apart
        // while the mapping keeps the wire identical.
        assert_ne!(InputError::UnknownMinor, InputError::DeviceNotActive);
        assert_eq!(
            InputError::UnknownMinor.to_errno(),
            InputError::DeviceNotActive.to_errno()
        );
    }
}

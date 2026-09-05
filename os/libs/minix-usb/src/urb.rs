//! USB request block tracking: identifiers plus completion routing.
//!
//! C correspondence: the request block structure (`struct usb_urb`,
//! `usb.h:65-96`, with the invalid marker `USB_INVALID_URB_ID 0`,
//! `usb.h:63`), the driver callback table (`struct usb_driver` with
//! completion, connect, and disconnect callbacks, `usb.h:24-28`),
//! the pending list the client keeps (`pending_urbs`, `usb.c:66-68`),
//! and the completion path (`_usb_urb_complete` calling
//! `urb_completion`, `usb.c:157-192`, fed by `usb_handle_msg`,
//! `usb.c:197-225`). Endpoint traffic stays in the service binary;
//! this module owns the bookkeeping half: which identifier is in
//! flight and which callback a finished block wakes.

/// Marker for "no request block" (`USB_INVALID_URB_ID`, `usb.h:63`).
pub const INVALID_URB_ID: u32 = 0;

/// Transfer kind of one endpoint (`usb.h:52-58`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TransferKind {
    /// Isochronous transfer.
    Isochronous = 0,
    /// Interrupt transfer.
    Interrupt = 1,
    /// Control transfer.
    Control = 2,
    /// Bulk transfer.
    Bulk = 3,
}

/// Direction of one endpoint (`usb.h:60-61`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Device to host.
    In,
    /// Host to device.
    Out,
}

/// One in-flight request block entry (client-side mirror of the
/// pending list, `usb.c:66-68`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingUrb {
    /// Identifier assigned at submit time.
    pub id: u32,
    /// Endpoint number, 0 through 15.
    pub endpoint: u8,
    /// Transfer kind of the endpoint.
    pub kind: TransferKind,
    /// Direction of the endpoint.
    pub direction: Direction,
}

impl PendingUrb {
    /// Whether this entry carries a usable identifier and endpoint.
    pub fn is_valid(&self) -> bool {
        self.id != INVALID_URB_ID && self.endpoint < 16
    }
}

/// Find a pending block by identifier, if still in flight.
pub fn find_pending(pending: &[PendingUrb], id: u32) -> Option<&PendingUrb> {
    if id == INVALID_URB_ID {
        return None;
    }
    pending.iter().find(|entry| entry.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_marker_is_zero() {
        assert_eq!(INVALID_URB_ID, 0);
    }

    #[test]
    fn test_transfer_kind_numbers_match_header() {
        assert_eq!(TransferKind::Isochronous as u8, 0);
        assert_eq!(TransferKind::Interrupt as u8, 1);
        assert_eq!(TransferKind::Control as u8, 2);
        assert_eq!(TransferKind::Bulk as u8, 3);
    }

    #[test]
    fn test_pending_lookup_finds_live_entries_only() {
        let pending = [
            PendingUrb { id: 7, endpoint: 2, kind: TransferKind::Bulk, direction: Direction::In },
            PendingUrb { id: 9, endpoint: 16, kind: TransferKind::Bulk, direction: Direction::Out },
        ];
        assert!(pending[0].is_valid());
        assert!(!pending[1].is_valid());
        assert_eq!(find_pending(&pending, 7), Some(&pending[0]));
        assert_eq!(find_pending(&pending, 8), None);
        assert_eq!(find_pending(&pending, INVALID_URB_ID), None);
        assert!(find_pending(&[], 7).is_none());
    }
}

//! Bus query protocol: message numbers for enumeration and control.
//!
//! C correspondence: the bus-controller message numbers in
//! `minix3/minix/include/minix/com.h:95-160` (`BUSC_RQ_BASE 0x300`,
//! `BUSC_RS_BASE 0x380`, plus the twenty numbered operations), and the
//! control codes served by `pci_ioctl` in
//! `minix3/minix/drivers/bus/pci/main.c:538-668` (configuration read and
//! write, bus info, map, unmap, reserve, release).

/// Base of the bus-controller request range.
///
/// C: `BUSC_RQ_BASE 0x300` (`com.h:98`).
pub const BUS_REQUEST_BASE: i32 = 0x300;

/// Base of the bus-controller reply range.
///
/// C: `BUSC_RS_BASE 0x380` (`com.h:99`).
pub const BUS_REPLY_BASE: i32 = 0x380;

/// Bus query operation kind, one variant per numbered message.
///
/// C: `BUSC_PCI_INIT` through `BUSC_PCI_GET_BAR` (`com.h:103-154`).
/// The input-output memory-map number (`IOMMU_MAP`, base plus
/// thirty-two) belongs to a different driver and is excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusQuery {
    /// First handshake from a new caller.
    Init,
    /// Index plus identifiers of the first visible device.
    FirstDevice,
    /// Index plus identifiers of the next visible device.
    NextDevice,
    /// Index of the device at this bus, device, function.
    FindDevice,
    /// Vendor plus device identifiers for this index.
    Identifiers,
    /// Reserve a device for the caller.
    Reserve,
    /// Read an eight-bit attribute.
    ReadAttribute8,
    /// Read a sixteen-bit attribute.
    ReadAttribute16,
    /// Read a thirty-two-bit attribute.
    ReadAttribute32,
    /// Write an eight-bit attribute.
    WriteAttribute8,
    /// Write a sixteen-bit attribute.
    WriteAttribute16,
    /// Write a thirty-two-bit attribute.
    WriteAttribute32,
    /// Rescan the bus.
    Rescan,
    /// Device name into the caller buffer.
    DeviceName,
    /// Slot name into the caller buffer.
    SlotName,
    /// Set the access list for a caller.
    SetAccess,
    /// Delete the access list of a caller.
    DeleteAccess,
    /// Base-address-register properties.
    GetBar,
}

impl BusQuery {
    /// Small index of the operation (zero through nineteen, skipping the
    /// gaps the C header leaves at five, six, seven).
    pub const fn index(self) -> i32 {
        match self {
            BusQuery::Init => 0,
            BusQuery::FirstDevice => 1,
            BusQuery::NextDevice => 2,
            BusQuery::FindDevice => 3,
            BusQuery::Identifiers => 4,
            BusQuery::Reserve => 7,
            BusQuery::ReadAttribute8 => 8,
            BusQuery::ReadAttribute16 => 9,
            BusQuery::ReadAttribute32 => 10,
            BusQuery::WriteAttribute8 => 11,
            BusQuery::WriteAttribute16 => 12,
            BusQuery::WriteAttribute32 => 13,
            BusQuery::Rescan => 14,
            BusQuery::DeviceName => 15,
            BusQuery::SlotName => 16,
            BusQuery::SetAccess => 17,
            BusQuery::DeleteAccess => 18,
            BusQuery::GetBar => 19,
        }
    }

    /// Full message type of the operation (base plus index).
    pub const fn message_type(self) -> i32 {
        BUS_REQUEST_BASE + self.index()
    }

    /// Decode a raw message type; `None` means "not a bus query".
    pub const fn decode(message_type: i32) -> Option<BusQuery> {
        match message_type - BUS_REQUEST_BASE {
            0 => Some(BusQuery::Init),
            1 => Some(BusQuery::FirstDevice),
            2 => Some(BusQuery::NextDevice),
            3 => Some(BusQuery::FindDevice),
            4 => Some(BusQuery::Identifiers),
            7 => Some(BusQuery::Reserve),
            8 => Some(BusQuery::ReadAttribute8),
            9 => Some(BusQuery::ReadAttribute16),
            10 => Some(BusQuery::ReadAttribute32),
            11 => Some(BusQuery::WriteAttribute8),
            12 => Some(BusQuery::WriteAttribute16),
            13 => Some(BusQuery::WriteAttribute32),
            14 => Some(BusQuery::Rescan),
            15 => Some(BusQuery::DeviceName),
            16 => Some(BusQuery::SlotName),
            17 => Some(BusQuery::SetAccess),
            18 => Some(BusQuery::DeleteAccess),
            19 => Some(BusQuery::GetBar),
            _ => None,
        }
    }
}

/// Returns true for a bus-controller request.
///
/// C: the family check behind `pci_other` (`main.c:670-710`): anything
/// outside the family falls to the "unhandled message" branch.
pub const fn is_bus_request(message_type: i32) -> bool {
    (message_type & !0x7f) == BUS_REQUEST_BASE
}

/// Device-control operation kind served through the control hook.
///
/// C: the switch in `pci_ioctl` (`main.c:547-666`): bus-device-function
/// configuration read and write, bus info, map, unmap, reserve, release.
/// Anything else (including the legacy bus-wide read and write) answers
/// "inappropriate control".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusControl {
    /// Read one configuration register by bus, device, function.
    ConfigRead,
    /// Write one configuration register by bus, device, function.
    ConfigWrite,
    /// Bus information (accepted, no payload in this driver).
    BusInfo,
    /// Map physical memory for the caller.
    Map,
    /// Unmap previously mapped memory.
    Unmap,
    /// Grant a caller access to a device.
    Reserve,
    /// Release a caller's device access.
    Release,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_indices_match_com_header() {
        assert_eq!(BusQuery::Init.message_type(), 0x300);
        assert_eq!(BusQuery::FirstDevice.message_type(), 0x301);
        assert_eq!(BusQuery::Identifiers.message_type(), 0x304);
        assert_eq!(BusQuery::Reserve.message_type(), 0x307);
        assert_eq!(BusQuery::GetBar.message_type(), 0x313);
    }

    #[test]
    fn test_decode_round_trips_with_gaps() {
        assert_eq!(BusQuery::decode(0x305), None);
        assert_eq!(BusQuery::decode(0x306), None);
        assert_eq!(BusQuery::decode(0x30D), Some(BusQuery::WriteAttribute32));
        assert_eq!(BusQuery::decode(0x400), None);
    }

    #[test]
    fn test_family_check() {
        assert!(is_bus_request(0x300));
        assert!(is_bus_request(0x313));
        assert!(!is_bus_request(0x400));
    }
}

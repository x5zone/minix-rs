//! The ACPI service protocol: request numbers, message shapes, and the
//! PCI limits the routing table is indexed by.
//!
//! C correspondence: `minix3/minix/include/minix/acpi.h` — the two request
//! numbers and the four message structures — and `pci.c`'s two limits
//! (`PCI_MAX_DEVICES`, `PCI_MAX_PINS`, `pci.c:11-12`).
//!
//! The C message structures overlay the kernel message header: the first
//! word is `m_source` and the second is the message type, which the ACPI
//! service reads as its `request` field (`acpi.c:148`) and overwrites with
//! its answer (`do_get_irq` stores the IRQ into the same slot,
//! `pci.c:132-133`). The payload lanes below are therefore measured from
//! the message start.

/// Translate a device and pin into an IRQ (`ACPI_REQ_GET_IRQ`).
pub const ACPI_REQ_GET_IRQ: i32 = 1;
/// Tell the service about a bridge's bus numbers
/// (`ACPI_REQ_MAP_BRIDGE`).
pub const ACPI_REQ_MAP_BRIDGE: i32 = 2;

/// The most PCI devices a bus can carry (`PCI_MAX_DEVICES`, `pci.c:11`).
pub const PCI_MAX_DEVICES: usize = 32;
/// The most interrupt pins a device can raise (`PCI_MAX_PINS`,
/// `pci.c:12`).
pub const PCI_MAX_PINS: usize = 4;
/// The routing table's size (`IRQ_TABLE_ENTRIES`, `pci.c:14`).
pub const IRQ_TABLE_ENTRIES: usize = PCI_MAX_DEVICES * PCI_MAX_PINS;

/// The message type slot, read as the request and written as the answer.
pub const LANE_REQUEST: usize = 4;
/// The `bus`/`primary_bus` lane of both requests.
pub const LANE_BUS: usize = 8;
/// The `dev`/`device` lane.
pub const LANE_DEV: usize = 12;
/// The `pin`/`secondary_bus` lane.
pub const LANE_PIN: usize = 16;
/// The answer lane: the IRQ of a `GET_IRQ` reply, the error code of a
/// `MAP_BRIDGE` reply.
pub const LANE_ANSWER: usize = 8;

/// One `ACPI_REQ_GET_IRQ` request's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetIrqRequest {
    /// The bus the device sits on.
    pub bus: u32,
    /// The device number.
    pub dev: u32,
    /// The interrupt pin.
    pub pin: u32,
}

/// One `ACPI_REQ_MAP_BRIDGE` request's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapBridgeRequest {
    /// The bridge's primary bus.
    pub primary_bus: u32,
    /// The bridge's secondary bus.
    pub secondary_bus: u32,
    /// The device number the bridge answers on.
    pub device: u32,
}

/// Reads one little-endian word out of a message body.
pub fn read_u32(message: &minix_types::Message, lane: usize) -> u32 {
    // SAFETY: byte-level read of the union payload at a documented lane.
    let raw = unsafe { message.m_u.raw };
    u32::from_le_bytes(raw[lane..lane + 4].try_into().unwrap())
}

/// Reads the request slot.
pub fn read_request(message: &minix_types::Message) -> i32 {
    read_u32(message, LANE_REQUEST) as i32
}

/// Writes one little-endian word into a message body.
pub fn write_u32(message: &mut minix_types::Message, lane: usize, value: u32) {
    // SAFETY: byte-level write of the union payload at a documented lane.
    unsafe {
        message.m_u.raw[lane..lane + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// Writes the answer slot.
pub fn write_answer(message: &mut minix_types::Message, value: i32) {
    write_u32(message, LANE_ANSWER, value as u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_request_numbers_match_the_header() {
        assert_eq!(ACPI_REQ_GET_IRQ, 1);
        assert_eq!(ACPI_REQ_MAP_BRIDGE, 2);
    }

    #[test]
    fn test_the_pci_limits_match_pci_c() {
        assert_eq!(PCI_MAX_DEVICES, 32);
        assert_eq!(PCI_MAX_PINS, 4);
        assert_eq!(IRQ_TABLE_ENTRIES, 128);
    }

    #[test]
    fn test_the_lanes_overlay_the_message_header() {
        // The request slot is the message type and the first payload lane
        // is the word after it, exactly as C's `struct acpi_request_hdr`
        // overlays the header.
        let mut message = minix_types::Message::default();
        write_u32(&mut message, LANE_REQUEST, ACPI_REQ_GET_IRQ as u32);
        write_u32(&mut message, LANE_BUS, 1);
        write_u32(&mut message, LANE_DEV, 2);
        write_u32(&mut message, LANE_PIN, 3);
        assert_eq!(read_request(&message), ACPI_REQ_GET_IRQ);
        assert_eq!(read_u32(&message, LANE_BUS), 1);
        assert_eq!(read_u32(&message, LANE_DEV), 2);
        assert_eq!(read_u32(&message, LANE_PIN), 3);
        // The answer overwrites the type slot's neighbour.
        write_answer(&mut message, 11);
        assert_eq!(read_u32(&message, LANE_ANSWER), 11);
    }
}

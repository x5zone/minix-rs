//! Generic device records: add, delete, serialize.
//!
//! C correspondence: `devman_add_device`, `devman_del_device`, and the
//! `serialize_dev` walk in `minix3/minix/lib/libdevman/generic.c`.
//! Strings travel as offsets into a trailing string area (the C
//! serializer packs name plus attribute pairs after the fixed header);
//! this module renders the shape (offsets, counts) without owning any
//! wire buffer.

use alloc::vec::Vec;

/// Maximum devices in one registry (test-sized; production sizes from the
/// enumerated bus, which is topologically bounded).
pub const MAX_DEVICES: usize = 64;

/// Maximum attributes per device.
pub const MAX_ATTRIBUTES: usize = 16;

/// One named attribute of a device (name plus data, both opaque strings
/// owned by the service crate; the lengths live here for sizing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute {
    /// Bytes of the name including the terminator.
    pub name_len: usize,
    /// Bytes of the data including the terminator.
    pub data_len: usize,
}

/// One device record: parent link, name length, attributes.
///
/// C: `struct devman_dev` as walked by `serialize_dev` (`generic.c`):
/// parent identifier, name, attribute list. The fixed header carries the
/// attribute count, the parent identifier, and the name offset; each
/// attribute adds one entry plus two strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    /// Parent device identifier (zero means root).
    pub parent: u32,
    /// Bytes of the name including the terminator.
    pub name_len: usize,
    /// Attributes of this device.
    pub attributes: Vec<Attribute>,
}

impl DeviceRecord {
    /// Fresh record with this parent and name length.
    pub fn new(parent: u32, name_len: usize) -> DeviceRecord {
        DeviceRecord {
            parent,
            name_len,
            attributes: Vec::new(),
        }
    }

    /// Attach one attribute; false when the per-device bound is hit.
    pub fn add_attribute(&mut self, attribute: Attribute) -> bool {
        if self.attributes.len() >= MAX_ATTRIBUTES {
            return false;
        }
        self.attributes.push(attribute);
        true
    }

    /// Serialized size: fixed header plus one entry per attribute plus
    /// all string bytes.
    ///
    /// C: `size = sizeof(info) + count * sizeof(entry)` plus the summed
    /// string lengths (`generic.c`, serialize walk).
    pub fn serialized_size(&self) -> usize {
        const HEADER: usize = 16;
        const ENTRY: usize = 16;
        HEADER
            + self.attributes.len() * ENTRY
            + self.name_len
            + self
                .attributes
                .iter()
                .map(|attribute| attribute.name_len + attribute.data_len)
                .sum::<usize>()
    }
}

/// Driver-side registry: records by handle.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    records: Vec<Option<DeviceRecord>>,
}

impl Registry {
    /// Empty registry.
    pub fn new() -> Registry {
        Registry {
            records: Vec::new(),
        }
    }

    /// Add a record; returns its handle, or `None` when full.
    ///
    /// C: `devman_add_device` (`generic.c`). Freed handles are reused
    /// lowest-first.
    pub fn add(&mut self, record: DeviceRecord) -> Option<usize> {
        if let Some(index) = self.records.iter().position(|slot| slot.is_none()) {
            self.records[index] = Some(record);
            return Some(index);
        }
        if self.records.len() >= MAX_DEVICES {
            return None;
        }
        self.records.push(Some(record));
        Some(self.records.len() - 1)
    }

    /// Delete the record under this handle; false when already empty.
    ///
    /// C: `devman_del_device` (`generic.c`).
    pub fn delete(&mut self, handle: usize) -> bool {
        match self.records.get_mut(handle) {
            Some(slot) if slot.is_some() => {
                *slot = None;
                true
            }
            _ => false,
        }
    }

    /// Borrow the record under this handle.
    pub fn get(&self, handle: usize) -> Option<&DeviceRecord> {
        self.records.get(handle).and_then(|slot| slot.as_ref())
    }

    /// Handles currently in use, in order.
    pub fn live_handles(&self) -> Vec<usize> {
        self.records
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.is_some().then_some(index))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> DeviceRecord {
        DeviceRecord::new(0, 8)
    }

    #[test]
    fn test_add_delete_recycles_handles() {
        let mut registry = Registry::new();
        let first = registry.add(record()).unwrap();
        let second = registry.add(record()).unwrap();
        assert!(registry.delete(first));
        assert!(!registry.delete(first));
        let third = registry.add(record()).unwrap();
        assert_eq!(third, first);
        assert_eq!(registry.live_handles(), [first, second].as_slice());
    }

    #[test]
    fn test_attribute_bound_is_sixteen() {
        let mut record = record();
        for _ in 0..MAX_ATTRIBUTES {
            assert!(record.add_attribute(Attribute {
                name_len: 4,
                data_len: 4
            }));
        }
        assert!(!record.add_attribute(Attribute {
            name_len: 4,
            data_len: 4
        }));
    }

    #[test]
    fn test_serialized_size_sums_header_entries_strings() {
        let mut record = DeviceRecord::new(0, 8);
        record.add_attribute(Attribute {
            name_len: 4,
            data_len: 6,
        });
        assert_eq!(record.serialized_size(), 16 + 16 + 8 + 4 + 6);
    }
}

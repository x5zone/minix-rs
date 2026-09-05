//! USB bookkeeping: device and interface tracking plus bind outcome.
//!
//! C correspondence: `devman_usb_device_new`, `devman_usb_device_add`,
//! `devman_usb_device_remove`, `devman_usb_device_delete`, and
//! `devman_usb_init` in `minix3/minix/lib/libdevman/usb.c:62-301`.
//! Descriptors stay opaque (the USB stack owns them); this module tracks
//! attachment: which interfaces exist, which endpoints bind them, and
//! whether the bind callback accepted.

/// Maximum interfaces tracked per USB device.
///
/// C: `interfaces[32]` in `struct devman_usb_dev` (`devman.h:52`).
pub const MAX_INTERFACES: usize = 32;

/// Maximum USB devices in one tracker.
pub const MAX_USB_DEVICES: usize = 16;

/// Outcome of a bind callback for one interface.
///
/// C: `devman_usb_bind_cb_t` returns an integer endpoint or an error; the
/// tracker only needs accepted versus refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindOutcome {
    /// Bound to this endpoint.
    Bound(i64),
    /// Refused.
    Refused,
}

/// One tracked interface: number plus bind state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackedInterface {
    /// Interface number.
    pub number: u8,
    /// Bound endpoint, if any.
    pub bound: Option<i64>,
}

/// One tracked USB device: identifier plus interfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbDevice {
    /// Device identifier.
    pub id: i32,
    /// Tracked interfaces.
    pub interfaces: alloc::vec::Vec<TrackedInterface>,
}

impl UsbDevice {
    /// Fresh device with no interfaces.
    pub fn new(id: i32) -> UsbDevice {
        UsbDevice {
            id,
            interfaces: alloc::vec::Vec::new(),
        }
    }

    /// Track one more interface; false when the thirty-two bound is hit.
    pub fn add_interface(&mut self, number: u8) -> bool {
        if self.interfaces.len() >= MAX_INTERFACES {
            return false;
        }
        if self.interfaces.iter().any(|known| known.number == number) {
            return true;
        }
        self.interfaces.push(TrackedInterface {
            number,
            bound: None,
        });
        true
    }

    /// Record a bind outcome for one interface; false when unknown.
    ///
    /// C: the bind callback runs per interface during
    /// `devman_usb_device_add` (`usb.c:219-...`).
    pub fn bind(&mut self, number: u8, outcome: BindOutcome) -> bool {
        match self
            .interfaces
            .iter_mut()
            .find(|known| known.number == number)
        {
            Some(interface) => {
                interface.bound = match outcome {
                    BindOutcome::Bound(endpoint) => Some(endpoint),
                    BindOutcome::Refused => None,
                };
                true
            }
            None => false,
        }
    }

    /// Interfaces still unbound.
    pub fn unbound(&self) -> usize {
        self.interfaces
            .iter()
            .filter(|known| known.bound.is_none())
            .count()
    }
}

/// Tracker for all USB devices of one driver.
#[derive(Debug, Default, Clone)]
pub struct UsbTracker {
    devices: alloc::vec::Vec<Option<UsbDevice>>,
}

impl UsbTracker {
    /// Empty tracker.
    pub fn new() -> UsbTracker {
        UsbTracker {
            devices: alloc::vec::Vec::new(),
        }
    }

    /// Track a new device; returns its handle, or `None` when full.
    ///
    /// C: `devman_usb_device_new` plus `devman_usb_device_add`
    /// (`usb.c:62-...`).
    pub fn add(&mut self, device: UsbDevice) -> Option<usize> {
        if let Some(index) = self.devices.iter().position(|slot| slot.is_none()) {
            self.devices[index] = Some(device);
            return Some(index);
        }
        if self.devices.len() >= MAX_USB_DEVICES {
            return None;
        }
        self.devices.push(Some(device));
        Some(self.devices.len() - 1)
    }

    /// Detach a device (interfaces forgotten with it); false when empty.
    ///
    /// C: `devman_usb_device_remove` (`usb.c:276-...`).
    pub fn remove(&mut self, handle: usize) -> bool {
        match self.devices.get_mut(handle) {
            Some(slot) if slot.is_some() => {
                *slot = None;
                true
            }
            _ => false,
        }
    }

    /// Borrow a device by handle.
    pub fn get(&self, handle: usize) -> Option<&UsbDevice> {
        self.devices.get(handle).and_then(|slot| slot.as_ref())
    }

    /// Mutably borrow a device by handle.
    pub fn get_mut(&mut self, handle: usize) -> Option<&mut UsbDevice> {
        self.devices.get_mut(handle).and_then(|slot| slot.as_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interfaces_bind_and_count_unbound() {
        let mut device = UsbDevice::new(3);
        assert!(device.add_interface(0));
        assert!(device.add_interface(1));
        assert!(device.add_interface(0));
        assert_eq!(device.unbound(), 2);
        assert!(device.bind(0, BindOutcome::Bound(40)));
        assert_eq!(device.unbound(), 1);
        assert!(device.bind(1, BindOutcome::Refused));
        assert_eq!(device.unbound(), 1);
        assert!(!device.bind(9, BindOutcome::Refused));
    }

    #[test]
    fn test_tracker_add_remove_cycle() {
        let mut tracker = UsbTracker::new();
        let handle = tracker.add(UsbDevice::new(1)).unwrap();
        assert!(tracker.get(handle).is_some());
        assert!(tracker.remove(handle));
        assert!(!tracker.remove(handle));
    }

    #[test]
    fn test_interface_bound_is_thirty_two() {
        let mut device = UsbDevice::new(1);
        for number in 0..MAX_INTERFACES as u8 {
            assert!(device.add_interface(number));
        }
        assert!(!device.add_interface(32));
        assert_eq!(MAX_INTERFACES, 32);
    }
}

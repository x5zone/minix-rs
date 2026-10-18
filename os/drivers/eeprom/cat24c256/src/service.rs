//! Service assembly: startup, the bus-update subscription, and the pump.
//!
//! C correspondence: `sef_cb_init` (`cat24c256.c:420-462`: the device
//! geometry, the bus endpoint lookup, the device reservation, the
//! bus-update subscription, and the two announcements) and `main`
//! (`cat24c256.c:484-505`: the environment parse then
//! `blockdriver_task`). The loop shell and the RS birth handshake live in
//! the shared driver runtime; this module owns what the driver does
//! *before* the first receive and how a data-store notification is
//! handled on the way in.

use minix_blockdriver::pump::BlockService;
use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::{Endpoint, Message};

use crate::face::Cat24c256Face;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The cat24c256 service: the block pump plus the bus binding it
/// re-resolves when the bus driver comes back.
pub struct Cat24c256Service {
    pump: BlockService<Cat24c256Face>,
    bus: u32,
}

impl<T: DriverTransport> minix_driver_rt::runtime::DriverHandler<T> for Cat24c256Service {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

impl Cat24c256Service {
    /// A service over an already-started face.
    pub fn new(face: Cat24c256Face) -> Self {
        Cat24c256Service {
            bus: face.bus(),
            pump: BlockService::new(face),
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &Cat24c256Face {
        self.pump.face()
    }

    /// The face, mutably.
    pub fn face_mut(&mut self) -> &mut Cat24c256Face {
        self.pump.face_mut()
    }

    /// Handle one delivered message: a data-store notification refreshes
    /// the bus binding first, then everything goes to the shared pump.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) && msg.m_source == Endpoint::DS {
            self.refresh_bus(transport);
        }
        self.pump.dispatch(transport, msg);
    }

    /// Re-resolve the bus driver and re-reserve the device
    /// (C's bus-update subscription callback: the driver re-runs the two
    /// startup legs against the new endpoint).
    ///
    /// The subscription itself (C `i2cdriver_subscribe_bus_updates`) is a
    /// registered seam: the data store's subscribe call has no Rust verb
    /// yet, so this path runs when a notification arrives rather than
    /// being armed by the driver.
    fn refresh_bus<T: DriverTransport>(&mut self, transport: &mut T) {
        let Some(endpoint) = client::bus_endpoint(transport, self.bus) else {
            self.pump.face_mut().set_bus_endpoint(None);
            return;
        };
        let address = self.pump.face().address();
        if client::reserve(transport, endpoint, address).is_ok() {
            self.pump.face_mut().set_bus_endpoint(Some(endpoint));
        }
    }
}

/// Startup (C `sef_cb_init`, `cat24c256.c:420-462`): resolve the bus
/// driver, claim the device, and hand back the face for the pump.
///
/// The C init returns `EXIT_FAILURE` when the bus driver is missing or the
/// device cannot be claimed; the rewrite reports the errno and the binary
/// returns, which is the same fate RS observes from a failed fresh-start
/// init.
pub fn start<T: DriverTransport>(
    transport: &mut T,
    bus: u32,
    address: u16,
) -> Result<Cat24c256Face, i32> {
    if !crate::face::valid_address(address) {
        return Err(-minix_types::EINVAL);
    }
    let endpoint = client::bus_endpoint(transport, bus).ok_or(-minix_types::ENOENT)?;
    client::reserve(transport, endpoint, address)?;
    let mut face = Cat24c256Face::new(bus, address);
    face.set_bus_endpoint(Some(endpoint));
    Ok(face)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use minix_driver_rt::transport::DriverTransport;
    use minix_types::Message;

    /// A transport that serves the bus label and records reservations.
    struct FakeBus {
        bus_present: bool,
        reserves: Vec<(Endpoint, u16)>,
        replies: Vec<(Endpoint, i32)>,
    }

    impl FakeBus {
        fn new() -> Self {
            FakeBus {
                bus_present: true,
                reserves: Vec::new(),
                replies: Vec::new(),
            }
        }
    }

    impl DriverTransport for FakeBus {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            match msg.m_type {
                client::BUSC_I2C_RESERVE => {
                    // SAFETY(test): the reserve payload's address word.
                    let base = ::core::ptr::addr_of!(*msg) as *const u8;
                    let mut bytes = [0u8; 2];
                    unsafe {
                        ::core::ptr::copy_nonoverlapping(base.add(8), bytes.as_mut_ptr(), 2);
                    }
                    self.reserves.push((dst, u16::from_le_bytes(bytes)));
                    msg.m_type = 0;
                    Ok(())
                }
                _ => Ok(()),
            }
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.replies.push((dst, msg.m_type));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &mut [u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &[u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
        fn lookup_label(&mut self, label: &str) -> Option<Endpoint> {
            if self.bus_present && label == "i2c.1" {
                Some(Endpoint(6))
            } else {
                None
            }
        }
    }

    #[test]
    fn test_start_resolves_the_bus_and_reserves_the_device() {
        let mut transport = FakeBus::new();
        let face = start(&mut transport, 1, 0x50).expect("bus present");
        assert_eq!(face.bus_endpoint(), Some(Endpoint(6)));
        assert_eq!(face.bus(), 1);
        assert_eq!(face.address(), 0x50);
        assert_eq!(transport.reserves, alloc::vec![(Endpoint(6), 0x50)]);

        // A missing bus driver and an address outside the table both
        // refuse startup.
        let mut absent = FakeBus {
            bus_present: false,
            ..FakeBus::new()
        };
        assert_eq!(
            start(&mut absent, 1, 0x50).map(|_| ()),
            Err(-minix_types::ENOENT)
        );
        let mut transport = FakeBus::new();
        assert_eq!(
            start(&mut transport, 1, 0x60).map(|_| ()),
            Err(-minix_types::EINVAL)
        );
    }

    #[test]
    fn test_a_data_store_notification_refreshes_the_bus_binding() {
        let mut transport = FakeBus::new();
        let face = start(&mut transport, 1, 0x50).unwrap();
        let mut service = Cat24c256Service::new(face);
        // The bus goes away: the refresh clears the binding.
        service.face_mut().set_bus_endpoint(None);
        service.refresh_bus(&mut transport);
        assert_eq!(service.face().bus_endpoint(), Some(Endpoint(6)));
        assert_eq!(transport.reserves.len(), 2, "the refresh re-reserves");

        let mut absent = FakeBus {
            bus_present: false,
            ..FakeBus::new()
        };
        service.refresh_bus(&mut absent);
        assert_eq!(service.face().bus_endpoint(), None);
    }
}

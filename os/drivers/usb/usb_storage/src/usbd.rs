//! The usbd client seam: what the mass-storage driver asks of the host.
//!
//! C correspondence: the DDEkit USB client API the driver uses —
//! `ddekit_usb_init(&mass_storage_driver, ...)` with its three callbacks
//! (`usb_storage.c:121-124`) and the bulk submissions behind the BOT
//! phases (`send_scsi_cbw_out`, `usb_storage.c:230-259`, and the data and
//! CSW stages in `bulk.c`). The daemon end is `USB_RQ_*` / `USB_*`
//! (`minix-driver-usbd::protocol`, `minix-usb::wire`).

/// The driver's callbacks (`struct ddekit_usb_driver`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverEvent {
    /// One submitted request completed (`completion`).
    Completion,
    /// A device appeared (`connect`).
    Connect,
    /// The device vanished (`disconnect`).
    Disconnect,
}

/// What the mass-storage driver asks of usbd.
pub trait UsbdClient {
    /// Register the driver with the daemon (`ddekit_usb_init`).
    fn init(&mut self) -> Result<(), i32>;

    /// Submit one bulk transfer on an endpoint (`ddekit_usb_submit_urb`
    /// with a bulk EP configuration): the CBW out, the data stage, and
    /// the CSW in all ride this.
    fn submit_bulk(&mut self, endpoint: u8, data: &mut [u8], read: bool) -> Result<usize, i32>;

    /// Cancel a submitted transfer (`ddekit_usb_cancle_urb`).
    fn cancel(&mut self, id: u32) -> Result<(), i32>;

    /// Read the device descriptor (`ddekit_usb_get_device_desc`).
    fn device_descriptor(&mut self) -> Result<[u8; 18], i32>;
}

/// The seam's default: no daemon is reachable, so every request is refused
/// honestly.
pub struct UnboundUsbd;

impl UsbdClient for UnboundUsbd {
    fn init(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn submit_bulk(&mut self, _endpoint: u8, _data: &mut [u8], _read: bool) -> Result<usize, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn cancel(&mut self, _id: u32) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn device_descriptor(&mut self) -> Result<[u8; 18], i32> {
        Err(-minix_types::ENOSYS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_unbound_client_refuses_instead_of_faking_a_transfer() {
        let mut client = UnboundUsbd;
        assert_eq!(client.init(), Err(-minix_types::ENOSYS));
        let mut data = [0u8; 31];
        assert_eq!(
            client.submit_bulk(2, &mut data, false),
            Err(-minix_types::ENOSYS)
        );
        assert_eq!(client.cancel(1), Err(-minix_types::ENOSYS));
        assert_eq!(client.device_descriptor(), Err(-minix_types::ENOSYS));
    }
}

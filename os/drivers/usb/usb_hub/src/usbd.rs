//! The usbd client seam: what the hub driver asks of the host daemon.
//!
//! C correspondence: the DDEkit USB client API the hub uses — `ddekit_usb_init(&usb_driver, ...)`
//! with its three callbacks (`usb_hub.c:196-202`, `:323-360`) and the
//! URB submission (`ddekit_usb_submit_urb`, `:185-206` in usbd's
//! `hcd_ddekit.c`). The daemon end of the protocol is `USB_RQ_*` /
//! `USB_*` (`minix-driver-usbd::protocol`, `minix-usb::wire`).
//!
//! Production wires this to usbd over IPC (init, submit, cancel, and the
//! daemon's completion/announce/withdraw notifications); the seam keeps
//! the hub's round logic host-testable.

/// The hub driver's callbacks (`struct ddekit_usb_driver`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverEvent {
    /// One submitted request completed (`completion`).
    Completion,
    /// A device appeared (`connect`).
    Connect,
    /// The device vanished (`disconnect`).
    Disconnect,
}

/// What the hub asks of usbd.
pub trait UsbdClient {
    /// Register the hub driver with the daemon (`ddekit_usb_init`).
    fn init(&mut self) -> Result<(), i32>;

    /// Submit one control transfer (`ddekit_usb_submit_urb` with the
    /// control EP configuration): setup packet, data buffer, direction.
    fn submit_control(
        &mut self,
        setup: &[u8; 8],
        data: &mut [u8],
        read: bool,
    ) -> Result<usize, i32>;

    /// Cancel a submitted transfer (`ddekit_usb_cancle_urb`).
    fn cancel(&mut self, id: u32) -> Result<(), i32>;

    /// Read the class descriptor of the attached hub
    /// (`hub_get_descriptor`).
    fn hub_descriptor(&mut self) -> Result<[u8; 7], i32>;
}

/// The seam's default: no daemon is reachable, so every request is refused
/// honestly.
pub struct UnboundUsbd;

impl UsbdClient for UnboundUsbd {
    fn init(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn submit_control(
        &mut self,
        _setup: &[u8; 8],
        _data: &mut [u8],
        _read: bool,
    ) -> Result<usize, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn cancel(&mut self, _id: u32) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn hub_descriptor(&mut self) -> Result<[u8; 7], i32> {
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
        let mut data = [0u8; 4];
        assert_eq!(
            client.submit_control(&[0; 8], &mut data, true),
            Err(-minix_types::ENOSYS)
        );
        assert_eq!(client.cancel(1), Err(-minix_types::ENOSYS));
        assert_eq!(client.hub_descriptor(), Err(-minix_types::ENOSYS));
    }
}

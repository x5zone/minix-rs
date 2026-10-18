//! The host-controller seam: what usbd needs from the USB controller.
//!
//! C correspondence: the boundary between the generic host layer and the
//! hardware backend — `hcd/hcd.c` (1314 lines: the enumeration and
//! transfer state machine) and `hcd/hcd_common.c` (761 lines: device
//! management) drive the controller through the `musb/` backend
//! (mentor-graphics registers, DMA, and interrupts).
//!
//! # The DDEkit reading (adjudication, `2026-10-19`)
//!
//! The family's earlier note said "C goes through DDEkit's host
//! abstraction; the rewrite has no counterpart layer". Reading
//! `hcd/hcd_ddekit.c` (484 lines) shows the direction is the *other* way
//! round: that file **implements** the DDEkit USB client API
//! (`ddekit_usb_get_device_desc`, `ddekit_usb_submit_urb`,
//! `ddekit_usb_init`, ...) so that USB *client* drivers (usb_hub,
//! usb_storage, and DDEkit's own USB glue) can talk to usbd. The layer
//! *beneath* usbd is the controller itself.
//!
//! So the seam to model is the controller ([`HcdLeg`]), not DDEkit: the
//! DDEkit-facing half is usbd's own served protocol, which
//! [`crate::protocol`] and `minix-usb` already carry.

/// What usbd asks of the controller.
pub trait HcdLeg {
    /// Bring the controller up (`hcd_init`): clocks, the port scan.
    fn init(&mut self) -> Result<(), i32>;

    /// One control transfer (`hcd_control_transfer`): setup packet plus
    /// an optional data stage.
    fn control_transfer(
        &mut self,
        setup: &[u8; 8],
        data: &mut [u8],
        read: bool,
    ) -> Result<usize, i32>;

    /// One bulk transfer on an endpoint (`hcd_bulk_transfer`).
    fn bulk_transfer(&mut self, endpoint: u8, data: &mut [u8], read: bool) -> Result<usize, i32>;

    /// Cancel a transfer by its identifier (`hcd_cancel_urb`).
    fn cancel(&mut self, id: u32) -> Result<(), i32>;

    /// The port's device-presence state (`hcd_port_status`).
    fn port_connected(&mut self) -> bool;
}

/// The controller seam's default: nothing is bound, so every transfer is
/// refused honestly rather than reporting a completed request block.
pub struct UnboundHcd;

impl HcdLeg for UnboundHcd {
    fn init(&mut self) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn control_transfer(
        &mut self,
        _setup: &[u8; 8],
        _data: &mut [u8],
        _read: bool,
    ) -> Result<usize, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn bulk_transfer(
        &mut self,
        _endpoint: u8,
        _data: &mut [u8],
        _read: bool,
    ) -> Result<usize, i32> {
        Err(-minix_types::ENOSYS)
    }
    fn cancel(&mut self, _id: u32) -> Result<(), i32> {
        Err(-minix_types::ENOSYS)
    }
    fn port_connected(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_unbound_controller_refuses_instead_of_completing() {
        let mut hcd = UnboundHcd;
        assert_eq!(hcd.init(), Err(-minix_types::ENOSYS));
        let mut data = [0u8; 8];
        assert_eq!(
            hcd.control_transfer(&[0; 8], &mut data, true),
            Err(-minix_types::ENOSYS)
        );
        assert_eq!(
            hcd.bulk_transfer(1, &mut data, false),
            Err(-minix_types::ENOSYS)
        );
        assert_eq!(hcd.cancel(1), Err(-minix_types::ENOSYS));
        assert!(!hcd.port_connected());
    }
}

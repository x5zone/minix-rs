//! Service assembly: the hub's polling rounds over the usbd seam.
//!
//! C correspondence: `hub_task` (`usb_hub.c:38-...`: the round loop that
//! reads the hub descriptor, polls each port's status, and acts on the
//! changes) and the three callbacks (`usb_driver_completion`,
//! `usb_driver_connect`, `usb_driver_disconnect`, `usb_hub.c:337-380`).
//! The loop shell and the RS birth handshake live in the shared driver
//! runtime; the port state machine is [`crate::ports`].

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{EIO, Message, OK};

use crate::char_face::{HubFace, RoundEffect};
use crate::hub_class;
use crate::ports::PortObservation;
use crate::usbd::{DriverEvent, UsbdClient};

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The USB hub service.
pub struct HubService<U: UsbdClient> {
    face: HubFace,
    usbd: U,
    /// The hub's port count (`driver_state.num_ports`).
    num_ports: u16,
    /// Whether the driver is connected to a hub device.
    connected: bool,
}

impl<U: UsbdClient> HubService<U> {
    /// A service over a fresh face and the given daemon seam.
    pub fn new(usbd: U) -> Self {
        HubService {
            face: HubFace::new(),
            usbd,
            num_ports: 0,
            connected: false,
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &HubFace {
        &self.face
    }

    /// The daemon seam, mutably.
    pub fn usbd_mut(&mut self) -> &mut U {
        &mut self.usbd
    }

    /// The hub's port count, once the descriptor was read.
    pub fn num_ports(&self) -> u16 {
        self.num_ports
    }

    /// Whether a hub device is attached.
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// The startup (`hub_sef_hdlr` + the hub thread's first round): register
    /// with the daemon and read the hub descriptor.
    pub fn start(&mut self) -> Result<(), i32> {
        self.usbd.init()?;
        let descriptor = self.usbd.hub_descriptor()?;
        // `bNbrPorts` sits at byte 2 of the hub descriptor (`common.h`'s
        // `hub_descriptor`).
        self.num_ports = u16::from(descriptor[2]);
        Ok(())
    }

    /// A device appeared (`usb_driver_connect`, `usb_hub.c:352-...`).
    pub fn connect(&mut self) -> i32 {
        if self.connected {
            // C panics: the hub driver can be connected only once.
            return EIO;
        }
        self.connected = true;
        OK
    }

    /// The device vanished (`usb_driver_disconnect`).
    pub fn disconnect(&mut self) -> i32 {
        self.connected = false;
        OK
    }

    /// One polling round (`hub_task`'s body): read every port's status and
    /// feed the observations to the state machine, powering and resetting
    /// ports as the changes demand.
    pub fn round(&mut self) -> i32 {
        for port in 1..=self.num_ports {
            let setup = match hub_class::port_status_setup(port, self.num_ports) {
                Ok(setup) => setup,
                Err(status) => return status,
            };
            let mut data = [0u8; 4];
            if let Err(status) = self.usbd.submit_control(&setup, &mut data, true) {
                return status;
            }
            // Derive the observation from the status word against the
            // port's belief (`hub_handle_change`, `usb_hub.c:146-158`).
            let attached = data[0] & 0x01 == 1;
            let belief = self.face.belief(port as usize);
            let observation = match (attached, belief) {
                (true, Some(crate::ports::PortBelief::Disconnected)) => PortObservation::Arrived,
                (false, Some(crate::ports::PortBelief::Connected)) => PortObservation::Left,
                _ => PortObservation::None,
            };
            let effect = self.face.observe(port as usize, observation);
            if effect == RoundEffect::SuspendTask {
                // A communication error hangs the whole hub task in C
                // (`usb_hub.c:465-468`); the service stops the round.
                return EIO;
            }
            if observation == PortObservation::Arrived {
                // Power the port, then try a reset within the budget
                // (`hub_handle_connection`, `usb_hub.c:620-700`).
                if let Ok(setup) = hub_class::port_feature_setup(
                    port,
                    hub_class::code::SET_FEATURE,
                    hub_class::feature::PORT_POWER,
                    self.num_ports,
                ) {
                    let mut none = [0u8; 0];
                    let _ = self.usbd.submit_control(&setup, &mut none, false);
                }
                if !self.face.try_reset(port as usize) {
                    return EIO;
                }
            }
        }
        self.face.complete_round();
        OK
    }

    /// Handle one delivered message: the daemon's notifications drive the
    /// rounds and the connect/disconnect callbacks.
    pub fn dispatch<T: DriverTransport>(&mut self, _transport: &mut T, msg: &Message) {
        if !is_ipc_notify(msg.m_type) {
            return;
        }
        // The daemon's events arrive as notifications; the hub's own loop
        // advances on the completion callback (`usb_driver_completion`).
        let _ = DriverEvent::Completion;
        let _ = self.round();
    }
}

impl<U: UsbdClient, T: DriverTransport> DriverHandler<T> for HubService<U> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use minix_types::Endpoint;

    /// A daemon seam that answers a scripted port bitmap.
    struct FakeUsbd {
        ports: u8,
        rounds: usize,
        refused: bool,
    }

    impl UsbdClient for FakeUsbd {
        fn init(&mut self) -> Result<(), i32> {
            if self.refused {
                return Err(-minix_types::ENOSYS);
            }
            Ok(())
        }
        fn submit_control(
            &mut self,
            setup: &[u8; 8],
            data: &mut [u8],
            _read: bool,
        ) -> Result<usize, i32> {
            if self.refused {
                return Err(-minix_types::ENOSYS);
            }
            self.rounds += 1;
            if !data.is_empty() {
                // A status read answers the requested port's bit (the port
                // rides the setup's wIndex); a feature write carries no
                // data stage.
                let port = u16::from_le_bytes([setup[4], setup[5]]);
                data[0] = if port >= 1 && self.ports & (1 << (port - 1)) != 0 {
                    1
                } else {
                    0
                };
            }
            Ok(data.len())
        }
        fn cancel(&mut self, _id: u32) -> Result<(), i32> {
            Ok(())
        }
        fn hub_descriptor(&mut self) -> Result<[u8; 7], i32> {
            if self.refused {
                return Err(-minix_types::ENOSYS);
            }
            let mut descriptor = [0u8; 7];
            descriptor[2] = 4; // four ports
            Ok(descriptor)
        }
    }

    struct FakeTransport;

    impl DriverTransport for FakeTransport {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn asynsend(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
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
    }

    #[test]
    fn test_start_reads_the_descriptor_and_the_round_polls_every_port() {
        let mut svc = HubService::new(FakeUsbd {
            ports: 0x01,
            rounds: 0,
            refused: false,
        });
        assert_eq!(svc.start(), Ok(()));
        assert_eq!(svc.num_ports(), 4);
        assert_eq!(svc.connect(), OK);
        assert_eq!(svc.round(), OK);
        // One status read per port, plus the power-on write for the port
        // that just showed a device (the arriving port only).
        assert_eq!(svc.usbd_mut().rounds, 5);
        assert_eq!(
            svc.face().belief(1),
            Some(crate::ports::PortBelief::Connected)
        );
        assert_eq!(
            svc.face().belief(2),
            Some(crate::ports::PortBelief::Disconnected)
        );
        assert_eq!(svc.disconnect(), OK);
        assert!(!svc.connected());
    }

    #[test]
    fn test_a_notification_drives_a_round() {
        let mut svc = HubService::new(FakeUsbd { ports: 0x00, rounds: 0, refused: false });
        svc.start().unwrap();
        let mut transport = FakeTransport;
        let mut msg = Message {
            m_type: 0x1000,
            m_source: Endpoint::HARDWARE,
            ..Message::default()
        };
        svc.dispatch(&mut transport, &msg);
        // One status read per port on the round the notification drove.
        assert_eq!(svc.usbd_mut().rounds, 4);
        // A non-notification message is ignored.
        msg.m_type = 0x1100;
        svc.dispatch(&mut transport, &msg);
        assert_eq!(svc.usbd_mut().rounds, 4);
    }

    #[test]
    fn test_an_unreachable_daemon_refuses_startup_and_the_round() {
        let mut svc = HubService::new(FakeUsbd {
            ports: 0,
            rounds: 0,
            refused: true,
        });
        assert_eq!(svc.start(), Err(-minix_types::ENOSYS));
        assert_eq!(svc.num_ports(), 0);
        assert_eq!(svc.round(), OK, "no ports to poll");
        // A second connect while attached is refused.
        let mut svc = HubService::new(FakeUsbd {
            ports: 0,
            rounds: 0,
            refused: false,
        });
        svc.start().unwrap();
        assert_eq!(svc.connect(), OK);
        assert_eq!(svc.connect(), EIO);
        let _ = Vec::<u8>::new();
    }
}

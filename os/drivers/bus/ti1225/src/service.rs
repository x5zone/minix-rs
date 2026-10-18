//! Service assembly: the TI1225 CardBus controller driver.
//!
//! C correspondence: `sef_cb_init_fresh` (`ti1225.c:87-115`: probe, then
//! `hw_init`), `hw_probe`/`hw_init` (`ti1225.c:117-240`: bus numbers,
//! clearing `CBB_BC_INTEXCA`, enabling the socket masks, one `do_int`
//! pass), `do_int` (`ti1225.c:246-431`: the functional-interrupt clear,
//! the power negotiation, the event clear, the power-cycle wait, the
//! reset pulse and the secondary-bus rescan), and the main loop
//! (`ti1225.c:46-64`) — which prints each message and drops it: the C
//! driver has no client protocol at all, and with `USE_INTS 0` no
//! interrupt notifications arrive. The Rust pump therefore matches C by
//! staying inert, and the binary runs an announce-free serve
//! ([`minix_driver_rt::runtime::DriverRuntime::serve_quiet`]) because C's
//! startup publishes nothing.
//!
//! Registered seams: the instance/debug environment parsing and
//! `tsc_calibrate` (`ti1225.c:91-99`) ride their own seams; the interrupt
//! path is off in C (`USE_INTS 0`), so none is wired here; the power-cycle
//! wait is bounded by [`POWER_CYCLE_POLLS`] register reads instead of C's
//! hundred-millisecond spin, because the uptime source rides the clock
//! seam.

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::Message;

use crate::controller::CardBusController;
use crate::registers::{
    csr, CBB_BC_CRST, CBB_BC_INTEXCA, CBB_BRIDGECTRL, CM_CDMASK, CM_CSTSMASK, CM_PWRMASK,
    CP_PWRCYCLE, TI_CARD_CTRL, TI_CB_BUS_NR, TI_CCR_IFG,
};
use crate::socket::{power_outcome, with_vcc, SocketOutcome};

/// Reads spent waiting for the power cycle (`do_int`'s 100 ms spin).
pub const POWER_CYCLE_POLLS: usize = 1000;

/// The CardBus controller service.
pub struct Ti1225Service<C: CardBusController> {
    controller: C,
    /// The bridge's secondary bus number (`p_cb_busnr`), read at init.
    cardbus_bus: u8,
}

impl<C: CardBusController> Ti1225Service<C> {
    /// A service over the given controller.
    pub fn new(controller: C) -> Self {
        Ti1225Service {
            controller,
            cardbus_bus: 0,
        }
    }

    /// The controller seam (tests inspect the register traffic).
    pub fn controller(&self) -> &C {
        &self.controller
    }

    /// C's fresh-start init: probe, bridge setup, masks, one socket pass.
    pub fn start(&mut self) -> Result<(), i32> {
        self.controller.probe()?;
        self.cardbus_bus = self.controller.config_read8(TI_CB_BUS_NR);

        // Clear CBB_BC_INTEXCA (`ti1225.c:213-218`).
        let mut bridge = self.controller.config_read16(CBB_BRIDGECTRL);
        bridge &= !CBB_BC_INTEXCA;
        self.controller.config_write16(CBB_BRIDGECTRL, bridge);

        // Enable socket interrupts (`ti1225.c:230-231`).
        let mask = self.controller.csr_read(csr::MASK);
        self.controller
            .csr_write(csr::MASK, mask | CM_PWRMASK | CM_CDMASK | CM_CSTSMASK);

        self.socket_pass();
        Ok(())
    }

    /// One `do_int` pass (`ti1225.c:246-431`).
    fn socket_pass(&mut self) {
        self.clear_functional_interrupt();

        let present = self.controller.csr_read(csr::PRESENT);
        let control = self.controller.csr_read(csr::CONTROL);
        let choice = match power_outcome(present) {
            SocketOutcome::Apply(choice) => choice,
            // No card, not a card, already powered, or no common supply:
            // C returns without touching the socket.
            _ => return,
        };
        self.controller
            .csr_write(csr::CONTROL, with_vcc(control, choice));

        let event = self.controller.csr_read(csr::EVENT);
        if event != 0 {
            self.controller.csr_write(csr::EVENT, event);
        }
        self.clear_functional_interrupt();

        // Wait for the power cycle (`do_int`'s bounded spin).
        let mut present = self.controller.csr_read(csr::PRESENT);
        for _ in 0..POWER_CYCLE_POLLS {
            if present & CP_PWRCYCLE != 0 {
                break;
            }
            present = self.controller.csr_read(csr::PRESENT);
        }
        if present & CP_PWRCYCLE == 0 {
            return;
        }

        // Reset the device: assert the reset line, wait, clear it, wait
        // (`ti1225.c:405-424`).
        let bridge = self.controller.config_read16(CBB_BRIDGECTRL);
        self.controller
            .config_write16(CBB_BRIDGECTRL, bridge | CBB_BC_CRST);
        self.controller.delay_micros(1);
        let bridge = self.controller.config_read16(CBB_BRIDGECTRL);
        self.controller
            .config_write16(CBB_BRIDGECTRL, bridge & !CBB_BC_CRST);
        self.controller.delay_micros(1);

        self.controller.rescan_bus(self.cardbus_bus);
    }

    /// Clear the functional-interrupt flag when the card control register
    /// carries it (C writes the read value back).
    fn clear_functional_interrupt(&mut self) {
        let card = self.controller.config_read8(TI_CARD_CTRL);
        if card & TI_CCR_IFG != 0 {
            self.controller.config_write8(TI_CARD_CTRL, card);
        }
    }

    /// Handle one delivered message.
    ///
    /// C's loop prints and drops everything (`ti1225.c:46-64`); with
    /// `USE_INTS 0` no interrupt notifications arrive either, so the pump
    /// has no road to run.
    pub fn dispatch<T: DriverTransport>(&mut self, _transport: &mut T, _msg: &Message) {}
}

impl<C: CardBusController, T: DriverTransport> DriverHandler<T> for Ti1225Service<C> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    use crate::registers::{
        CC_VCC_5V, CC_VCC_YV, CM_PWRMASK, CP_5VCARD, CP_5VSOCKET, CP_CDETECT1, CP_PWRCYCLE,
    };

    /// Scripted controller: registers plus every write recorded.
    struct FakeController {
        bridge: u16,
        mask: u32,
        present: u32,
        control: u32,
        event: u32,
        card_ctrl: u8,
        bus: u8,
        /// Set when the socket control write happens (the power gesture).
        power_on_control_write: bool,
        writes8: Vec<(u8, u8)>,
        writes16: Vec<(u8, u16)>,
        csr_writes: Vec<(usize, u32)>,
        rescans: Vec<u8>,
        delays: Vec<u32>,
    }

    impl FakeController {
        fn card_ready() -> Self {
            FakeController {
                bridge: CBB_BC_INTEXCA,
                mask: 0x10,
                present: CP_5VCARD | CP_5VSOCKET,
                control: CC_VCC_YV,
                event: 0x02,
                card_ctrl: TI_CCR_IFG,
                bus: 3,
                power_on_control_write: true,
                writes8: Vec::new(),
                writes16: Vec::new(),
                csr_writes: Vec::new(),
                rescans: Vec::new(),
                delays: Vec::new(),
            }
        }
    }

    impl CardBusController for FakeController {
        fn probe(&mut self) -> Result<(), i32> {
            Ok(())
        }
        fn config_read8(&mut self, offset: u8) -> u8 {
            match offset {
                TI_CB_BUS_NR => self.bus,
                TI_CARD_CTRL => self.card_ctrl,
                _ => 0,
            }
        }
        fn config_read16(&mut self, offset: u8) -> u16 {
            match offset {
                CBB_BRIDGECTRL => self.bridge,
                _ => 0,
            }
        }
        fn config_write8(&mut self, offset: u8, value: u8) {
            if offset == TI_CARD_CTRL {
                // Writing the register back clears the flag (the C
                // hardware behavior the second pass then finds clear).
                self.card_ctrl &= !TI_CCR_IFG;
            }
            self.writes8.push((offset, value));
        }
        fn config_write16(&mut self, offset: u8, value: u16) {
            if offset == CBB_BRIDGECTRL {
                self.bridge = value;
            }
            self.writes16.push((offset, value));
        }
        fn csr_read(&mut self, offset: usize) -> u32 {
            match offset {
                csr::MASK => self.mask,
                csr::PRESENT => self.present,
                csr::CONTROL => self.control,
                csr::EVENT => self.event,
                _ => 0,
            }
        }
        fn csr_write(&mut self, offset: usize, value: u32) {
            match offset {
                csr::MASK => self.mask = value,
                csr::CONTROL => {
                    self.control = value;
                    if self.power_on_control_write {
                        self.present |= CP_PWRCYCLE;
                    }
                }
                csr::EVENT => self.event = 0,
                _ => {}
            }
            self.csr_writes.push((offset, value));
        }
        fn rescan_bus(&mut self, bus: u8) {
            self.rescans.push(bus);
        }
        fn delay_micros(&mut self, micros: u32) {
            self.delays.push(micros);
        }
    }

    /// Minimal transport for the inert dispatch test.
    struct NullTransport;

    impl DriverTransport for NullTransport {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: minix_types::Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: minix_types::Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn asynsend(&mut self, _dst: minix_types::Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: minix_types::Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &mut [u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _granter: minix_types::Endpoint,
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
    fn test_start_clears_intexca_masks_and_powers_the_socket() {
        let mut service = Ti1225Service::new(FakeController::card_ready());
        assert_eq!(service.start(), Ok(()));
        let controller = service.controller();

        // Bridge control: INTEXCA cleared once at init, then the CRST
        // pulse (set, then clear).
        assert_eq!(
            controller.writes16,
            vec![
                (CBB_BRIDGECTRL, 0x00),
                (CBB_BRIDGECTRL, CBB_BC_CRST),
                (CBB_BRIDGECTRL, 0x00),
            ]
        );
        // The mask gains the three enable bits.
        assert_eq!(controller.mask, 0x10 | CM_PWRMASK | CM_CDMASK | CM_CSTSMASK);
        // The voltage field is replaced with 5 V.
        assert_eq!(controller.control, CC_VCC_5V);
        // The socket event was cleared.
        assert!(controller.csr_writes.contains(&(csr::EVENT, 0x02)));
        // The functional interrupt flag was written back.
        assert_eq!(controller.writes8, vec![(TI_CARD_CTRL, TI_CCR_IFG)]);
        // The reset pulse waited twice and the secondary bus was rescanned.
        assert_eq!(controller.delays, vec![1, 1]);
        assert_eq!(controller.rescans, vec![3]);
    }

    #[test]
    fn test_no_card_leaves_the_socket_alone() {
        let mut controller = FakeController::card_ready();
        controller.present = CP_CDETECT1;
        let mut service = Ti1225Service::new(controller);
        assert_eq!(service.start(), Ok(()));
        let controller = service.controller();
        assert!(
            !controller
                .csr_writes
                .iter()
                .any(|(offset, _)| *offset == csr::CONTROL),
            "no control write without a card"
        );
        assert!(controller.rescans.is_empty());
        assert!(controller.delays.is_empty());
    }

    #[test]
    fn test_incompatible_supply_leaves_the_socket_alone() {
        let mut controller = FakeController::card_ready();
        controller.present = CP_5VCARD; // socket cannot do 5 V
        let mut service = Ti1225Service::new(controller);
        assert_eq!(service.start(), Ok(()));
        let controller = service.controller();
        assert!(
            !controller
                .csr_writes
                .iter()
                .any(|(offset, _)| *offset == csr::CONTROL)
        );
        assert!(controller.rescans.is_empty());
    }

    #[test]
    fn test_null_controller_refuses_and_dispatch_is_inert() {
        let mut service = crate::init();
        assert_eq!(service.start(), Err(-minix_types::ENODEV));

        let mut service = Ti1225Service::new(FakeController::card_ready());
        let mut transport = NullTransport;
        let msg = Message::default();
        service.dispatch(&mut transport, &msg);
        // C prints and drops every message; nothing observable happens.
        assert_eq!(service.controller().rescans.len(), 0);
    }
}

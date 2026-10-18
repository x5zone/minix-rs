//! Service assembly: the TPS65217 PMIC driver.
//!
//! C correspondence: `sef_cb_init` (`tps65217.c:267-320`: bus lookup,
//! reservation, `check_revision`, `intr_enable` — including the
//! interrupt-register read that clears pending bits — and
//! `enable_pwr_off`) and the main loop (`tps65217.c:332-395`): only
//! notifications are acted on — a data-store notification refreshes the
//! bus endpoint, a hardware notification runs `intr_handler`
//! (`tps65217.c:225-256`: read the interrupt register; the power-button
//! bit tells init with `SIGUSR1`, anything else merely re-enables the
//! interrupt) — while every other message is logged and dropped. The
//! loop shell and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`].
//!
//! Registered seams: the environment plumbing (bus and address defaults),
//! the bus-restart subscription, and the interrupt re-enable call
//! (`sys_irqenable`) are not wired; the signal to init rides
//! [`minix_driver_rt::transport::DriverTransport::signal`].

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::{Endpoint, Message, EIO};

use crate::device::{chip_recognized, interrupt_of, Interrupt};
use crate::registers::{CHIPID_REG, DEFAULT_INT_MASK, INT_REG, PWR_OFF_MASK, STATUS_REG};

/// The bus number (`bus=1` in C's environment; the generic build carries
/// no environment, so the documented default stands).
pub const BUS_NUMBER: u32 = 1;

/// The device address (the only value in C's accepted-address table:
/// `0x24`).
pub const DEVICE_ADDRESS: u16 = 0x24;

/// Init's process id: the power button tells process 1 (`kill(1,
/// SIGUSR1)`).
pub const INIT_PID: i32 = 1;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The PMIC service: bus binding plus the notification pump.
pub struct Tps65217Service {
    bus: Option<Endpoint>,
    address: u16,
}

impl Tps65217Service {
    /// A service with no bus yet.
    pub fn new() -> Self {
        Tps65217Service {
            bus: None,
            address: DEVICE_ADDRESS,
        }
    }

    /// C's fresh-start init: bus lookup, reservation, chip check,
    /// interrupt setup and the power-off enable.
    pub fn start<T: DriverTransport>(&mut self, transport: &mut T) -> Result<(), i32> {
        let endpoint = client::bus_endpoint(transport, BUS_NUMBER).ok_or(-minix_types::ENOENT)?;
        client::reserve(transport, endpoint, self.address)?;

        let chipid = client::read_byte(transport, endpoint, self.address, CHIPID_REG)?;
        if !chip_recognized(chipid) {
            return Err(-EIO);
        }

        // Enable the power-button interrupt and clear any pending bits by
        // reading the interrupt register back.
        client::write_byte(transport, endpoint, self.address, INT_REG, DEFAULT_INT_MASK)?;
        let _pending = client::read_byte(transport, endpoint, self.address, INT_REG)?;

        // Enable power-off through the status register so the kernel can
        // cut power later.
        client::write_byte(transport, endpoint, self.address, STATUS_REG, PWR_OFF_MASK)?;

        self.bus = Some(endpoint);
        Ok(())
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if !is_ipc_notify(msg.m_type) {
            // Any other message is logged and dropped, as in C.
            return;
        }
        if msg.m_source == Endpoint::DS {
            self.refresh_bus(transport);
        } else if msg.m_source == Endpoint::HARDWARE {
            self.handle_interrupt(transport);
        }
    }

    /// Read the interrupt status and act (`intr_handler`).
    fn handle_interrupt<T: DriverTransport>(&mut self, transport: &mut T) {
        let Some(bus) = self.bus else {
            return;
        };
        let Ok(value) = client::read_byte(transport, bus, self.address, INT_REG) else {
            return;
        };
        match interrupt_of(value) {
            Interrupt::PowerButton => {
                // C `kill(1, SIGUSR1)`; a failed signal is logged there,
                // and the transport reports it the same way here.
                let _ = transport.signal(INIT_PID, minix_types::SIGNAL_USER_1);
            }
            // The interrupt re-enable (`sys_irqenable`) is a registered
            // seam; nothing else is observable.
            Interrupt::Other => {}
        }
    }

    /// Re-resolve the bus endpoint after a data-store notification.
    fn refresh_bus<T: DriverTransport>(&mut self, transport: &mut T) {
        let Some(old) = self.bus else {
            return;
        };
        if let Some(new) = client::bus_endpoint(transport, BUS_NUMBER)
            && new != old
            && client::reserve(transport, new, self.address).is_ok()
        {
            self.bus = Some(new);
        }
    }
}

impl Default for Tps65217Service {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: DriverTransport> DriverHandler<T> for Tps65217Service {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::vec;
    use std::vec::Vec;

    use crate::registers::{PBI_MASK, TPS65217C_CHIP_MASK};

    /// Scripted bus: a register file plus recorded signal calls.
    struct FakeBus {
        endpoint: Endpoint,
        bus_present: bool,
        registers: HashMap<u8, u8>,
        writes: Vec<(u8, u8)>,
        signals: Vec<(i32, i32)>,
        block: u64,
    }

    impl FakeBus {
        fn new(chipid: u8) -> Self {
            let mut registers = HashMap::new();
            registers.insert(CHIPID_REG, chipid);
            FakeBus {
                endpoint: Endpoint(42),
                bus_present: true,
                registers,
                writes: Vec::new(),
                signals: Vec::new(),
                block: 0,
            }
        }
    }

    impl DriverTransport for FakeBus {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            match msg.m_type {
                client::BUSC_I2C_RESERVE => {
                    msg.m_type = 0;
                    Ok(())
                }
                client::BUSC_I2C_EXEC => {
                    let block = self.block as *mut client::ExecBlock;
                    // SAFETY: the test granted its own exec block.
                    let block = unsafe { &mut *block };
                    match block.op {
                        client::I2C_OP_READ_WITH_STOP => {
                            block.buf[0] = self
                                .registers
                                .get(&block.cmd[0])
                                .copied()
                                .unwrap_or(0);
                        }
                        client::I2C_OP_WRITE_WITH_STOP => {
                            self.writes.push((block.buf[0], block.buf[1]));
                            self.registers.insert(block.buf[0], block.buf[1]);
                        }
                        _ => {}
                    }
                    msg.m_type = 0;
                    Ok(())
                }
                _ => Ok(()),
            }
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
        fn lookup_label(&mut self, label: &str) -> Option<Endpoint> {
            if self.bus_present && label == "i2c.1" {
                Some(self.endpoint)
            } else {
                None
            }
        }
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            self.block = address;
            Ok(1)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
        fn signal(&mut self, pid: i32, signal: i32) -> Result<(), i32> {
            self.signals.push((pid, signal));
            Ok(())
        }
    }

    fn notification(source: Endpoint) -> Message {
        Message {
            m_type: 0x1000,
            m_source: source,
            ..Message::default()
        }
    }

    #[test]
    fn test_start_checks_the_chip_and_sets_the_registers() {
        let mut bus = FakeBus::new(TPS65217C_CHIP_MASK | 0x02);
        let mut service = Tps65217Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        assert_eq!(
            bus.writes,
            vec![(INT_REG, DEFAULT_INT_MASK), (STATUS_REG, PWR_OFF_MASK)]
        );

        let mut wrong = FakeBus::new(0x00);
        let mut service = Tps65217Service::new();
        assert_eq!(service.start(&mut wrong), Err(-EIO));

        let mut no_bus = FakeBus::new(TPS65217C_CHIP_MASK);
        no_bus.bus_present = false;
        let mut service = Tps65217Service::new();
        assert_eq!(service.start(&mut no_bus), Err(-minix_types::ENOENT));
    }

    #[test]
    fn test_power_button_tells_init_and_other_status_does_not() {
        let mut bus = FakeBus::new(TPS65217C_CHIP_MASK);
        let mut service = Tps65217Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));

        bus.registers.insert(INT_REG, PBI_MASK);
        service.dispatch(&mut bus, &notification(Endpoint::HARDWARE));
        assert_eq!(
            bus.signals,
            vec![(INIT_PID, minix_types::SIGNAL_USER_1)]
        );

        bus.registers.insert(INT_REG, 0x02);
        service.dispatch(&mut bus, &notification(Endpoint::HARDWARE));
        assert_eq!(bus.signals.len(), 1, "no second signal");

        // Non-notify messages are dropped silently, as in C.
        let stray = Message {
            m_type: 0x2222,
            m_source: Endpoint(7),
            ..Message::default()
        };
        service.dispatch(&mut bus, &stray);
        assert_eq!(bus.signals.len(), 1);
    }

    #[test]
    fn test_missing_hooks_are_inert() {
        let mut bus = FakeBus::new(TPS65217C_CHIP_MASK);
        let mut service = Tps65217Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        // A character request decodes but the C driver has no such hooks:
        // the pump drops it without a reply or a write.
        let before = bus.writes.len();
        let msg = Message {
            m_type: 0x480 + 2, // CDEV_READ (base plus two)
            m_source: Endpoint(7),
            ..Message::default()
        };
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.writes.len(), before);
        assert!(bus.signals.is_empty());
    }
}

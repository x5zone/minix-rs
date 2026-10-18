//! Service assembly: the AMD DEV (IOMMU) driver.
//!
//! C correspondence: `sef_cb_init_fresh` (`amddev.c:118-170`: find the
//! device, decode `DEVF_CAP`, init domain zero and map zero, and write
//! the control value) and `main` (`amddev.c:69-98`): every iteration
//! reports exceptions first, an `IOMMU_MAP` request runs `do_add4pci`
//! (`amddev.c:375-432`) and is answered by sending the request message
//! back with the result in its type, and every other message is printed
//! and dropped. The loop shell and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`]; C's startup announces
//! nothing, so the binary uses the announce-free serve path.
//!
//! Registered seams: the signal handler that would delete mappings on
//! `SIGTERM` is disabled in C (`#if 0`, `amddev.c:171-202`) and stays
//! unwired here; the PCI capability walk and the translation call are the
//! [`IommuController`] trait.

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Message, EINVAL, OK};

use crate::controller::IommuController;
use crate::device;
use crate::registers::{
    CONTROL_VALUE, DEVF_BASE_HI, DEVF_BASE_LO, DEVF_CAP, DEVF_CR, DEVF_ERR_ADDR_HI,
    DEVF_ERR_ADDR_LO, DEVF_ERR_STATUS, DEVF_MAP, IOMMU_MAP,
};

/// The message union payload begins at byte 8 of [`Message`].
const PAYLOAD: usize = 8;

/// Page size the alignment gates use (`PAGE_SIZE`).
pub const PAGE_SIZE: u64 = 4096;

fn read_u64(msg: &Message, offset: usize) -> u64 {
    let base = ::core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; 8];
    // SAFETY: `Message` is a `#[repr(C)]` plain-old-data union; the
    // request's first two lanes are the 64-bit start and size.
    unsafe {
        ::core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), 8);
    }
    u64::from_le_bytes(bytes)
}

/// The DEV service: the controller seam plus the request pump.
pub struct AmddevService<C: IommuController> {
    controller: C,
}

impl<C: IommuController> AmddevService<C> {
    /// A service over the given controller.
    pub fn new(controller: C) -> Self {
        AmddevService { controller }
    }

    /// The controller seam (tests inspect the register traffic).
    pub fn controller(&self) -> &C {
        &self.controller
    }

    /// The controller seam, mutably (tests script the next status).
    pub fn controller_mut(&mut self) -> &mut C {
        &mut self.controller
    }

    /// C's fresh-start init: probe, capability decode, table setup, and
    /// the control write.
    pub fn start(&mut self) -> Result<(), i32> {
        self.controller.probe()?;

        // The capability word's three fields are diagnostic in C; the
        // decode is kept so the layout stays exercised.
        let capability = device::decode_capability(self.controller.read_reg(DEVF_CAP, 0));
        let _ = capability;
        // The error registers are read for the startup log.
        let _ = self.controller.read_reg(DEVF_ERR_STATUS, 0);
        let _ = self.controller.read_reg(DEVF_ERR_ADDR_LO, 0);
        let _ = self.controller.read_reg(DEVF_ERR_ADDR_HI, 0);

        // Domain zero: the table's bus address with the two low bits set.
        let table = self.controller.allocate_table()?;
        self.controller.write_reg(DEVF_BASE_HI, 0, 0);
        self.controller
            .write_reg(DEVF_BASE_LO, 0, (table as u32) | 3);

        // Map zero: domain one, bus seven, units nine and nine.
        self.controller
            .write_reg(DEVF_MAP, 0, device::init_map_value(1, 7, 9, 9));

        self.controller.write_reg(DEVF_CR, 0, CONTROL_VALUE);
        Ok(())
    }

    /// Report and clear a pending exception (`report_exceptions`).
    fn report_exceptions(&mut self) {
        let status = self.controller.read_reg(DEVF_ERR_STATUS, 0);
        if status & 0x8000_0000 != 0 {
            self.controller.write_reg(DEVF_ERR_STATUS, 0, 0);
        }
    }

    /// Handle one delivered message.
    ///
    /// The exception check runs first, as at the top of C's loop. An
    /// `IOMMU_MAP` is answered by sending the request's result back in
    /// the reply type (C sends the message itself back); any other
    /// message is dropped.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        self.report_exceptions();
        if msg.m_type != IOMMU_MAP {
            return;
        }
        let status = self.do_add4pci(msg);
        let mut reply = Message {
            m_type: status,
            ..Message::default()
        };
        let _ = transport.asynsend(msg.m_source, &mut reply);
    }

    /// `do_add4pci`: the alignment gates, the translation, and the
    /// exclusion-bit clearing.
    fn do_add4pci(&mut self, msg: &Message) -> i32 {
        let start = read_u64(msg, PAYLOAD);
        let size = read_u64(msg, PAYLOAD + 8);
        if !device::aligned(start, size, PAGE_SIZE) {
            return EINVAL;
        }
        let busaddr = match self.controller.translate(msg.m_source, start, size as usize) {
            Ok(busaddr) => busaddr,
            Err(status) => return status,
        };
        self.controller.clear_pages(busaddr, size as usize);
        OK
    }
}

impl<C: IommuController, T: DriverTransport> DriverHandler<T> for AmddevService<C> {
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
        DEVF_BASE_HI as BASE_HI, DEVF_BASE_LO as BASE_LO, DEVF_CAP as CAP, DEVF_CR as CR,
        DEVF_MAP as MAP,
    };
    use minix_types::Endpoint;

    /// Scripted controller: records register traffic and the map path.
    #[derive(Default)]
    struct FakeController {
        cap: u32,
        error_status: u32,
        table: u64,
        translate_to: u64,
        translate_fails: bool,
        writes: Vec<(u32, u32, u32)>,
        cleared: Vec<(u64, usize)>,
        translated: Vec<(Endpoint, u64, usize)>,
    }

    impl IommuController for FakeController {
        fn probe(&mut self) -> Result<(), i32> {
            Ok(())
        }
        fn read_reg(&mut self, function: u32, _index: u32) -> u32 {
            match function {
                CAP => self.cap,
                DEVF_ERR_STATUS => self.error_status,
                _ => 0,
            }
        }
        fn write_reg(&mut self, function: u32, index: u32, value: u32) {
            self.writes.push((function, index, value));
        }
        fn allocate_table(&mut self) -> Result<u64, i32> {
            let table = self.table;
            self.table = 0;
            Ok(table)
        }
        fn translate(&mut self, caller: Endpoint, start: u64, size: usize) -> Result<u64, i32> {
            self.translated.push((caller, start, size));
            if self.translate_fails {
                Err(-minix_types::EIO)
            } else {
                Ok(self.translate_to)
            }
        }
        fn clear_pages(&mut self, busaddr: u64, size: usize) {
            self.cleared.push((busaddr, size));
        }
    }

    /// Scripted transport: records replies.
    #[derive(Default)]
    struct Scripted {
        replies: Vec<(Endpoint, i32)>,
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
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
    }

    fn map_request(caller: Endpoint, start: u64, size: u64) -> Message {
        let mut msg = Message {
            m_type: IOMMU_MAP,
            m_source: caller,
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: test-only writers; the request's first two lanes are the
        // 64-bit start and size.
        unsafe {
            ::core::ptr::copy_nonoverlapping(start.to_le_bytes().as_ptr(), base.add(PAYLOAD), 8);
            ::core::ptr::copy_nonoverlapping(
                size.to_le_bytes().as_ptr(),
                base.add(PAYLOAD + 8),
                8,
            );
        }
        msg
    }

    #[test]
    fn test_start_sets_the_domain_map_and_control() {
        let mut service = AmddevService::new(FakeController {
            cap: (2 << 16) | (3 << 8) | 4,
            table: 0x1000,
            ..FakeController::default()
        });
        assert_eq!(service.start(), Ok(()));
        assert_eq!(
            service.controller().writes,
            vec![
                (BASE_HI, 0, 0),
                (BASE_LO, 0, 0x1003),
                (MAP, 0, 0x0410_7249),
                (CR, 0, CONTROL_VALUE),
            ]
        );
        assert_eq!(service.controller().writes.len(), 4);
    }

    #[test]
    fn test_null_controller_refuses_to_start() {
        let mut service = crate::init();
        assert_eq!(service.start(), Err(-minix_types::ENODEV));
    }

    #[test]
    fn test_map_request_translates_clears_and_replies() {
        let mut service = AmddevService::new(FakeController {
            translate_to: 0x8000,
            ..FakeController::default()
        });
        let mut transport = Scripted::default();
        let msg = map_request(Endpoint(5), 0x2000, 0x2000);
        service.dispatch(&mut transport, &msg);
        assert_eq!(service.controller().translated, vec![(Endpoint(5), 0x2000, 0x2000)]);
        assert_eq!(service.controller().cleared, vec![(0x8000, 0x2000)]);
        assert_eq!(transport.replies, vec![(Endpoint(5), OK)]);
    }

    #[test]
    fn test_misaligned_and_failed_translation() {
        let mut service = AmddevService::new(FakeController {
            translate_fails: true,
            ..FakeController::default()
        });
        let mut transport = Scripted::default();

        // A misaligned start never reaches the translation.
        let msg = map_request(Endpoint(5), 0x1001, 0x2000);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, EINVAL);
        assert!(service.controller().translated.is_empty());

        // A failing translation answers its error and clears nothing.
        let msg = map_request(Endpoint(5), 0x2000, 0x2000);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, -minix_types::EIO);
        assert!(service.controller().cleared.is_empty());
    }

    #[test]
    fn test_exceptions_are_cleared_and_other_messages_dropped() {
        let mut controller = FakeController {
            error_status: 0x8000_0001,
            ..FakeController::default()
        };
        controller.writes.clear();
        let mut service = AmddevService::new(controller);
        let mut transport = Scripted::default();

        // Any delivered message runs the exception check first: the set
        // status bit is cleared by writing zero.
        let stray = Message {
            m_type: 0x2222,
            m_source: Endpoint(7),
            ..Message::default()
        };
        service.dispatch(&mut transport, &stray);
        assert!(transport.replies.is_empty(), "non-map messages are dropped");
        assert!(
            service
                .controller()
                .writes
                .contains(&(DEVF_ERR_STATUS, 0, 0)),
            "the error status was cleared"
        );

        // Without the status bit nothing is written.
        service.controller_mut().error_status = 0;
        let writes = service.controller().writes.len();
        service.dispatch(&mut transport, &stray);
        assert_eq!(service.controller().writes.len(), writes);
    }
}

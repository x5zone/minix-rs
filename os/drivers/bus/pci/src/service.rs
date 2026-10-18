//! Service assembly: the PCI driver's receive-classify-dispatch pump.
//!
//! C correspondence: `main` registers the chardriver table and calls
//! `chardriver_task` (`main.c:721-738`), while the typed bus-query
//! family lands in `pci_other` (`main.c:671-700`). The loop shell and the
//! RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`]; this module owns the
//! one-message-at-a-time policy.
//!
//! Modeled legs: enumeration (`scan`) and the query family — INIT,
//! FIRST_DEV, NEXT_DEV, FIND_DEV, IDS, RESERVE, RESCAN — plus the
//! character open/close defaults. The reply shape matches C: the typed
//! answers travel in the three `m1` integer slots, a found first/next
//! answer reports type one with index plus identifiers, and a miss
//! reports type zero.
//!
//! Unmodeled legs answer honestly instead of inventing values: attribute
//! reads and writes, ACL set/delete, BAR properties and the two name
//! copies reply `ENOSYS`, and the whole `/dev/pci` control surface
//! (BDF configuration, map, unmap, reserve, release) answers `ENOTTY`.
//! C serves those with configuration-space passthrough, grant copies,
//! and kernel privilege calls; those seams are the config-space backend
//! and the privilege transport, not this pump.

use minix_chardriver::protocol::CdevRequest;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{
    Endpoint, Message, CDEV_REPLY_BASE, EBADF, EINVAL, EIO, ENODEV, ENOSYS, ENOTTY, OK,
};

use crate::char_face::{PciFace, QueryReply};
use crate::config::{AccessWidth, ConfigAddress, ConfigSpace};
use crate::protocol::{BusQuery, BUS_REQUEST_BASE};
use crate::scan;

/// The message union payload begins at byte 8 of [`Message`].
const PAYLOAD: usize = 8;

/// The three `m1` integer slots (`m1_i1` through `m1_i3`).
mod m1 {
    use super::PAYLOAD;
    pub const I1: usize = PAYLOAD;
    pub const I2: usize = PAYLOAD + 4;
    pub const I3: usize = PAYLOAD + 8;
}

/// Character open/close payload: the request id leads.
const CDEV_OPENCLOSE_ID: usize = PAYLOAD;
/// Character read/write payload: the request id sits after the flags.
const CDEV_READWRITE_ID: usize = PAYLOAD + 36;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

fn read_i32(msg: &Message, offset: usize) -> i32 {
    let base = ::core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; 4];
    // SAFETY: `Message` is a `#[repr(C)]` plain-old-data union; every
    // offset comes from the pinned slot layout above and stays inside the
    // fixed-size body.
    unsafe {
        ::core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), 4);
    }
    i32::from_le_bytes(bytes)
}

fn write_i32(msg: &mut Message, offset: usize, value: i32) {
    let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: same invariants as [`read_i32`]; writers only use the pinned
    // slot offsets.
    unsafe {
        ::core::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), base.add(offset), 4);
    }
}

/// The PCI service: the query face plus its configuration-space backend.
pub struct PciService<C: ConfigSpace> {
    /// Query face over the enumerated database.
    pub face: PciFace,
    space: C,
}

impl<C: ConfigSpace> PciService<C> {
    /// A service over the given configuration space, with an empty database.
    pub fn new(space: C) -> Self {
        PciService {
            face: PciFace::new(),
            space,
        }
    }

    /// The configuration-space backend (tests and the binary's diagnostics).
    pub fn space(&mut self) -> &mut C {
        &mut self.space
    }

    /// Enumerate the given buses into the database; returns devices added.
    pub fn scan(&mut self, buses: &[u8]) -> usize {
        scan::scan(&mut self.space, &mut self.face.db, buses)
    }

    /// The fresh-start probe: refuse to be born when the configuration
    /// space cannot answer at all.
    ///
    /// C starts unconditionally (its port backend exists); the Rust
    /// service starts only with a backend, which keeps an unwired machine
    /// from claiming an empty bus.
    pub fn probe(&mut self) -> Result<(), i32> {
        match self.space.read(ConfigAddress {
            bus: 0,
            device: 0,
            function: 0,
            offset: scan::PCI_VID,
            width: AccessWidth::Bits16,
        }) {
            Ok(_) => Ok(()),
            Err(_) => Err(ENODEV),
        }
    }

    /// Send one typed bus reply; the three slots carry the payload.
    fn reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        m_type: i32,
        i1: i32,
        i2: i32,
        i3: i32,
    ) {
        let mut message = Message {
            m_type,
            ..Message::default()
        };
        write_i32(&mut message, m1::I1, i1);
        write_i32(&mut message, m1::I2, i2);
        write_i32(&mut message, m1::I3, i3);
        let _ = transport.asynsend(caller, &mut message);
    }

    /// Send one character reply with `{ status, id }`.
    fn char_reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        status: i32,
        id: i32,
    ) {
        let mut message = Message {
            m_type: CDEV_REPLY_BASE,
            ..Message::default()
        };
        write_i32(&mut message, PAYLOAD, status);
        write_i32(&mut message, PAYLOAD + 4, id);
        let _ = transport.asynsend(caller, &mut message);
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) {
            // The PCI driver registers neither an interrupt nor a clock
            // hook; notifications are ignored (`pci_other`'s notify path).
            return;
        }
        let caller = msg.m_source;
        if let Some(request) = CdevRequest::decode(msg.m_type) {
            self.handle_char(transport, request, msg);
            return;
        }
        let Some(query) = BusQuery::decode(msg.m_type) else {
            // C's default arm logs and drops unknown message types.
            return;
        };
        self.handle_query(transport, query, caller, msg);
    }

    /// The character defaults: open and close succeed; the control surface
    /// is a registered seam and answers "inappropriate control".
    fn handle_char<T: DriverTransport>(&mut self, transport: &mut T, request: CdevRequest, msg: &Message) {
        let caller = msg.m_source;
        match request {
            CdevRequest::Open | CdevRequest::Close => {
                let id = read_i32(msg, CDEV_OPENCLOSE_ID);
                self.char_reply(transport, caller, OK, id);
            }
            CdevRequest::Read | CdevRequest::Write => {
                let id = read_i32(msg, CDEV_READWRITE_ID);
                self.char_reply(transport, caller, EIO, id);
            }
            CdevRequest::Ioctl => {
                let id = read_i32(msg, CDEV_READWRITE_ID);
                self.char_reply(transport, caller, ENOTTY, id);
            }
            CdevRequest::Select => self.char_reply(transport, caller, EBADF, 0),
            // `chardriver_task`'s default for a missing cancel hook.
            CdevRequest::Cancel => {}
        }
    }

    /// Run one typed bus query.
    fn handle_query<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        query: BusQuery,
        caller: Endpoint,
        msg: &Message,
    ) {
        let echo1 = read_i32(msg, m1::I1);
        let echo2 = read_i32(msg, m1::I2);
        let echo3 = read_i32(msg, m1::I3);
        match query {
            BusQuery::Init => self.reply(transport, caller, OK, echo1, echo2, echo3),
            BusQuery::FirstDevice => {
                match self.face.first_dev(caller.0 as i64) {
                    QueryReply::Index(Some(index)) => self.reply_found(transport, caller, index),
                    QueryReply::Index(None) => {
                        self.reply(transport, caller, 0, echo1, echo2, echo3)
                    }
                    QueryReply::Status(status) => {
                        self.reply(transport, caller, status, echo1, echo2, echo3)
                    }
                }
            }
            BusQuery::NextDevice => {
                let previous = echo1.max(0) as usize;
                match self.face.next_dev(caller.0 as i64, previous) {
                    QueryReply::Index(Some(index)) => self.reply_found(transport, caller, index),
                    QueryReply::Index(None) => {
                        self.reply(transport, caller, 0, echo1, echo2, echo3)
                    }
                    QueryReply::Status(status) => {
                        self.reply(transport, caller, status, echo1, echo2, echo3)
                    }
                }
            }
            BusQuery::FindDevice => {
                let bus = echo1 as u8;
                let device = echo2 as u8;
                let function = echo3 as u8;
                match self.face.find_dev(caller.0 as i64, bus, device, function) {
                    QueryReply::Index(Some(index)) => {
                        self.reply(transport, caller, 1, index as i32, echo2, echo3)
                    }
                    QueryReply::Index(None) => {
                        self.reply(transport, caller, 0, echo1, echo2, echo3)
                    }
                    QueryReply::Status(status) => {
                        self.reply(transport, caller, status, echo1, echo2, echo3)
                    }
                }
            }
            BusQuery::Identifiers => {
                let index = echo1.max(0) as usize;
                match self.face.db.get(index) {
                    Some(device) => self.reply(
                        transport,
                        caller,
                        OK,
                        device.vendor as i32,
                        device.device_id as i32,
                        echo3,
                    ),
                    None => self.reply(transport, caller, EINVAL, 0, 0, echo3),
                }
            }
            BusQuery::Reserve => {
                let index = echo1.max(0) as usize;
                let status = match self.face.reserve(caller.0 as i64, index) {
                    QueryReply::Status(code) => code,
                    QueryReply::Index(_) => EINVAL,
                };
                self.reply(transport, caller, status, echo1, echo2, echo3);
            }
            BusQuery::Rescan => {
                let bus = echo1.max(0) as u8;
                self.scan(&[bus]);
                self.reply(transport, caller, OK, echo1, echo2, echo3);
            }
            // Attributes, names, ACL management and BAR properties wait
            // on the configuration-space passthrough and privilege
            // transport seams; answering ENOSYS keeps a caller from
            // reading a fabricated value.
            BusQuery::ReadAttribute8
            | BusQuery::ReadAttribute16
            | BusQuery::ReadAttribute32
            | BusQuery::WriteAttribute8
            | BusQuery::WriteAttribute16
            | BusQuery::WriteAttribute32
            | BusQuery::DeviceName
            | BusQuery::SlotName
            | BusQuery::SetAccess
            | BusQuery::DeleteAccess
            | BusQuery::GetBar => self.reply(transport, caller, ENOSYS, echo1, echo2, echo3),
        }
    }

    /// Answer a found first/next device: type one, index, vendor, device.
    fn reply_found<T: DriverTransport>(&mut self, transport: &mut T, caller: Endpoint, index: usize) {
        match self.face.db.get(index) {
            Some(device) => self.reply(
                transport,
                caller,
                1,
                index as i32,
                device.vendor as i32,
                device.device_id as i32,
            ),
            None => self.reply(transport, caller, 0, 0, 0, 0),
        }
    }
}

// The typed request base is the C `BUSC_RQ_BASE`; keep the link visible in
// this module so a renumbering of the protocol fails to compile here.
const _: () = assert!(BUS_REQUEST_BASE == 0x300);

impl<C: ConfigSpace, T: DriverTransport> DriverHandler<T> for PciService<C> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    use crate::config::MemConfigSpace;
    use crate::scan::{PCI_DID, PCI_VID};

    /// Scripted transport recording every reply.
    struct Scripted {
        sent: Vec<(Endpoint, Message)>,
    }

    impl Scripted {
        fn new() -> Self {
            Scripted { sent: Vec::new() }
        }
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sent.push((dst, *msg));
            Ok(())
        }
        fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sent.push((dst, *msg));
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sent.push((dst, *msg));
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

    fn write16(space: &mut MemConfigSpace, bus: u8, device: u8, function: u8, offset: u8, value: u16) {
        space
            .write(
                ConfigAddress {
                    bus,
                    device,
                    function,
                    offset,
                    width: AccessWidth::Bits16,
                },
                value as u32,
            )
            .unwrap();
    }

    fn present(space: &mut MemConfigSpace, device: u8, vendor: u16, device_id: u16) {
        write16(space, 0, device, 0, PCI_VID, vendor);
        write16(space, 0, device, 0, PCI_DID, device_id);
    }

    fn service_with_two_devices() -> PciService<MemConfigSpace> {
        let mut space = MemConfigSpace::new();
        present(&mut space, 0, 0x8086, 0x100E);
        present(&mut space, 1, 0x10EC, 0x8139);
        let mut service = PciService::new(space);
        assert_eq!(service.scan(&[0]), 2);
        service
    }

    fn query(kind: BusQuery, caller: Endpoint, i1: i32, i2: i32, i3: i32) -> Message {
        let mut msg = Message {
            m_type: kind.message_type(),
            m_source: caller,
            ..Message::default()
        };
        write_i32(&mut msg, m1::I1, i1);
        write_i32(&mut msg, m1::I2, i2);
        write_i32(&mut msg, m1::I3, i3);
        msg
    }

    fn last(transport: &Scripted) -> &Message {
        &transport.sent.last().expect("a reply").1
    }

    #[test]
    fn test_probe_needs_a_configuration_space() {
        let mut absent = PciService::new(crate::config::NullConfigSpace);
        assert_eq!(absent.probe(), Err(ENODEV));
        let mut present = PciService::new(MemConfigSpace::new());
        assert_eq!(present.probe(), Ok(()));
    }

    #[test]
    fn test_first_and_next_report_index_and_identifiers() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();

        service.dispatch(&mut transport, &query(BusQuery::FirstDevice, Endpoint(5), 0, 0, 0));
        assert_eq!(last(&transport).m_type, 1);
        assert_eq!(read_i32(last(&transport), m1::I1), 0);
        assert_eq!(read_i32(last(&transport), m1::I2), 0x8086);
        assert_eq!(read_i32(last(&transport), m1::I3), 0x100E);

        service.dispatch(&mut transport, &query(BusQuery::NextDevice, Endpoint(5), 0, 0, 0));
        assert_eq!(last(&transport).m_type, 1);
        assert_eq!(read_i32(last(&transport), m1::I1), 1);
        assert_eq!(read_i32(last(&transport), m1::I2), 0x10EC);

        service.dispatch(&mut transport, &query(BusQuery::NextDevice, Endpoint(5), 1, 0, 0));
        assert_eq!(last(&transport).m_type, 0, "end of the visible list");
    }

    #[test]
    fn test_find_device_reports_index_or_miss() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &query(BusQuery::FindDevice, Endpoint(5), 0, 1, 0));
        assert_eq!(last(&transport).m_type, 1);
        assert_eq!(read_i32(last(&transport), m1::I1), 1);

        service.dispatch(&mut transport, &query(BusQuery::FindDevice, Endpoint(5), 0, 7, 0));
        assert_eq!(last(&transport).m_type, 0);
    }

    #[test]
    fn test_identifiers_reply_uses_the_database() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &query(BusQuery::Identifiers, Endpoint(5), 1, 0, 0));
        assert_eq!(last(&transport).m_type, OK);
        assert_eq!(read_i32(last(&transport), m1::I1), 0x10EC);
        assert_eq!(read_i32(last(&transport), m1::I2), 0x8139);

        service.dispatch(&mut transport, &query(BusQuery::Identifiers, Endpoint(5), 9, 0, 0));
        assert_eq!(last(&transport).m_type, EINVAL);
    }

    #[test]
    fn test_reserve_is_per_caller_and_reports_occupancy() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &query(BusQuery::Reserve, Endpoint(5), 0, 0, 0));
        assert_eq!(last(&transport).m_type, OK);

        service.dispatch(&mut transport, &query(BusQuery::Reserve, Endpoint(6), 0, 0, 0));
        assert_eq!(last(&transport).m_type, minix_types::EBUSY);

        service.dispatch(&mut transport, &query(BusQuery::Reserve, Endpoint(5), 9, 0, 0));
        assert_eq!(last(&transport).m_type, EINVAL);
    }

    #[test]
    fn test_rescan_adds_devices_found_later() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();
        present(service.space(), 2, 0x1AF4, 0x1000);
        service.dispatch(&mut transport, &query(BusQuery::Rescan, Endpoint(5), 0, 0, 0));
        assert_eq!(last(&transport).m_type, OK);
        service.dispatch(&mut transport, &query(BusQuery::Identifiers, Endpoint(5), 2, 0, 0));
        assert_eq!(read_i32(last(&transport), m1::I1), 0x1AF4);
    }

    #[test]
    fn test_init_replies_ok() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &query(BusQuery::Init, Endpoint(5), 0, 0, 0));
        assert_eq!(last(&transport).m_type, OK);
    }

    #[test]
    fn test_unmodeled_queries_answer_enosys() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();
        for kind in [
            BusQuery::ReadAttribute8,
            BusQuery::WriteAttribute32,
            BusQuery::SetAccess,
            BusQuery::GetBar,
        ] {
            service.dispatch(&mut transport, &query(kind, Endpoint(5), 0, 0, 0));
            assert_eq!(last(&transport).m_type, ENOSYS, "{:?}", kind);
        }
    }

    #[test]
    fn test_character_defaults_and_control_seam() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();
        for kind in [CdevRequest::Open, CdevRequest::Close] {
            let msg = Message {
                m_type: kind.message_type(),
                m_source: Endpoint(5),
                ..Message::default()
            };
            service.dispatch(&mut transport, &msg);
            assert_eq!(read_i32(last(&transport), PAYLOAD), OK);
        }
        for (kind, expected) in [
            (CdevRequest::Ioctl, ENOTTY),
            (CdevRequest::Read, EIO),
            (CdevRequest::Write, EIO),
            (CdevRequest::Select, EBADF),
        ] {
            let msg = Message {
                m_type: kind.message_type(),
                m_source: Endpoint(5),
                ..Message::default()
            };
            service.dispatch(&mut transport, &msg);
            assert_eq!(read_i32(last(&transport), PAYLOAD), expected);
        }
    }

    #[test]
    fn test_notifications_and_unknown_types_stay_silent() {
        let mut service = service_with_two_devices();
        let mut transport = Scripted::new();
        let notify = Message {
            m_type: 0x1000,
            m_source: Endpoint::HARDWARE,
            ..Message::default()
        };
        service.dispatch(&mut transport, &notify);
        let unknown = Message {
            m_type: 0x2222,
            m_source: Endpoint(5),
            ..Message::default()
        };
        service.dispatch(&mut transport, &unknown);
        assert!(transport.sent.is_empty());
    }
}

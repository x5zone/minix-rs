//! Service assembly: the printer's receive-classify-dispatch pump.
//!
//! C correspondence: `printer_tab` (`printer.c:99-103`: open, write,
//! cancel, and interrupt hooks) served by `chardriver_task`
//! (`printer.c:109-116`). The loop shell and the RS birth handshake live
//! in [`minix_driver_rt::runtime::DriverRuntime`]; this module owns the
//! one-message-at-a-time policy and the byte pump between the caller's
//! grant and the printer interface.
//!
//! The driver has no read and no ioctl hook in C: `chardriver_task`'s
//! default arms answer a read through the missing hook (the C table
//! leaves `cdr_read` null) and answer ioctl with `ENOTTY`, select with
//! `EBADF`, cancel without a hook with `EDONTREPLY`. The pump keeps those
//! defaults, with the read case answered as an input-output error rather
//! than dereferencing a null hook.
//!
//! C stores ordinary completion as `OK`, a copy failure as `EFAULT`, and
//! a hardware error as the raw status byte in one `int`, then bit-tests
//! that int as a status byte in `output_done` (`printer.c:208-241`). The
//! Rust pump keeps the three cases apart in [`Completion`]; the only
//! external difference is the copy-failure leg, which C would classify by
//! accident of EFAULT's low bits. A copy failure answers `EIO` here.

use minix_chardriver::protocol::CdevRequest;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{
    Endpoint, Message, CDEV_NONBLOCK, CDEV_REPLY_BASE, EAGAIN, EBADF, EINVAL, EINTR, EIO, ENOTTY,
    OK,
};

use crate::hardware::PrinterHardware;
use crate::status::{
    read_status, PrinterState, MAX_ONLINE_RETRIES, STATUS_BUSY, STATUS_MASK, STATUS_NORMAL,
    STATUS_ONLINE,
};

/// The message union payload begins at byte 8 of [`Message`].
const PAYLOAD: usize = 8;

/// Bytes fetched from the caller per chunk (`obuf[128]`,
/// `printer.c:64-65`).
const PRINT_BUFFER: usize = 128;

/// Delay between online polls (`micro_delay(500000)`,
/// `printer.c:200-201`).
const ONLINE_POLL_DELAY_MICROS: u32 = 500_000;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// Character-request wire offsets, keyed per payload struct.
mod cwire {
    use super::PAYLOAD;
    pub mod openclose {
        use super::PAYLOAD;
        pub const ID: usize = PAYLOAD;
    }
    pub mod readwrite {
        use super::PAYLOAD;
        pub const GRANT: usize = PAYLOAD + 8;
        pub const COUNT: usize = PAYLOAD + 16;
        pub const FLAGS: usize = PAYLOAD + 32;
        pub const ID: usize = PAYLOAD + 36;
    }
    pub mod cancel {
        use super::PAYLOAD;
        pub const ID: usize = PAYLOAD;
    }
}

fn read_bytes<const N: usize>(msg: &Message, offset: usize) -> [u8; N] {
    let base = ::core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; N];
    // SAFETY: `Message` is a `#[repr(C)]` plain-old-data union; every
    // offset comes from the pinned wire layout above and stays inside the
    // fixed-size body.
    unsafe {
        ::core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), N);
    }
    bytes
}

fn read_u32(msg: &Message, offset: usize) -> u32 {
    u32::from_le_bytes(read_bytes(msg, offset))
}

fn read_i32(msg: &Message, offset: usize) -> i32 {
    i32::from_le_bytes(read_bytes(msg, offset))
}

fn read_u64(msg: &Message, offset: usize) -> u64 {
    u64::from_le_bytes(read_bytes(msg, offset))
}

/// Fills a reply message's `{ status; id; }` payload at the union start.
fn write_reply(msg: &mut Message, status: i32, id: i32) {
    let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: the reply payload is two 4-byte words at the union start,
    // inside the message's fixed-size body.
    unsafe {
        ::core::ptr::copy_nonoverlapping(status.to_le_bytes().as_ptr(), base.add(PAYLOAD), 4);
        ::core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(PAYLOAD + 4), 4);
    }
}

/// What ended the current output chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Completion {
    /// The whole request printed (`done_status = OK`).
    Ok,
    /// The caller's grant could not be read (`done_status = EFAULT`).
    CopyFault,
    /// The interface reported this raw status byte.
    Status(u8),
}

/// The printer service: the hardware seam plus the in-flight write.
pub struct PrinterService<H: PrinterHardware> {
    hw: H,
    /// Whether the first open already initialized the interface
    /// (`printer.c:309-321`).
    initialized: bool,
    /// Whether a write request is in flight (`writing`).
    writing: bool,
    caller: Endpoint,
    write_id: u32,
    grant: i32,
    /// Bytes of the caller's buffer already handed to the printer
    /// (`user_vir_d`).
    offset: u64,
    orig_count: usize,
    /// Bytes of the caller's buffer left (`user_left`).
    user_left: usize,
    /// Current chunk fetched from the caller (`obuf`).
    chunk: [u8; PRINT_BUFFER],
    /// Next byte of the chunk (`optr`).
    out_at: usize,
    /// Live bytes in the chunk (`oleft`).
    in_chunk: usize,
}

impl<H: PrinterHardware> PrinterService<H> {
    /// A service over the given hardware seam.
    pub fn new(hw: H) -> Self {
        PrinterService {
            hw,
            initialized: false,
            writing: false,
            caller: Endpoint(0),
            write_id: 0,
            grant: 0,
            offset: 0,
            orig_count: 0,
            user_left: 0,
            chunk: [0; PRINT_BUFFER],
            out_at: 0,
            in_chunk: 0,
        }
    }

    /// The C fresh-start probe: refuse to be born without a printer
    /// (`printer.c:145-150`).
    pub fn probe(&mut self) -> Result<(), i32> {
        if self.hw.probe() { Ok(()) } else { Err(minix_types::ENODEV) }
    }

    /// The hardware seam (tests and the binary's diagnostics).
    pub fn hardware(&self) -> &H {
        &self.hw
    }

    /// The hardware seam, mutably (tests that script port traffic).
    pub fn hardware_mut(&mut self) -> &mut H {
        &mut self.hw
    }

    /// Send one `{ status, id }` character reply.
    fn reply<T: DriverTransport>(&mut self, transport: &mut T, caller: Endpoint, status: i32, id: u32) {
        let mut message = Message {
            m_type: CDEV_REPLY_BASE,
            ..Message::default()
        };
        write_reply(&mut message, status, id as i32);
        let _ = transport.asynsend(caller, &mut message);
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) {
            if msg.m_source == Endpoint::HARDWARE {
                self.handle_interrupt(transport);
            }
            return;
        }
        let caller = msg.m_source;
        let Some(request) = CdevRequest::decode(msg.m_type) else {
            // `chardriver_task`'s default arm reports invalid argument.
            self.reply(transport, caller, EINVAL, 0);
            return;
        };
        match request {
            CdevRequest::Open => self.handle_open(transport, msg),
            CdevRequest::Close => {
                let id = read_u32(msg, cwire::openclose::ID);
                self.reply(transport, caller, OK, id);
            }
            // No read hook exists in C; answering an input-output error is
            // the honest reading of the missing default.
            CdevRequest::Read => {
                let id = read_u32(msg, cwire::readwrite::ID);
                self.reply(transport, caller, EIO, id);
            }
            CdevRequest::Write => self.handle_write(transport, msg),
            CdevRequest::Ioctl => {
                let id = read_u32(msg, cwire::readwrite::ID);
                self.reply(transport, caller, ENOTTY, id);
            }
            CdevRequest::Cancel => self.handle_cancel(transport, msg),
            CdevRequest::Select => {
                let id = 0;
                self.reply(transport, caller, EBADF, id);
            }
        }
    }

    /// Initialize the interface on the first open (`printer_open`).
    fn handle_open<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let id = read_u32(msg, cwire::openclose::ID);
        if !self.initialized {
            if self.hw.initialize().is_err() {
                self.reply(transport, msg.m_source, EIO, id);
                return;
            }
            self.initialized = true;
        }
        self.reply(transport, msg.m_source, OK, id);
    }

    /// Start one print request (`printer_write`, `printer.c:161-206`).
    fn handle_write<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let caller = msg.m_source;
        let id = read_u32(msg, cwire::readwrite::ID);
        if self.writing {
            self.reply(transport, caller, EIO, id);
            return;
        }
        let size = read_u64(msg, cwire::readwrite::COUNT) as usize;
        let flags = read_i32(msg, cwire::readwrite::FLAGS);
        if size == 0 {
            self.reply(transport, caller, EINVAL, id);
            return;
        }
        if flags & CDEV_NONBLOCK != 0 {
            self.reply(transport, caller, EAGAIN, id);
            return;
        }

        self.writing = true;
        self.caller = caller;
        self.write_id = id;
        self.grant = read_u32(msg, cwire::readwrite::GRANT) as i32;
        self.offset = 0;
        self.orig_count = size;
        self.user_left = size;
        self.out_at = 0;
        self.in_chunk = 0;

        // Wait until the printer is online before sending anything, so an
        // offline printer is reported once instead of after a byte.
        let mut last_status = 0u8;
        for attempt in 0..MAX_ONLINE_RETRIES {
            match self.hw.status() {
                Ok(status) => {
                    last_status = status;
                    if status & STATUS_ONLINE != 0 {
                        self.run_output(transport);
                        return;
                    }
                }
                Err(_) => panic!("printer: status read failed"),
            }
            let _ = attempt;
            self.hw.delay_micros(ONLINE_POLL_DELAY_MICROS);
        }
        self.finish(transport, Completion::Status(last_status));
    }

    /// Fetch the next chunk of the caller's buffer (`prepare_output`).
    fn prepare_output<T: DriverTransport>(&mut self, transport: &mut T) -> bool {
        let chunk = self.user_left.min(PRINT_BUFFER);
        match transport.copy_from_grant(self.caller, self.grant, self.offset, &mut self.chunk[..chunk])
        {
            Ok(()) => {
                self.in_chunk = chunk;
                self.out_at = 0;
                true
            }
            Err(_) => false,
        }
    }

    /// Push chunks and bytes while the interface accepts them
    /// (`printer_intr`, `printer.c:345-424`).
    fn run_output<T: DriverTransport>(&mut self, transport: &mut T) {
        loop {
            if self.out_at == self.in_chunk {
                if self.user_left == 0 {
                    self.finish(transport, Completion::Ok);
                    return;
                }
                if !self.prepare_output(transport) {
                    self.finish(transport, Completion::CopyFault);
                    return;
                }
            }
            while self.out_at < self.in_chunk {
                let status = match self.hw.status() {
                    Ok(status) => status,
                    Err(_) => panic!("printer: status read failed"),
                };
                if status & STATUS_MASK == STATUS_BUSY {
                    // Still printing the previous byte; the interrupt
                    // notification resumes this request.
                    return;
                }
                if status & STATUS_MASK == STATUS_NORMAL {
                    let byte = self.chunk[self.out_at];
                    if self.hw.output(byte).is_err() {
                        panic!("printer: output failed");
                    }
                    self.out_at += 1;
                    self.offset += 1;
                    self.user_left -= 1;
                } else {
                    self.finish(transport, Completion::Status(status));
                    return;
                }
            }
        }
    }

    /// Complete the request (`output_done`, `printer.c:208-241`).
    fn finish<T: DriverTransport>(&mut self, transport: &mut T, completion: Completion) {
        if !self.writing {
            return;
        }
        let status = match completion {
            Completion::Ok => self.orig_count as i32,
            Completion::CopyFault => EIO,
            Completion::Status(raw) => match read_status(raw) {
                PrinterState::NoPaper => {
                    // Some characters were printed: report how many so the
                    // caller can resume (`printer.c:236-238`).
                    if self.user_left < self.orig_count {
                        (self.orig_count - self.user_left) as i32
                    } else {
                        EAGAIN
                    }
                }
                _ => EIO,
            },
        };
        let caller = self.caller;
        let id = self.write_id;
        self.writing = false;
        self.in_chunk = 0;
        self.out_at = 0;
        self.user_left = 0;
        self.reply(transport, caller, status, id);
    }

    /// Hardware interrupt: resume a parked request, or drop the select
    /// line when nothing is pending (`printer_intr`'s empty path).
    fn handle_interrupt<T: DriverTransport>(&mut self, transport: &mut T) {
        if !self.writing {
            let _ = self.hw.deselect();
            return;
        }
        self.run_output(transport);
    }

    /// Cancel a print request (`printer_cancel`, `printer.c:243-262`).
    fn handle_cancel<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let id = read_u32(msg, cwire::cancel::ID);
        let caller = msg.m_source;
        if self.writing && self.caller == caller && self.write_id == id {
            self.writing = false;
            self.in_chunk = 0;
            self.out_at = 0;
            self.user_left = 0;
            self.reply(transport, caller, EINTR, id);
        }
        // A cancel for someone else's request is left unanswered
        // (`EDONTREPLY`); VFS owns the caller's fate.
    }
}

impl<H: PrinterHardware, T: DriverTransport> DriverHandler<T> for PrinterService<H> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    /// Scripted hardware: records every port action, serves a status
    /// script and then a healthy default.
    struct TestPrinter {
        present: bool,
        init_count: u32,
        status_script: Vec<u8>,
        status_default: u8,
        written: Vec<u8>,
        output_fails: bool,
        delays: Vec<u32>,
        deselected: u32,
    }

    impl TestPrinter {
        fn new() -> Self {
            TestPrinter {
                present: true,
                init_count: 0,
                status_script: Vec::new(),
                status_default: STATUS_NORMAL,
                written: Vec::new(),
                output_fails: false,
                delays: Vec::new(),
                deselected: 0,
            }
        }
    }

    impl PrinterHardware for TestPrinter {
        fn probe(&mut self) -> bool {
            self.present
        }
        fn initialize(&mut self) -> Result<(), i32> {
            self.init_count += 1;
            Ok(())
        }
        fn status(&mut self) -> Result<u8, i32> {
            if self.status_script.is_empty() {
                Ok(self.status_default)
            } else {
                Ok(self.status_script.remove(0))
            }
        }
        fn output(&mut self, byte: u8) -> Result<(), i32> {
            if self.output_fails {
                return Err(-EIO);
            }
            self.written.push(byte);
            Ok(())
        }
        fn deselect(&mut self) -> Result<(), i32> {
            self.deselected += 1;
            Ok(())
        }
        fn delay_micros(&mut self, micros: u32) {
            self.delays.push(micros);
        }
    }

    /// Scripted transport recording replies and grant copies.
    struct Scripted {
        sent: Vec<(Endpoint, Message)>,
        copies: Vec<(Endpoint, i32, u64, usize)>,
        copy_fails: bool,
    }

    impl Scripted {
        fn new() -> Self {
            Scripted {
                sent: Vec::new(),
                copies: Vec::new(),
                copy_fails: false,
            }
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
            granter: Endpoint,
            grant: i32,
            offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            self.copies.push((granter, grant, offset, buf.len()));
            if self.copy_fails {
                return Err(-minix_types::EFAULT);
            }
            for (index, byte) in buf.iter_mut().enumerate() {
                *byte = 0x40u8.wrapping_add(index as u8);
            }
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

    fn request(kind: CdevRequest, caller: Endpoint) -> Message {
        Message {
            m_type: kind.message_type(),
            m_source: caller,
            ..Message::default()
        }
    }

    fn open_message(caller: Endpoint, id: u32) -> Message {
        let mut msg = request(CdevRequest::Open, caller);
        write_u32_at(&mut msg, cwire::openclose::ID, id);
        msg
    }

    fn write_message(caller: Endpoint, id: u32, count: u64, grant: i32, flags: i32) -> Message {
        let mut msg = request(CdevRequest::Write, caller);
        write_u32_at(&mut msg, cwire::readwrite::ID, id);
        write_u64_at(&mut msg, cwire::readwrite::COUNT, count);
        write_u32_at(&mut msg, cwire::readwrite::GRANT, grant as u32);
        write_i32_at(&mut msg, cwire::readwrite::FLAGS, flags);
        msg
    }

    fn cancel_message(caller: Endpoint, id: u32) -> Message {
        let mut msg = request(CdevRequest::Cancel, caller);
        write_u32_at(&mut msg, cwire::cancel::ID, id);
        msg
    }

    fn interrupt_message() -> Message {
        Message {
            m_type: 0x1000,
            m_source: Endpoint::HARDWARE,
            ..Message::default()
        }
    }

    fn write_u32_at(msg: &mut Message, offset: usize, value: u32) {
        let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
        // SAFETY: test-only writer into the fixed-size message body.
        unsafe {
            ::core::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), base.add(offset), 4);
        }
    }

    fn write_i32_at(msg: &mut Message, offset: usize, value: i32) {
        write_u32_at(msg, offset, value as u32);
    }

    fn write_u64_at(msg: &mut Message, offset: usize, value: u64) {
        let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
        // SAFETY: test-only writer into the fixed-size message body.
        unsafe {
            ::core::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), base.add(offset), 8);
        }
    }

    fn last(transport: &Scripted) -> &Message {
        &transport.sent.last().expect("a reply").1
    }

    fn last_status(transport: &Scripted) -> i32 {
        read_i32(last(transport), PAYLOAD)
    }

    fn last_id(transport: &Scripted) -> i32 {
        read_i32(last(transport), PAYLOAD + 4)
    }

    #[test]
    fn test_probe_reports_absence_honestly() {
        let mut absent = PrinterService::new(crate::hardware::NullPrinter);
        assert!(absent.probe().is_err());
        assert!(PrinterService::new(TestPrinter::new()).probe().is_ok());
    }

    #[test]
    fn test_open_initializes_once() {
        let mut service = PrinterService::new(TestPrinter::new());
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &open_message(Endpoint(5), 1));
        service.dispatch(&mut transport, &open_message(Endpoint(5), 2));
        assert_eq!(service.hardware().init_count, 1);
        assert_eq!(transport.sent.len(), 2);
        assert_eq!(last_status(&transport), OK);
        assert_eq!(last_id(&transport), 2);
    }

    #[test]
    fn test_write_waits_for_online_then_prints_the_chunk() {
        let mut card = TestPrinter::new();
        // Two offline polls, then online; healthy statuses beyond.
        card.status_script = vec![0x80, 0x80, STATUS_NORMAL];
        let mut service = PrinterService::new(card);
        let mut transport = Scripted::new();
        let msg = write_message(Endpoint(5), 7, 4, 9, 0);
        service.dispatch(&mut transport, &msg);

        assert_eq!(service.hardware().delays.len(), 2);
        assert_eq!(service.hardware().delays[0], ONLINE_POLL_DELAY_MICROS);
        assert_eq!(service.hardware().written, vec![0x40, 0x41, 0x42, 0x43]);
        assert_eq!(last_status(&transport), 4);
        assert_eq!(last_id(&transport), 7);
        assert_eq!(transport.copies[0], (Endpoint(5), 9, 0, 4));
    }

    #[test]
    fn test_paper_out_mid_way_reports_the_partial_count() {
        let mut card = TestPrinter::new();
        // Online poll, two healthy bytes, then online-but-no-paper.
        card.status_script = vec![STATUS_NORMAL, STATUS_NORMAL, STATUS_NORMAL, 0xB0];
        let mut service = PrinterService::new(card);
        let mut transport = Scripted::new();
        let msg = write_message(Endpoint(5), 8, 6, 1, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(service.hardware().written.len(), 2);
        assert_eq!(last_status(&transport), 2);
    }

    #[test]
    fn test_offline_mid_way_is_an_input_output_error() {
        let mut card = TestPrinter::new();
        card.status_script = vec![STATUS_NORMAL, STATUS_NORMAL, 0x80];
        let mut service = PrinterService::new(card);
        let mut transport = Scripted::new();
        let msg = write_message(Endpoint(5), 9, 6, 1, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(last_status(&transport), EIO);
    }

    #[test]
    fn test_busy_status_parks_and_the_interrupt_finishes() {
        let mut card = TestPrinter::new();
        // Online poll, then busy on the first byte.
        card.status_script = vec![STATUS_NORMAL, STATUS_BUSY];
        let mut service = PrinterService::new(card);
        let mut transport = Scripted::new();
        let msg = write_message(Endpoint(5), 10, 3, 1, 0);
        service.dispatch(&mut transport, &msg);
        assert!(transport.sent.is_empty(), "busy keeps the write parked");

        // The interrupt resumes with the healthy default.
        service.dispatch(&mut transport, &interrupt_message());
        assert_eq!(transport.sent.len(), 1);
        assert_eq!(last_status(&transport), 3);
        assert_eq!(service.hardware().written.len(), 3);
    }

    #[test]
    fn test_write_refusals_follow_the_c_order() {
        let mut service = PrinterService::new(TestPrinter::new());
        let mut transport = Scripted::new();

        service.dispatch(&mut transport, &write_message(Endpoint(5), 1, 0, 1, 0));
        assert_eq!(last_status(&transport), EINVAL);

        service.dispatch(
            &mut transport,
            &write_message(Endpoint(5), 2, 4, 1, CDEV_NONBLOCK),
        );
        assert_eq!(last_status(&transport), EAGAIN);

        // Park a real write first (busy), then a second one is refused.
        service.hardware_mut().status_script = vec![STATUS_NORMAL, STATUS_BUSY];
        service.dispatch(&mut transport, &write_message(Endpoint(5), 3, 4, 1, 0));
        service.dispatch(&mut transport, &write_message(Endpoint(6), 4, 4, 1, 0));
        assert_eq!(last_status(&transport), EIO);
    }

    #[test]
    fn test_cancel_from_the_owner_answers_eintr_and_others_do_not() {
        let mut card = TestPrinter::new();
        card.status_script = vec![STATUS_NORMAL, STATUS_BUSY];
        let mut service = PrinterService::new(card);
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &write_message(Endpoint(5), 11, 4, 1, 0));
        assert!(transport.sent.is_empty());

        // A stranger's cancel is left unanswered.
        service.dispatch(&mut transport, &cancel_message(Endpoint(6), 11));
        assert!(transport.sent.is_empty());

        service.dispatch(&mut transport, &cancel_message(Endpoint(5), 11));
        assert_eq!(transport.sent.len(), 1);
        assert_eq!(last_status(&transport), EINTR);
        assert_eq!(last_id(&transport), 11);
    }

    #[test]
    fn test_copy_fault_answers_io_error() {
        let mut service = PrinterService::new(TestPrinter::new());
        let mut transport = Scripted::new();
        transport.copy_fails = true;
        let msg = write_message(Endpoint(5), 12, 4, 1, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(last_status(&transport), EIO);
        assert!(service.hardware().written.is_empty());
    }

    #[test]
    fn test_missing_hooks_answer_like_the_c_defaults() {
        let mut service = PrinterService::new(TestPrinter::new());
        let mut transport = Scripted::new();
        for kind in [CdevRequest::Read, CdevRequest::Ioctl, CdevRequest::Select] {
            service.dispatch(&mut transport, &request(kind, Endpoint(5)));
        }
        assert_eq!(read_i32(&transport.sent[0].1, PAYLOAD), EIO);
        assert_eq!(read_i32(&transport.sent[1].1, PAYLOAD), ENOTTY);
        assert_eq!(read_i32(&transport.sent[2].1, PAYLOAD), EBADF);

        service.dispatch(&mut transport, &request(CdevRequest::Close, Endpoint(5)));
        assert_eq!(last_status(&transport), OK);
    }
}

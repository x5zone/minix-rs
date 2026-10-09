//! Real [`IoBus`] driving the controller through the kernel `SYS_DEVIO` relay.
//!
//! Bridges the pure sequencing layer ([`crate::pio`]) to hardware: each port
//! access becomes a `minix_sys::syscall::sys_in*`/`sys_out*` call. Minix drivers
//! never hold IOPL — the kernel performs the `in`/`out` on their behalf
//! (see the `sys_inb` doc in `minix-sys/src/syscall.rs`; C `do_devio.c`).
//!
//! C uses `sys_insw`/`sys_outsw` (16-bit string ops, one syscall per sector)
//! for the data register (at_wini.c:1156-1178). The word-at-a-time `sys_inw`/
//! `sys_outw` loop here is functionally equivalent for the ATA data register
//! (each access reads/writes one 16-bit word from the same port); the batched
//! string syscall is a later throughput optimization, not a correctness
//! requirement.

use crate::pio::IoBus;
use minix_sys::syscall::{KernelCallTransport, sys_inb, sys_inw, sys_outb, sys_outw};

/// A [`IoBus`] backed by `SYS_DEVIO` port I/O over the given kernel transport.
///
/// The real service binary instantiates this with the direct-trap transport
/// (`minix_sys::syscall::DirectKernelCallTransport`); tests inject a canned one.
pub struct DevioIoBus<T: KernelCallTransport> {
    transport: T,
}

impl<T: KernelCallTransport> DevioIoBus<T> {
    /// Wrap a kernel-call transport as a port bus.
    pub const fn new(transport: T) -> Self {
        DevioIoBus { transport }
    }
}

impl<T: KernelCallTransport> IoBus for DevioIoBus<T> {
    fn inb(&mut self, port: u16) -> u8 {
        // A failed port read means the kernel relay is unreachable; C panics
        // in the same spot (`panic("... sys_inb failed")`, at_wini.c:973) rather
        // than act on a stale byte. Fail-stop.
        sys_inb(&self.transport, port)
            .unwrap_or_else(|e| panic!("at_wini: sys_inb({port:#06x}) failed: {e}"))
    }

    fn outb(&mut self, port: u16, value: u8) {
        sys_outb(&self.transport, port, value)
            .unwrap_or_else(|e| panic!("at_wini: sys_outb({port:#06x}) failed: {e}"));
    }

    fn in_words(&mut self, port: u16, dst: &mut [u16]) {
        for slot in dst.iter_mut() {
            *slot = sys_inw(&self.transport, port)
                .unwrap_or_else(|e| panic!("at_wini: sys_inw({port:#06x}) failed: {e}"));
        }
    }

    fn out_words(&mut self, port: u16, src: &[u16]) {
        for &word in src {
            sys_outw(&self.transport, port, word)
                .unwrap_or_else(|e| panic!("at_wini: sys_outw({port:#06x}) failed: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pio::{REG_DATA, REG_LDH};
    use minix_sys::syscall::{
        _DIO_BYTE, _DIO_INPUT, _DIO_OUTPUT, _DIO_WORD, CannedKernelCallTransport,
    };
    use minix_types::Message;

    const BASE_CMD: u16 = 0x1F0;

    /// Build a `SYS_DEVIO` reply message whose input-value arm carries `value`
    /// (mirrors how the kernel `dispatch_devio` Input arm writes `value@0`).
    fn devio_reply(value: u32) -> Message {
        let mut reply = Message {
            m_type: 0,
            ..Default::default()
        };
        // SAFETY(test): 取 union 字段的 &mut 写回值——内核 Input 臂会填同一槽。
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_devio };
        arm.value = value;
        reply
    }

    #[test]
    fn inb_outb_map_to_byte_devio() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply_message(devio_reply(0x50)); // STATUS: DRDY set, BSY clear
        canned.reply(0); // outb ok
        let mut bus = DevioIoBus::new(canned);

        assert_eq!(bus.inb(0x3F6), 0x50);
        bus.outb(BASE_CMD + REG_LDH, 0xE0);

        let canned = &bus.transport;
        let sent = canned.sent.borrow();
        // SAFETY(test): read back the two request arms to check width/direction.
        let first = unsafe { sent[0].m_u.m_lsys_krn_sys_devio };
        assert_eq!(first.request, _DIO_INPUT | _DIO_BYTE);
        assert_eq!(first.port, 0x3F6);
        let second = unsafe { sent[1].m_u.m_lsys_krn_sys_devio };
        assert_eq!(second.request, _DIO_OUTPUT | _DIO_BYTE);
        assert_eq!(second.port, (BASE_CMD + REG_LDH) as i32);
        assert_eq!(second.value, 0xE0);
    }

    #[test]
    fn in_words_issues_one_word_input_per_slot() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply_message(devio_reply(0x1111));
        canned.reply_message(devio_reply(0x2222));
        canned.reply_message(devio_reply(0x3333));
        let mut bus = DevioIoBus::new(canned);

        let mut dst = [0u16; 3];
        bus.in_words(BASE_CMD + REG_DATA, &mut dst);
        assert_eq!(dst, [0x1111, 0x2222, 0x3333]);

        let canned = &bus.transport;
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 3);
        for msg in sent.iter() {
            // SAFETY(test): every slot is a distinct 16-bit input on the data reg.
            let d = unsafe { msg.m_u.m_lsys_krn_sys_devio };
            assert_eq!(d.request, _DIO_INPUT | _DIO_WORD);
            assert_eq!(d.port, (BASE_CMD + REG_DATA) as i32);
        }
    }

    #[test]
    fn out_words_issues_one_word_output_per_source() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        canned.reply(0);
        let mut bus = DevioIoBus::new(canned);

        bus.out_words(BASE_CMD + REG_DATA, &[0xAABB, 0xCCDD]);

        let canned = &bus.transport;
        let sent = canned.sent.borrow();
        assert_eq!(sent.len(), 2);
        // SAFETY(test): read back the two output words in order.
        let a = unsafe { sent[0].m_u.m_lsys_krn_sys_devio };
        let b = unsafe { sent[1].m_u.m_lsys_krn_sys_devio };
        assert_eq!((a.request, a.value), (_DIO_OUTPUT | _DIO_WORD, 0xAABB));
        assert_eq!((b.request, b.value), (_DIO_OUTPUT | _DIO_WORD, 0xCCDD));
    }
}

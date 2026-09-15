//! Event queue + the two read functions (doc 06-event-buf).
//!
//! C: `device.c:75-140` (`devman_device_add_event`,
//! `devman_device_remove_event`), `:142-183` (`devman_event_read`,
//! `devman_static_info_read`).
//!
//! Queue direction, read carefully (06 §2.3): producers `TAILQ_INSERT_HEAD`
//! (newest at head); consumers take `TAILQ_LAST` (oldest) — FIFO.
//! Drain protocol (06 §2.4): a read yielding **zero bytes with an event
//! present** removes and frees it. With fd offsets advancing across
//! `read()` syscalls, that means *data read, then EOF read to ACK* —
//! two reads per event, not one.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use minix_types::Errno;

use crate::buf::Buf;
use crate::structs::Event;

/// FIFO event queue: `push` = newest, `oldest`/`consume` = oldest-first.
/// (C `TAILQ_INSERT_HEAD` + `TAILQ_LAST`: head ≡ back, last ≡ front.)
/// Derives ride on [`InodeContent`] (the queue hangs off an inode).
#[derive(Default, Debug, PartialEq, Eq)]
pub struct EventQueue {
    queue: VecDeque<Event>,
}

impl EventQueue {
    pub fn new() -> Self {
        EventQueue {
            queue: VecDeque::new(),
        }
    }

    /// C: `TAILQ_INSERT_HEAD` (device.c:~100/~130) — newest first in,
    /// oldest out. Allocation failure (`malloc` NULL → C `panic`) is
    /// `ENOMEM` here ([ARCH:A-7]).
    pub fn push(&mut self, ev: Event) -> Result<(), Errno> {
        self.queue.try_reserve(1).map_err(|_| Errno::ENOMEM)?;
        self.queue.push_back(ev);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// C: `devman_event_read` (device.c:142-168) — format the **oldest**
    /// event through `buf` (offset-aware); consume it **iff** the result
    /// is empty while an event was present (`r == 0` removal, :160-164).
    /// Empty queue → empty (EOF, nothing consumed). `buf` is lent by the
    /// caller (VTreeFs owns one and reuses it — C reuses its statics,
    /// DM-P1-5/P3-3); infallible by construction.
    pub fn read_oldest(&mut self, buf: &mut Buf, len: usize, offset: usize) -> Vec<u8> {
        buf.init(len, offset);
        let had = !self.queue.is_empty();
        // Oldest = front (push_back appends newest; C HEAD ≡ back).
        if let Some(ev) = self.queue.front() {
            buf.printf("%s", ev.text());
        }
        let out = buf.result().to_vec();
        if had && out.is_empty() {
            self.queue.pop_front();
        }
        out
    }

    /// C: `devman_static_info_read` (device.c:173-183) — the text plus a
    /// **newline** (`buf_printf("%s\n", …)`; event lines carry no `\n`).
    /// Pure (no queue, no consumption), infallible.
    pub fn read_static(buf: &mut Buf, text: &str, len: usize, offset: usize) -> Vec<u8> {
        buf.init(len, offset);
        buf.printf("%s\n", text);
        buf.result().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> Event {
        Event::new(s).unwrap()
    }

    fn drain(q: &mut EventQueue, len: usize, offset: usize) -> Vec<u8> {
        let mut buf = Buf::new().unwrap();
        q.read_oldest(&mut buf, len, offset)
    }

    #[test]
    fn fifo_oldest_first() {
        let mut q = EventQueue::new();
        q.push(ev("ADD ./devices/a/ 0x00000001")).unwrap();
        q.push(ev("ADD ./devices/b/ 0x00000002")).unwrap();
        // Oldest out first (C TAILQ_LAST).
        assert_eq!(
            drain(&mut q, 128, 0),
            b"ADD ./devices/a/ 0x00000001"
        );
        // Non-consuming read (r > 0): still there.
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn drain_takes_two_reads() {
        // 06 §2.4: data read, then EOF read to ACK-consume.
        let mut q = EventQueue::new();
        q.push(ev("ADD ./devices/a/ 0x00000001")).unwrap();
        let data = drain(&mut q, 128, 0);
        assert!(!data.is_empty());
        assert_eq!(q.len(), 1);
        let ack = drain(&mut q, 128, data.len());
        assert!(ack.is_empty());
        assert_eq!(q.len(), 0);
        // Empty queue reads empty, consumes nothing.
        assert!(drain(&mut q, 128, 0).is_empty());
    }

    #[test]
    fn static_appends_newline() {
        // C: buf_printf("%s\n", n->data) (device.c:179).
        let mut buf = Buf::new().unwrap();
        assert_eq!(EventQueue::read_static(&mut buf, "USB_DEV", 64, 0), b"USB_DEV\n");
        // Offset applies the same skip funnel.
        assert_eq!(EventQueue::read_static(&mut buf, "USB_DEV", 64, 4), b"DEV\n");
    }
}

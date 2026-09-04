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
#[derive(Default)]
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
    /// event through `Buf` (offset-aware); consume it **iff** the result
    /// is empty while an event was present (`r == 0` removal, :160-164).
    /// Empty queue → empty (EOF, nothing consumed).
    pub fn read_oldest(
        &mut self,
        len: usize,
        offset: usize,
    ) -> Result<Vec<u8>, Errno> {
        let mut buf = Buf::new()?;
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
        Ok(out)
    }

    /// C: `devman_static_info_read` (device.c:173-183) — the text plus a
    /// **newline** (`buf_printf("%s\n", …)`; event lines carry no `\n`).
    /// Pure (no queue, no consumption).
    pub fn read_static(text: &str, len: usize, offset: usize) -> Result<Vec<u8>, Errno> {
        let mut buf = Buf::new()?;
        buf.init(len, offset);
        buf.printf("%s\n", text);
        Ok(buf.result().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> Event {
        Event::new(s).unwrap()
    }

    #[test]
    fn fifo_oldest_first() {
        let mut q = EventQueue::new();
        q.push(ev("ADD ./devices/a/ 0x00000001")).unwrap();
        q.push(ev("ADD ./devices/b/ 0x00000002")).unwrap();
        // Oldest out first (C TAILQ_LAST).
        assert_eq!(
            q.read_oldest(128, 0).unwrap(),
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
        let data = q.read_oldest(128, 0).unwrap();
        assert!(!data.is_empty());
        assert_eq!(q.len(), 1);
        let ack = q.read_oldest(128, data.len()).unwrap();
        assert!(ack.is_empty());
        assert_eq!(q.len(), 0);
        // Empty queue reads empty, consumes nothing.
        assert!(q.read_oldest(128, 0).unwrap().is_empty());
    }

    #[test]
    fn static_appends_newline() {
        // C: buf_printf("%s\n", n->data) (device.c:179).
        assert_eq!(
            EventQueue::read_static("USB_DEV", 64, 0).unwrap(),
            b"USB_DEV\n"
        );
        // Offset applies the same skip funnel.
        assert_eq!(
            EventQueue::read_static("USB_DEV", 64, 4).unwrap(),
            b"DEV\n"
        );
    }
}

//! Event ring-buffer mechanics: which bytes travel, in which order.
//!
//! C: `input_copy_events` (`minix3/minix/servers/input/input.c:130-157`).
//! A read takes the oldest buffered events and copies them to the caller in
//! two segments when the ring wraps: first from the tail to the end of the
//! array, then from the start of the array. This module owns that geometry
//! as pure functions — no grants, no system calls. The transport (a future
//! layer) performs the two copies described by [`CopyPlan`]; tests replay
//! the geometry against hand-computed C examples.
//!
//! The overflow rule ("full buffer overwrites the oldest event") belongs to
//! the event producer (document 09), not to the copy path: copying only
//! ever removes from the tail. This module never writes the head.
//!
//! Corresponding document: `07-input-read-suspend.md`.

use crate::error::InputError;
use crate::event::InputEvent;
use crate::structs::{EVENT_BUFFER_SIZE, InputDevice};
use alloc::vec::Vec;

/// How one copy is split across the ring wrap.
///
/// C: the two `sys_safecopyto` calls (`input.c:144-151`). `first_len` events
/// travel from the tail to the end of the array; `second_len` events (zero
/// when nothing wraps) travel from the start of the array. Both counts are
/// in events; multiply by the event size for bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyPlan {
    /// Events in the first segment (tail to array end, possibly all).
    pub first_len: u32,
    /// Events in the second segment (array start, zero when no wrap).
    pub second_len: u32,
    /// Tail position after the copy.
    pub new_tail: u32,
    /// Buffered count after the copy.
    pub new_count: u32,
}

impl CopyPlan {
    /// Total events moved (both segments).
    pub const fn event_total(self) -> u32 {
        self.first_len + self.second_len
    }
}

/// Plans the copy of `event_count` events out of a ring buffer.
///
/// `tail` is where the oldest buffered event sits, `count` how many are
/// buffered. Mirrors the C arithmetic (`input.c:140-142`):
/// `wrap_left = tail + event_count - EVENTBUF_SIZE` decides whether the
/// copy wraps, and the first segment covers the smaller of "everything
/// asked" and "everything up to the array end".
///
/// Asking for more than is buffered cannot happen on the read path (the
/// caller clamps first, `input.c:195-196`). C answers such a call with a
/// crash (`panic`, `input.c:137-138`); Rust answers with an error instead
/// (architecture evolution A-11): crashing the server over a bookkeeping
/// disagreement punishes every reader for one bad call. The error is
/// returned uniformly in all builds — deliberately no `debug_assert`:
/// asserting in debug while returning in release would give the same call
/// two different behaviors, and the debug one would be the very crash being
/// removed. The contract ("ask no more than buffered") is pinned by
/// `test_plan_copy_shortage_is_an_error_not_a_crash` instead.
pub fn plan_copy(tail: u32, count: u32, event_count: u32) -> Result<CopyPlan, InputError> {
    if event_count > count {
        return Err(InputError::InputOutput);
    }
    let capacity = EVENT_BUFFER_SIZE as u32;
    let wrap_left = tail as i64 + event_count as i64 - capacity as i64;
    let first_len = if wrap_left <= 0 {
        event_count
    } else {
        capacity - tail
    };
    let second_len = if wrap_left > 0 { wrap_left as u32 } else { 0 };
    Ok(CopyPlan {
        first_len,
        second_len,
        new_tail: (tail + event_count) % capacity,
        new_count: count - event_count,
    })
}

/// Applies a finished copy to the device: advances the tail, shrinks the
/// count.
///
/// C: `input.c:153-154`. Called after the transport reports both segments
/// copied; a failed transport copy must not advance (the events stay
/// buffered for a retry or a cancel).
pub fn apply_copy(device: &mut InputDevice, plan: &CopyPlan) {
    device.tail = plan.new_tail;
    device.count = plan.new_count;
}

/// Lists the buffered events oldest-first, without removing them.
///
/// Used by tests and (later) by wake-up paths that need to inspect the
/// queue. The ring order is `tail, tail+1, …, wrapping at the array end`.
pub fn drain_ordered(
    events: &[InputEvent; EVENT_BUFFER_SIZE],
    tail: u32,
    count: u32,
) -> Vec<InputEvent> {
    let capacity = EVENT_BUFFER_SIZE as u32;
    let mut out = Vec::new();
    let mut taken = 0;
    while taken < count {
        out.push(events[((tail + taken) % capacity) as usize]);
        taken += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(tail: u32, count: u32) -> [InputEvent; EVENT_BUFFER_SIZE] {
        // Fills the ring so event `k` (0-based oldest) carries code `k`:
        // slot (tail + k) % 32 holds code k.
        let mut events = [InputEvent::zero(); EVENT_BUFFER_SIZE];
        let mut k = 0;
        while k < count {
            let slot = ((tail + k) % EVENT_BUFFER_SIZE as u32) as usize;
            events[slot].code = k as u16;
            k += 1;
        }
        events
    }

    #[test]
    fn test_plan_copy_without_wrap_matches_c() {
        // C: tail = 5, take 3 → wrap_left = 5 + 3 - 32 < 0 → one segment.
        let plan = plan_copy(5, 10, 3).unwrap();
        assert_eq!(
            plan,
            CopyPlan {
                first_len: 3,
                second_len: 0,
                new_tail: 8,
                new_count: 7,
            }
        );
        assert_eq!(plan.event_total(), 3);
    }

    #[test]
    fn test_plan_copy_with_wrap_matches_c() {
        // C: tail = 30, take 5 → wrap_left = 3 → 2 + 3 segments.
        let plan = plan_copy(30, 8, 5).unwrap();
        assert_eq!(
            plan,
            CopyPlan {
                first_len: 2,
                second_len: 3,
                new_tail: 3,
                new_count: 3,
            }
        );
        assert_eq!(plan.event_total(), 5);
    }

    #[test]
    fn test_plan_copy_shortage_is_an_error_not_a_crash() {
        // C: `panic("input_copy_events: not enough input is ready")`
        // (input.c:137-138). Rust (A-11): an explicit error instead.
        assert_eq!(plan_copy(0, 2, 3), Err(InputError::InputOutput));
        // Taking nothing is a legal empty plan (C copies zero bytes).
        let empty = plan_copy(7, 4, 0).unwrap();
        assert_eq!(empty.event_total(), 0);
        assert_eq!((empty.new_tail, empty.new_count), (7, 4));
    }

    #[test]
    fn test_apply_copy_advances_tail_and_count() {
        // C: input.c:153-154.
        use crate::structs::InputTable;
        let mut table = InputTable::fresh();
        let device = &mut table.devices[0];
        device.tail = 30;
        device.count = 8;
        let plan = plan_copy(30, 8, 5).unwrap();
        apply_copy(device, &plan);
        assert_eq!((device.tail, device.count), (3, 3));
    }

    #[test]
    fn test_drain_ordered_reads_oldest_first_across_wrap() {
        let events = staged(30, 5);
        let out = drain_ordered(&events, 30, 5);
        let codes: Vec<u16> = out.iter().map(|event| event.code).collect();
        assert_eq!(codes, [0, 1, 2, 3, 4]);
    }
}

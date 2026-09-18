//! Socket object table: the machinery behind the naming half.
//!
//! C correspondence: the sock object lifecycle and event processing
//! (`minix3/minix/lib/libsockevent/sockevent.c` — raise rules around
//! :770-946, the CONNECT-implies-SEND linkage at :781-782, the OOB
//! never-tested rule at :817, error wake-all at :946-963), the suspended
//! call records (`sockevent_proc.h:4-19`: wake event, timeout, caller),
//! and the nonblock rule (`sockdriver.c:573`: a nonblocking request never
//! gets a call record, so it can never suspend).
//!
//! The structural difference from C is deliberate: libsockevent drives the
//! driver through callbacks and needs a reentrancy flag (`sockevent_working`,
//! `sockevent.c:915-941`) plus a deferred queue to survive callback
//! re-entry. This machinery has no callbacks. The service calls
//! [`SockTable::raise`] and receives a list of [`WakeAction`] values to
//! perform; raising while processing those actions is just another call
//! returning more actions, so the flag and the queue have nothing to
//! protect. Semantics preserved: SEV_CLOSE is never deferred — closing is
//! its own method ([`SockTable::close`]) that removes the object at once,
//! and [`SockTable::raise`] refuses the bit by construction.

use minix_sockdriver::sdev::{may_suspend, SdevRequest, SDEV_OP_RD, SDEV_OP_WR};
use minix_sockdriver::sockevent::{hash_slot, SocketEvent, HASH_SLOTS};
use crate::sockid::SockId;
use alloc::vec::Vec;
use minix_types::{Endpoint, EINVAL};

/// One suspended request: who is waiting, which reply they expect, which
/// event completes it, and when it gives up.
///
/// `wake` is the SEV_* mask that resumes the call. The C framework derives
/// it from the request kind when it files the call record
/// (`sockevent_proc.h:5`); here the service passes it explicitly, which
/// also lets an ioctl name several waking events. `deadline` mirrors the
/// per-call timer (`spr_timer`/`spr_time`); `None` waits forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    request: SdevRequest,
    caller: Endpoint,
    wake: u32,
    deadline: Option<u64>,
}

impl Continuation {
    /// File a suspended request. `None` unless the request kind may
    /// suspend at all (the `may_suspend` table, `sockdriver.c:8-26`) —
    /// a nonblocking or always-answered request has no continuation.
    pub const fn new(
        request: SdevRequest,
        caller: Endpoint,
        wake: u32,
        deadline: Option<u64>,
    ) -> Option<Continuation> {
        if !may_suspend(request) {
            return None;
        }
        Some(Continuation { request, caller, wake, deadline })
    }

    /// The suspended request kind.
    pub const fn request(&self) -> SdevRequest {
        self.request
    }

    /// The caller awaiting the reply.
    pub const fn caller(&self) -> Endpoint {
        self.caller
    }

    /// The waking event mask.
    pub const fn wake(&self) -> u32 {
        self.wake
    }

    /// The give-up moment in milliseconds, if any.
    pub const fn deadline(&self) -> Option<u64> {
        self.deadline
    }
}

/// One registered select waiter: a socket has at most one
/// (`sockdriver.h:56`, the single `ss_endpt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectWait {
    caller: Endpoint,
    ops: u8,
}

impl SelectWait {
    /// The caller awaiting the select reply.
    pub const fn caller(&self) -> Endpoint {
        self.caller
    }

    /// The operation bits being waited for (`SDEV_OP_*`).
    pub const fn ops(&self) -> u8 {
        self.ops
    }
}

/// One live socket object: identity plus everything the framework
/// bookkeeps for it.
#[derive(Debug, Clone)]
pub struct SockEntry {
    id: SockId,
    flags: u32,
    error: Option<i32>,
    continuations: Vec<Continuation>,
    select_wait: Option<SelectWait>,
    alarm: Option<u64>,
}

impl SockEntry {
    /// The object's identifier.
    pub const fn id(&self) -> SockId {
        self.id
    }

    /// The socket flags (`SFL_*` mask).
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Set flag bits (read shutdown, closing, timer armed, ...).
    pub fn set_flags(&mut self, mask: u32) {
        self.flags |= mask;
    }

    /// Clear flag bits.
    pub fn clear_flags(&mut self, mask: u32) {
        self.flags &= !mask;
    }

    /// The pending error, if one was set (a later error overrides an
    /// earlier one, `sockevent.c:952-953`).
    pub const fn error(&self) -> Option<i32> {
        self.error
    }
}

/// One thing the service must do after an event or a timer tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeAction {
    /// Re-run a suspended request. Afterwards the service either replies
    /// by the request's reply shape or files the continuation again.
    Resume { id: SockId, continuation: Continuation },
    /// Re-test a registered select waiter against current readiness.
    /// Satisfied: send the select reply and consume the waiter with
    /// [`SockTable::take_select`]. Not satisfied: do nothing, the waiter
    /// stays registered.
    RetestSelect { id: SockId, ops: u8 },
    /// The object's alarm fired (the SFL_TIMER heartbeat the service uses
    /// for stack polling); one-shot, rearm with [`SockTable::arm_alarm`].
    Alarm { id: SockId },
    /// A suspended call with a deadline ran out of time. The service
    /// replies by the request's reply shape with the timeout error.
    TimedOut { id: SockId, continuation: Continuation },
}

/// The socket object table: two hundred fifty-six hash slots, each a short
/// chain (`sockevent.c:12-14`, [`minix_sockdriver::sockevent::hash_slot`]).
#[derive(Debug)]
pub struct SockTable {
    slots: Vec<Vec<SockEntry>>,
}

impl SockTable {
    /// An empty table with the fixed slot count.
    pub fn new() -> SockTable {
        SockTable { slots: (0..HASH_SLOTS).map(|_| Vec::new()).collect() }
    }

    fn slot_of(&self, id: SockId) -> &Vec<SockEntry> {
        &self.slots[hash_slot(id.raw() as u32) as usize]
    }

    fn slot_mut(&mut self, id: SockId) -> &mut Vec<SockEntry> {
        let slot = hash_slot(id.raw() as u32) as usize;
        &mut self.slots[slot]
    }

    /// Register a freshly opened socket object. Fails when the id is
    /// already live.
    pub fn add(&mut self, id: SockId) -> Result<(), i32> {
        if self.contains(id) {
            return Err(EINVAL);
        }
        let entry =
            SockEntry { id, flags: 0, error: None, continuations: Vec::new(), select_wait: None, alarm: None };
        self.slot_mut(id).push(entry);
        Ok(())
    }

    /// Whether the id has a live object.
    pub fn contains(&self, id: SockId) -> bool {
        self.slot_of(id).iter().any(|entry| entry.id == id)
    }

    /// Borrow a live object.
    pub fn entry(&self, id: SockId) -> Option<&SockEntry> {
        self.slot_of(id).iter().find(|entry| entry.id == id)
    }

    /// Close a socket: remove the object at once, pending continuations
    /// and all. C defers nothing on SEV_CLOSE precisely so drivers can
    /// recycle objects immediately (`sockevent.c:899-909`).
    pub fn close(&mut self, id: SockId) -> Option<SockEntry> {
        let slot = self.slot_mut(id);
        let position = slot.iter().position(|entry| entry.id == id)?;
        Some(slot.remove(position))
    }

    /// File a suspended request on a live socket.
    pub fn suspend(&mut self, id: SockId, continuation: Continuation) -> Result<(), i32> {
        let entry = self.entry_mut(id)?;
        entry.continuations.push(continuation);
        Ok(())
    }

    /// Raise events on a socket and get back the work they unlock.
    ///
    /// Linkage rule: a completed connect always also implies writability
    /// (`sockevent.c:781-782`). Every suspended call whose waking mask
    /// intersects the raised mask is handed out as a
    /// [`WakeAction::Resume`]. A registered select waiter whose operation
    /// bits could possibly be satisfied produces one
    /// [`WakeAction::RetestSelect`]: read interest needs accept or receive
    /// events, write interest needs send, and error interest is never
    /// retested because the C framework itself hardwires that test off
    /// (`sockevent.c:817`).
    pub fn raise(&mut self, id: SockId, mask: u32) -> Vec<WakeAction> {
        let mut actions = Vec::new();
        let Ok(entry) = self.entry_mut(id) else {
            return actions;
        };

        let mut effective = mask;
        if effective & SocketEvent::Connect.bits() != 0 {
            effective |= SocketEvent::Send.bits();
        }

        let mut remaining = Vec::new();
        for continuation in core::mem::take(&mut entry.continuations) {
            if continuation.wake & effective != 0 {
                actions.push(WakeAction::Resume { id, continuation });
            } else {
                remaining.push(continuation);
            }
        }
        entry.continuations = remaining;

        if let Some(wait) = entry.select_wait {
            let mut candidates = 0u8;
            if effective & (SocketEvent::Accept.bits() | SocketEvent::Receive.bits()) != 0 {
                candidates |= wait.ops & SDEV_OP_RD;
            }
            if effective & SocketEvent::Send.bits() != 0 {
                candidates |= wait.ops & SDEV_OP_WR;
            }
            if candidates != 0 {
                actions.push(WakeAction::RetestSelect { id, ops: candidates });
            }
        }

        actions
    }

    /// Set the pending error and wake every waitable interest: bind,
    /// connect, send, and receive all resume so callers observe the
    /// failure (`sockevent.c:946-963`).
    pub fn set_error(&mut self, id: SockId, error: i32) -> Vec<WakeAction> {
        let Ok(entry) = self.entry_mut(id) else {
            return Vec::new();
        };
        entry.error = Some(error);
        let wake = SocketEvent::Bind.bits()
            | SocketEvent::Connect.bits()
            | SocketEvent::Send.bits()
            | SocketEvent::Receive.bits();
        self.raise(id, wake)
    }

    /// Register the select waiter, displacing any previous one. The
    /// immediate-satisfaction test is the service's job: register only
    /// after the service found nothing ready.
    pub fn register_select(
        &mut self,
        id: SockId,
        caller: Endpoint,
        ops: u8,
    ) -> Result<Option<SelectWait>, i32> {
        let entry = self.entry_mut(id)?;
        Ok(entry.select_wait.replace(SelectWait { caller, ops }))
    }

    /// Consume the select waiter after the service re-tested it as
    /// satisfied (the service sends the second select reply).
    pub fn take_select(&mut self, id: SockId) -> Option<SelectWait> {
        self.entry_mut(id).ok().and_then(|entry| entry.select_wait.take())
    }

    /// Cancel every suspended request and select waiter filed by one
    /// caller. True when something was actually cancelled.
    pub fn cancel(&mut self, id: SockId, caller: Endpoint) -> bool {
        let Ok(entry) = self.entry_mut(id) else {
            return false;
        };
        let before = entry.continuations.len();
        entry.continuations.retain(|cont| cont.caller != caller);
        let removed_call = entry.continuations.len() != before;
        let removed_select = entry.select_wait.take_if(|wait| wait.caller == caller).is_some();
        removed_call || removed_select
    }

    /// Arm the object's alarm (one heartbeat deadline).
    pub fn arm_alarm(&mut self, id: SockId, deadline_millis: u64) -> Result<(), i32> {
        self.entry_mut(id)?.alarm = Some(deadline_millis);
        Ok(())
    }

    /// Disarm the object's alarm.
    pub fn disarm_alarm(&mut self, id: SockId) -> Result<(), i32> {
        self.entry_mut(id)?.alarm = None;
        Ok(())
    }

    /// Process due timers across the whole table: expired alarms become
    /// one-shot [`WakeAction::Alarm`] values, expired call deadlines
    /// become [`WakeAction::TimedOut`] with the continuation handed back.
    pub fn poll_timers(&mut self, now_millis: u64) -> Vec<WakeAction> {
        let mut actions = Vec::new();
        for slot in &mut self.slots {
            for entry in slot.iter_mut() {
                if entry.alarm.is_some_and(|deadline| now_millis >= deadline) {
                    entry.alarm = None;
                    actions.push(WakeAction::Alarm { id: entry.id });
                }
                let mut remaining = Vec::new();
                for continuation in core::mem::take(&mut entry.continuations) {
                    match continuation.deadline {
                        Some(deadline) if now_millis >= deadline => {
                            actions.push(WakeAction::TimedOut { id: entry.id, continuation });
                        }
                        _ => remaining.push(continuation),
                    }
                }
                entry.continuations = remaining;
            }
        }
        actions
    }

    fn entry_mut(&mut self, id: SockId) -> Result<&mut SockEntry, i32> {
        self.slot_mut(id)
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(EINVAL)
    }
}

impl Default for SockTable {
    fn default() -> Self {
        SockTable::new()
    }
}

/// Compile-time reminder that the flag words stay usable as `SFL_*` masks.
#[cfg(test)]
const _: () = assert!(minix_sockdriver::sockevent::SocketFlag::Timer.bits() == 0x10);

#[cfg(test)]
mod tests {
    use super::*;
    use minix_sockdriver::sdev::SdevRequest::{Accept, Connect, Receive, Send};
    use minix_sockdriver::sockevent::{SocketEvent, SocketFlag};

    const CALLER_A: Endpoint = Endpoint(10);
    const CALLER_B: Endpoint = Endpoint(20);
    const ID: SockId = match SockId::from_class(crate::sockid::SockClass::Udp, 1) {
        Some(id) => id,
        None => panic!("udp id 1 fits"),
    };

    fn recv_cont(caller: Endpoint) -> Continuation {
        Continuation::new(Receive, caller, SocketEvent::Receive.bits(), None).expect("recv suspends")
    }

    #[test]
    fn test_suspend_then_raise_resumes_matching_call() {
        let mut table = SockTable::new();
        table.add(ID).expect("add");
        table.suspend(ID, recv_cont(CALLER_A)).expect("suspend");

        let actions = table.raise(ID, SocketEvent::Receive.bits());
        assert_eq!(actions.len(), 1, "一次事件唤醒一条续作");
        match &actions[0] {
            WakeAction::Resume { id, continuation } => {
                assert!(*id == ID);
                assert!(*continuation == recv_cont(CALLER_A));
                assert!(continuation.request() == Receive);
                assert!(continuation.caller() == CALLER_A);
            }
            other => panic!("期望 Resume，得到 {other:?}"),
        }
        assert!(table.entry(ID).expect("still live").error().is_none());
        assert!(
            table.entry(ID).expect("live").flags() == 0,
            "事件泵不动标志位"
        );
    }

    #[test]
    fn test_connect_event_implies_send_wakeup() {
        let mut table = SockTable::new();
        table.add(ID).expect("add");
        let send_cont = Continuation::new(Send, CALLER_A, SocketEvent::Send.bits(), None)
            .expect("send suspends");
        table.suspend(ID, send_cont).expect("suspend");

        let actions = table.raise(ID, SocketEvent::Connect.bits());
        assert_eq!(actions.len(), 1, "接通即 imply 可写（sockevent.c:781-782）");
        assert!(matches!(actions[0], WakeAction::Resume { .. }));
    }

    #[test]
    fn test_error_overrides_and_wakes_data_interests_only() {
        let mut table = SockTable::new();
        table.add(ID).expect("add");
        table.suspend(
            ID,
            Continuation::new(Accept, CALLER_A, SocketEvent::Accept.bits(), None)
                .expect("accept suspends"),
        )
        .expect("suspend");
        table
            .suspend(
                ID,
                Continuation::new(Connect, CALLER_B, SocketEvent::Connect.bits(), None)
                    .expect("connect suspends"),
            )
            .expect("suspend");

        let actions = table.set_error(ID, -12);
        assert_eq!(actions.len(), 1, "只有连接续作被唤醒");
        assert!(
            matches!(&actions[0], WakeAction::Resume { continuation, .. }
                if continuation.request() == Connect),
            "错误唤醒集不含接收兴趣（sockevent.c:963 的 C 形状）"
        );
        assert_eq!(table.entry(ID).expect("live").error(), Some(-12));
        table.set_error(ID, -1);
        assert_eq!(table.entry(ID).expect("live").error(), Some(-1), "后错覆盖前错");
        assert_eq!(table.entry(ID).expect("live").continuations.len(), 1, "接收续作仍在");
    }

    #[test]
    fn test_close_removes_entry_with_pending_work_immediately() {
        let mut table = SockTable::new();
        table.add(ID).expect("add");
        table.suspend(ID, recv_cont(CALLER_A)).expect("suspend");
        table.register_select(ID, CALLER_B, SDEV_OP_RD).expect("register");

        let removed = table.close(ID).expect("removed");
        assert!(removed.id() == ID);
        assert_eq!(removed.continuations.len(), 1, "挂起续作随对象带走");
        assert!(removed.select_wait.is_some());
        assert!(!table.contains(ID), "close 立即回收，绝不定时");
    }

    #[test]
    fn test_select_retests_until_taken() {
        let mut table = SockTable::new();
        table.add(ID).expect("add");
        table.register_select(ID, CALLER_A, SDEV_OP_RD | SDEV_OP_WR).expect("register");

        let actions = table.raise(ID, SocketEvent::Send.bits());
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(actions[0], WakeAction::RetestSelect { ops: 0x02, .. }),
            "只有写兴趣可能被发送事件满足"
        );

        let actions = table.raise(ID, SocketEvent::Receive.bits());
        assert_eq!(actions.len(), 1, "等待者未被确认取走前持续在册");
        assert!(matches!(actions[0], WakeAction::RetestSelect { ops: 0x01, .. }));

        let wait = table.take_select(ID).expect("服务确认满足后取走");
        assert!(wait.caller() == CALLER_A);
        assert!(table.raise(ID, SocketEvent::Receive.bits()).is_empty(), "取走后不再重测");
    }

    #[test]
    fn test_cancel_removes_matching_caller_only() {
        let mut table = SockTable::new();
        table.add(ID).expect("add");
        table.suspend(ID, recv_cont(CALLER_A)).expect("suspend a");
        table
            .suspend(
                ID,
                Continuation::new(Send, CALLER_B, SocketEvent::Send.bits(), None).expect("send"),
            )
            .expect("suspend b");
        table.register_select(ID, CALLER_A, SDEV_OP_RD).expect("register");

        assert!(table.cancel(ID, CALLER_B), "撤掉乙的发送续作");
        assert!(table.cancel(ID, CALLER_A), "撤掉甲的接收续作与选择等待");
        assert!(!table.cancel(ID, CALLER_A), "再撤无事可撤");
        let entry = table.entry(ID).expect("live");
        assert!(entry.continuations.is_empty());
    }

    #[test]
    fn test_timers_fire_once_and_time_out_calls() {
        let mut table = SockTable::new();
        table.add(ID).expect("add");
        table.arm_alarm(ID, 1_000).expect("arm");
        table
            .suspend(
                ID,
                Continuation::new(Receive, CALLER_A, SocketEvent::Receive.bits(), Some(500))
                    .expect("recv with timeout"),
            )
            .expect("suspend");

        let actions = table.poll_timers(400);
        assert!(actions.is_empty(), "都未到期");

        let actions = table.poll_timers(600);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], WakeAction::TimedOut { .. }), "续作超时交回服务");

        let actions = table.poll_timers(1_100);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], WakeAction::Alarm { .. }), "闹钟一次性触发");
        let actions = table.poll_timers(1_200);
        assert!(actions.is_empty(), "闹钟不再重复");
    }

    #[test]
    fn test_chained_slot_holds_two_live_objects() {
        let mut collision = None;
        'outer: for raw_a in 0..512u32 {
            for raw_b in (raw_a + 1)..512u32 {
                if hash_slot(raw_a) == hash_slot(raw_b) {
                    collision = Some((raw_a, raw_b));
                    break 'outer;
                }
            }
        }
        let (raw_a, raw_b) = collision.expect("低位必有一对同槽标识");
        let id_a = SockId::from_raw(raw_a as i32).expect("id a");
        let id_b = SockId::from_raw(raw_b as i32).expect("id b");

        let mut table = SockTable::new();
        table.add(id_a).expect("add a");
        table.add(id_b).expect("add b 同槽共存");
        table.suspend(id_a, recv_cont(CALLER_A)).expect("suspend on a");

        let actions = table.raise(id_b, SocketEvent::Receive.bits());
        assert!(actions.is_empty(), "事件不串门");
        assert!(table.close(id_a).expect("close a").continuations.len() == 1);
        assert!(table.contains(id_b), "同槽邻居安然无恙");
    }

    #[test]
    fn test_non_suspending_request_has_no_continuation() {
        assert!(Continuation::new(minix_sockdriver::sdev::SdevRequest::Socket, CALLER_A, 0, None).is_none());
        assert!(
            Continuation::new(minix_sockdriver::sdev::SdevRequest::Cancel, CALLER_A, 0, None).is_none(),
            "撤单无答复，永不挂起"
        );
        let mut table = SockTable::new();
        table.add(ID).expect("add");
        assert!(table.raise(ID, SocketEvent::Receive.bits()).is_empty());
        assert!(SocketFlag::Closing.bits() != 0);
    }
}

//! PM ↔ subscriber process event protocol.
//!
//! C: `minix3/minix/include/minix/com.h:597-619`（COMMON_RQ/RS + PROC_EVENT
//! 族）+ `minix3/minix/include/minix/syslib.h:289-293`（PROC_EVENT_EXIT/SIGNAL
//! 位掩码）+ `minix3/minix/include/minix/ipc.h:1414/1800`（mess_* 布局）。
//!
//! 对应 04-stage-pm/06-event-subscription.md 的协议层。

use crate::ipc::message::{MessLsysPmProceventmask, MessPmLsysProcEvent};
use crate::{Endpoint, Message, MessageUnion};

/// `PM_PROCEVENTMASK` — `callnr.h:53` (`PM_BASE + 40`).
pub const PM_PROCEVENTMASK: i32 = 40;

// ── COMMON 基址与 PROC_EVENT 类型 ──

/// `COMMON_RQ_BASE` — `com.h:597`。
pub const COMMON_RQ_BASE: i32 = 0xE00;

/// `COMMON_RS_BASE` — `com.h:598`。
pub const COMMON_RS_BASE: i32 = 0xE80;

/// `PROC_EVENT` — PM → 订阅者的事件通知（`COMMON_RQ_BASE + 3`，`com.h:610`）。
pub const PROC_EVENT: i32 = COMMON_RQ_BASE + 3;

/// `PROC_EVENT_REPLY` — 订阅者 → PM 的回复（`COMMON_RS_BASE + 0`，`com.h:619`）。
pub const PROC_EVENT_REPLY: i32 = COMMON_RS_BASE;

/// `PROC_EVENT_EXIT` — 进程退出事件位（`syslib.h:292`，`0x01`，可作掩码）。
pub const PROC_EVENT_EXIT: u32 = 0x01;

/// `PROC_EVENT_SIGNAL` — 进程信号事件位（`syslib.h:293`，`0x02`，可作掩码）。
pub const PROC_EVENT_SIGNAL: u32 = 0x02;

// ── 事件类型化 ──

/// 进程事件（`syslib.h:292-293` 的 `0x01`/`0x02`）。
///
/// 这两个值同时是"事件编号"与"掩码位"（注释"they form a bit mask"），因此
/// `ProcEvent::Exit as u32 == PROC_EVENT_EXIT`，且可用 `ProcEventMask` 做集合运算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ProcEvent {
    /// 进程正在退出（`EXITING` 推断，`event.c:87`）。
    Exit = PROC_EVENT_EXIT,
    /// 进程被信号解暂停（`UNPAUSED` 推断，`event.c:89`）。
    Signal = PROC_EVENT_SIGNAL,
}

impl ProcEvent {
    /// 从裸 `event` 字段解析（仅接受 `0x01` / `0x02`）。
    pub fn from_bits(bits: u32) -> Option<Self> {
        match bits {
            PROC_EVENT_EXIT => Some(Self::Exit),
            PROC_EVENT_SIGNAL => Some(Self::Signal),
            _ => None,
        }
    }

    /// 对应的掩码位（`0x01` / `0x02`）。
    pub const fn mask(self) -> u32 {
        self as u32
    }
}

bitflags::bitflags! {
    /// 进程事件掩码（`subs[i].mask`，`event.c:62`）。
    ///
    /// C 为裸 `unsigned int`，`mask & event != 0` 即匹配；Rust 用 bitflags
    /// 在类型层表达"多事件的或"。空掩码 `empty()` 表示退订（`mask==0`）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProcEventMask: u32 {
        /// 订阅 EXIT 事件（`PROC_EVENT_EXIT`）。
        const EXIT = PROC_EVENT_EXIT;
        /// 订阅 SIGNAL 事件（`PROC_EVENT_SIGNAL`）。
        const SIGNAL = PROC_EVENT_SIGNAL;
    }
}

impl ProcEventMask {
    /// 是否包含某事件（`mask & event != 0` 的类型化表达）。
    #[inline]
    pub fn contains_event(self, event: ProcEvent) -> bool {
        self.contains(ProcEventMask::from_bits_retain(event.mask()))
    }
}

// ── 消息编解码 ──

/// 构造 `PROC_EVENT` 事件通知消息（PM → 订阅者）。
///
/// C: `event.c:99-103` 的 `memset(&m,0)` + `m.m_type=PROC_EVENT` +
/// `m.m_pm_lsys_proc_event.endpt/event`。
pub fn proc_event_msg(target: Endpoint, event: ProcEvent) -> Message {
    let payload = MessPmLsysProcEvent {
        endpt: target.get(),
        event: event as u32,
        _padding: [0; 48],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: PROC_EVENT,
        m_u: MessageUnion {
            m_pm_lsys_proc_event: payload,
        },
    }
}

/// 解码 `PROC_EVENT` 消息的载荷（PM → 订阅者）。
pub fn decode_proc_event(msg: &Message) -> Option<(Endpoint, ProcEvent)> {
    if msg.m_type != PROC_EVENT {
        return None;
    }
    let pl = unsafe { msg.m_u.m_pm_lsys_proc_event };
    let ep = Endpoint(pl.endpt);
    let ev = ProcEvent::from_bits(pl.event)?;
    Some((ep, ev))
}

/// 构造 `PROC_EVENT_REPLY` 回复消息（订阅者 → PM）。
///
/// C: 订阅者收到 `PROC_EVENT` 后以 `m_pm_lsys_proc_event` 原样回送
/// （`endpt`/`event` 与收到的相同，`event.c:274-278` 的一致性检查依赖此回显）。
pub fn proc_event_reply_msg(target: Endpoint, event: ProcEvent) -> Message {
    let payload = MessPmLsysProcEvent {
        endpt: target.get(),
        event: event as u32,
        _padding: [0; 48],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: PROC_EVENT_REPLY,
        m_u: MessageUnion {
            m_pm_lsys_proc_event: payload,
        },
    }
}

/// 解码 `PROC_EVENT_REPLY` 消息的载荷（订阅者 → PM）。
pub fn decode_proc_event_reply(msg: &Message) -> Option<(Endpoint, ProcEvent)> {
    if msg.m_type != PROC_EVENT_REPLY {
        return None;
    }
    let pl = unsafe { msg.m_u.m_pm_lsys_proc_event };
    let ep = Endpoint(pl.endpt);
    let ev = ProcEvent::from_bits(pl.event)?;
    Some((ep, ev))
}

/// 构造 `PROCEVENTMASK` 订阅更新消息（订阅者 → PM）。
/// C: `event.c:179` 的 `m_in.m_lsys_pm_proceventmask.mask`。
pub fn proceventmask_msg(mask: ProcEventMask) -> Message {
    let payload = MessLsysPmProceventmask {
        mask: mask.bits(),
        _padding: [0; 52],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: PM_PROCEVENTMASK,
        m_u: MessageUnion {
            m_lsys_pm_proceventmask: payload,
        },
    }
}

/// 解码 `PROCEVENTMASK` 的掩码。
pub fn decode_proceventmask(msg: &Message) -> Option<ProcEventMask> {
    if msg.m_type != PM_PROCEVENTMASK {
        return None;
    }
    let pl = unsafe { msg.m_u.m_lsys_pm_proceventmask };
    Some(ProcEventMask::from_bits_truncate(pl.mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proc_event_constants_match_com_h() {
        assert_eq!(COMMON_RQ_BASE, 0xE00);
        assert_eq!(COMMON_RS_BASE, 0xE80);
        assert_eq!(PROC_EVENT, COMMON_RQ_BASE + 3);
        assert_eq!(PROC_EVENT_REPLY, COMMON_RS_BASE + 0);
        assert_eq!(PROC_EVENT_EXIT, 0x01);
        assert_eq!(PROC_EVENT_SIGNAL, 0x02);
    }

    #[test]
    fn test_proc_event_mask_contains() {
        let empty = ProcEventMask::empty();
        assert!(!empty.contains_event(ProcEvent::Exit));
        assert!(!empty.contains_event(ProcEvent::Signal));

        let both = ProcEventMask::EXIT | ProcEventMask::SIGNAL;
        assert!(both.contains_event(ProcEvent::Exit));
        assert!(both.contains_event(ProcEvent::Signal));

        let only_exit = ProcEventMask::EXIT;
        assert!(only_exit.contains_event(ProcEvent::Exit));
        assert!(!only_exit.contains_event(ProcEvent::Signal));
    }

    #[test]
    fn test_proc_event_msg_roundtrip() {
        let ep = Endpoint::from_generation_slot(1, 5);
        for ev in [ProcEvent::Exit, ProcEvent::Signal] {
            let msg = proc_event_msg(ep, ev);
            assert_eq!(msg.m_type, PROC_EVENT);
            let (dec_ep, dec_ev) = decode_proc_event(&msg).unwrap();
            assert_eq!(dec_ep, ep);
            assert_eq!(dec_ev, ev);

            let reply = proc_event_reply_msg(ep, ev);
            assert_eq!(reply.m_type, PROC_EVENT_REPLY);
            let (r_ep, r_ev) = decode_proc_event_reply(&reply).unwrap();
            assert_eq!(r_ep, ep);
            assert_eq!(r_ev, ev);
        }
    }

    #[test]
    fn test_proceventmask_msg_roundtrip() {
        for mask in [
            ProcEventMask::empty(),
            ProcEventMask::EXIT,
            ProcEventMask::SIGNAL,
            ProcEventMask::EXIT | ProcEventMask::SIGNAL,
        ] {
            let msg = proceventmask_msg(mask);
            let dec = decode_proceventmask(&msg).unwrap();
            assert_eq!(dec, mask);
        }
        // 未知位被 truncate 保留（与 C 同行为：mask 是任意 unsigned）
        let mask = ProcEventMask::from_bits_truncate(0xFF);
        let msg = proceventmask_msg(mask);
        let dec = decode_proceventmask(&msg).unwrap();
        assert_eq!(dec.bits(), 0x03); // 仅 EXIT|SIGNAL 被保留（truncate）
    }
}

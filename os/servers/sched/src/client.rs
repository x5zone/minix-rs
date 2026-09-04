//! SCHED's mirror of the client contract: who asks, what rides, who rules.
//!
//! Mirrors the PM-facing half of the contract (`minix3/minix/lib/libsys/
//! sched_start.c:11-97`, `sched_stop.c:9-29`): the three scheduler shapes,
//! the START/INHERIT payloads, and the reply readback. 13-pm-interaction.md.
//!
//! The module owns the routing and the packing, nothing else: which shape
//! takes which road, what each letter carries, and whose name the reply
//! names. Sending the letters, asserting the wire values, and holding
//! `mp_scheduler` stay caller-side — PM holds its table (sched.rs) and
//! the loop holds IPC (02); this module only mirrors their answers early
//! (06 `admit()`'s and 12 `validate_flags`' precedent: same answer up
//! front, decided once, in one home).
//!
//! Single-threaded event loop: pure functions, no shared state.

use minix_types::{
    Endpoint, MessLsysSchedSchedulingStart, MessLsysSchedSchedulingStop,
    MessSchedLsysSchedulingStart,
};

/// The scheduler named in a client call (`sched_start.c:58-76`).
///
/// Three shapes ride the `scheduler_e` argument: none (no scheduler),
/// the kernel itself, or a user-space server. The shapes decide the road
/// before any letter is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerSel {
    /// No scheduler: nothing to do. C: `scheduler_e == NONE`.
    None,
    /// The kernel schedules: params go direct. C: `== KERNEL`.
    Kernel,
    /// A user-space server schedules: a letter goes out. C: otherwise.
    Server(Endpoint),
}

/// Where a START call goes (`sched_start.c:58-96`).
///
/// Short circuits are variants, not early returns: the caller matches
/// all three roads exhaustively — no road silently skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartRoute {
    /// No scheduler: done, no letter. C: `58-59`.
    Done,
    /// The kernel takes it, with numbers. C: `70-76`.
    Kernel {
        /// Whose scheduler changes. C: `schedulee_e`.
        schedulee: Endpoint,
        /// Queue ceiling. C: `maxprio`.
        maxprio: i32,
        /// Share in ms. C: `quantum`.
        quantum: i32,
        /// Home CPU. C: `cpu`.
        cpu: i32,
    },
    /// A server takes it, by letter. C: `80-96`.
    Message(StartPack),
}

/// Where a STOP call goes (`sched_stop.c:9-29`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRoute {
    /// Kernel or none: done, no letter. C: `16-17`.
    Done,
    /// A server drops it, by letter. C: `23-29`.
    Message(StopPack),
}

/// Route a START call (`sched_start.c:58-76`).
///
/// `NONE` ends it, `KERNEL` carries numbers direct (`sys_schedctl`,
/// 12's `SchedctlCall::to_kernel` shape), a server earns the letter —
/// packed with the decided parent (PM knows it, 13 §2.4).
pub const fn route_start(
    sel: SchedulerSel,
    schedulee: Endpoint,
    parent: Endpoint,
    maxprio: i32,
    quantum: i32,
    cpu: i32,
) -> StartRoute {
    match sel {
        SchedulerSel::None => StartRoute::Done,
        SchedulerSel::Kernel => StartRoute::Kernel {
            schedulee,
            maxprio,
            quantum,
            cpu,
        },
        SchedulerSel::Server(_) => StartRoute::Message(StartPack {
            schedulee,
            parent,
            maxprio,
            quantum,
        }),
    }
}

/// Route a STOP call (`sched_stop.c:16-17`).
///
/// Kernel-scheduled and scheduler-less both end it (`Done`); only a
/// server earns the letter — and the letter carries the schedulee alone.
pub const fn route_stop(sel: SchedulerSel, schedulee: Endpoint) -> StopRoute {
    match sel {
        SchedulerSel::None | SchedulerSel::Kernel => StopRoute::Done,
        SchedulerSel::Server(_) => StopRoute::Message(StopPack { schedulee }),
    }
}

/// A START letter: birth carries its share (`sched_start.c:81-84`).
///
/// Four loads ride: who, whose child, how high, how much. The parent
/// arrives decided (PM knows it, 13 §2.4); the pack only carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartPack {
    /// Who is scheduled. C: `endpoint` — sched_start.c:81.
    pub schedulee: Endpoint,
    /// Whose child. C: `parent` — sched_start.c:82.
    pub parent: Endpoint,
    /// Queue ceiling. C: `maxprio` — sched_start.c:83.
    pub maxprio: i32,
    /// Share in ms. C: `quantum` — sched_start.c:84.
    pub quantum: i32,
}

impl StartPack {
    /// Pack the wire order (`message.rs:1054-1065`).
    ///
    /// Field order is the contract: endpoint, parent, maxprio, quantum.
    pub const fn wire(&self) -> MessLsysSchedSchedulingStart {
        MessLsysSchedSchedulingStart {
            endpoint: self.schedulee.0,
            parent: self.parent.0,
            maxprio: self.maxprio,
            quantum: self.quantum,
            _padding: [0; 40],
        }
    }
}

/// An INHERIT letter: continuation carries no share (`sched_start.c:25-27`).
///
/// Three loads ride; the quantum lane stays zero (`memset`, `24`) — the
/// child opens with the parent's share (06's INHERIT branch reads the
/// parent slot, never the letter), so zero reads the same as "no share".
/// The twin pack makes "an inherit with a quantum" inexpressible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InheritPack {
    /// Who is scheduled. C: `endpoint` — sched_start.c:25.
    pub schedulee: Endpoint,
    /// Whose share continues. C: `parent` — sched_start.c:26.
    pub parent: Endpoint,
    /// Queue ceiling. C: `maxprio` — sched_start.c:27.
    pub maxprio: i32,
}

impl InheritPack {
    /// Pack the same wire with a zeroed share lane (`message.rs:1054-1065`).
    ///
    /// Same struct, same order; the quantum lane is the `memset` zero —
    /// the letter says whose child, never how much.
    pub const fn wire(&self) -> MessLsysSchedSchedulingStart {
        MessLsysSchedSchedulingStart {
            endpoint: self.schedulee.0,
            parent: self.parent.0,
            maxprio: self.maxprio,
            quantum: 0,
            _padding: [0; 40],
        }
    }
}

/// A STOP letter: dropping carries the schedulee alone (`sched_stop.c:23-24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopPack {
    /// Who is dropped. C: `endpoint` — sched_stop.c:24.
    pub schedulee: Endpoint,
}

impl StopPack {
    /// Pack the wire order (`message.rs:1093-1098`).
    pub const fn wire(&self) -> MessLsysSchedSchedulingStop {
        MessLsysSchedSchedulingStop {
            endpoint: self.schedulee.0,
            _padding: [0; 52],
        }
    }
}

/// Read the reply's scheduler (`sched_start.c:39,96`).
///
/// The reply names who rules — which may differ from who was asked (the
/// forwarding note, `33-38`/`90-95`): a scheduler may hand the letter on
/// before answering. The caller takes the reply's word, never its own
/// guess; writing it back to `mp_scheduler` stays PM-side.
pub const fn read_scheduler(reply: &MessSchedLsysSchedulingStart) -> Endpoint {
    Endpoint(reply.scheduler)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(value: i32) -> Endpoint {
        Endpoint(value)
    }

    #[test]
    fn test_start_routes() {
        // NONE ends it with no letter (`58-59`).
        assert_eq!(
            route_start(SchedulerSel::None, ep(7), ep(3), 8, 200, 1),
            StartRoute::Done
        );
        // KERNEL carries numbers direct (`70-76`): the 12 `to_kernel` shape.
        assert_eq!(
            route_start(SchedulerSel::Kernel, ep(7), ep(3), 8, 200, 1),
            StartRoute::Kernel {
                schedulee: ep(7),
                maxprio: 8,
                quantum: 200,
                cpu: 1,
            }
        );
        // A server earns the letter (`80-96`): four loads ride.
        match route_start(SchedulerSel::Server(ep(4)), ep(7), ep(3), 8, 200, 1) {
            StartRoute::Message(pack) => {
                assert_eq!(
                    (pack.schedulee, pack.parent, pack.maxprio, pack.quantum),
                    (ep(7), ep(3), 8, 200)
                );
            }
            _ => panic!("server START must earn the letter"),
        }
    }

    #[test]
    fn test_stop_routes() {
        // Kernel-scheduled and scheduler-less both end it (`16-17`).
        assert_eq!(route_stop(SchedulerSel::Kernel, ep(7)), StopRoute::Done);
        assert_eq!(route_stop(SchedulerSel::None, ep(7)), StopRoute::Done);
        // A server earns the letter, carrying the schedulee alone (`23-25`).
        match route_stop(SchedulerSel::Server(ep(4)), ep(7)) {
            StopRoute::Message(pack) => assert_eq!(pack.schedulee, ep(7)),
            _ => panic!("server STOP must earn the letter"),
        }
    }

    #[test]
    fn test_inherit_pack() {
        // Three loads ride, the share lane stays zero (`25-27` + `24`).
        let wire = InheritPack {
            schedulee: ep(7),
            parent: ep(3),
            maxprio: 8,
        }
        .wire();
        assert_eq!(
            (wire.endpoint, wire.parent, wire.maxprio, wire.quantum),
            (7, 3, 8, 0)
        );
    }

    #[test]
    fn test_start_pack() {
        // Four loads ride in wire order (`81-84` + `message.rs:1054`).
        let wire = StartPack {
            schedulee: ep(7),
            parent: ep(3),
            maxprio: 8,
            quantum: 200,
        }
        .wire();
        assert_eq!(
            (wire.endpoint, wire.parent, wire.maxprio, wire.quantum),
            (7, 3, 8, 200)
        );
    }

    #[test]
    fn test_reply_forwarding() {
        // The reply may name another scheduler than asked (`33-39`):
        // the caller takes the reply's word, never its own guess.
        let reply = MessSchedLsysSchedulingStart {
            scheduler: 9,
            _padding: [0u8; 52],
        };
        assert_eq!(read_scheduler(&reply), ep(9));
    }
}

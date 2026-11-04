//! E5(g) — four-chain MIB/sysctl integration (host side, over the wire contract).
//!
//! The four chains registered as host-side acceptance coverage follow the E5(e)
//! pm_sched.rs precedent: both sides run real code, and only the end that cannot be
//! driven on a host is replaced by the wire contract.
//!
//! # Chain face
//!
//! 1. **Caller**: the real `minix_sys::{sysctl_via, sysctlbyname_via}` clients — the
//!    `MIB_SYSCTL` message assembly (the 64-bit lane overlay), the sendrec round trip,
//!    and the return length-lane read (including NetBSD's quirk that the length is
//!    written back even when the server refuses, `__sysctl.c:29-38`). The transport is
//!    the local `CallerSide` (implementing minix-sys's `IpcTransport`): `sendrec` puts
//!    the request on the shared bus, pumps one MIB main-loop round on the spot, then
//!    takes the reply (the blocking-sequence equivalent of
//!    minix3/minix/lib/libsys/taskcall.c:16).
//! 2. **MIB service**: `minix_mib::server::Server::run_once`, the real loop consuming
//!    the bus — SEF receive, triage, the five `sysctl_arm` gates, the walker tree
//!    lookup and `copy_out` all run on the production path. Kernel verbs are carried by
//!    `TestKernel`: `sys_datacopy` is a same-address-space move on the host (the
//!    pointers already point into buffers inside the caller process — the E5 harness
//!    precedent), while `sys_hz`/`getticks`/`boottime` answer canned values
//!    (`DEFAULT_HZ`=100, the kernel harness nominal at os/kernel/src/clock.rs:685); peer
//!    verbs (DS label, VM statistics, remote relay) honestly answer `-EIO` — the chains
//!    in this file do not depend on them.
//! 3. **Kernel/DS**: not part of the dependency (E5(e)'s feature-unification trap
//!    reconciliation stands unchanged).
//!
//! # Coverage (four chains)
//!
//! - **Chain 1 · MIB_SYSCTL round trip**: full clockrate read (OK + the 20-byte
//!   `struct clockinfo` + the length lane); short buffer → `ENOMEM` with a partial copy
//!   + **the full length is still written back** (C main.c:341-356); `oldp=NULL` size
//!   probe → OK + length.
//! - **Chain 1 · name walk through the tree**: `sysctlbyname_via("kern.clockrate")`
//!   resolves through the real tree's level-by-level `CTL_QUERY` enumeration (client
//!   name walk × real server-side tree).
//! - **Chain 2 · rmibtest registration contract** (server half): a SENDREC-shaped
//!   registration is refused (C remote.c:210-211 is a one-way protocol); an empty path
//!   is dropped silently (:894-897). The client's three local failure classes
//!   (empty/too long/not a node) and the **authorized relay half** of the eight remote
//!   refusals ride on T2 (grant transfer cannot be hosted) — see the registration line at
//!   the end of the file.
//! - **Chain 4 · minix.mib.* statistics reconciliation**: nodes/objects/remotes are read
//!   out of the real tree; a fresh service has remotes=0, nodes>0, objects≥nodes, and a
//!   name walk does not change the counts.
//! - **Chain 3 · ERESTART pure verdicts**: `check_reply`'s three branches (C remote.c
//!   :455-459 + :359/:361/:461/:463), `locate_slot`'s three verdicts (Fresh/
//!   Reuse/ReapThenReuse — reaping a dead slot is the first half of mib_down, C remote.c
//!   :64-68), and `dereg_slot`'s removal (around C :285).
//!
//! # Registered (outside this file's scope)
//!
//! The authorized-copy half of the remote relay (the `grant_magic` path) and the
//! client-side local failure face of libsys `rmib_register` belong to the E-RMIBWIRE
//! transport remainder, whose real-hardware half rides on T2 — this file pins their
//! decision logic through the pure verdicts.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use minix_mib::io::relay::RelayDir;
use minix_mib::server::{MibIpc, MibServer, Server};
use minix_mib::transport::{MibKernel, MibServices};
use minix_mib::{io, remote};
use minix_sef::SefIpc;
use minix_sys::ipc::{AsyncSlot, CALL_SEND, CALL_SENDREC, IpcStatus, IpcTransport, TrapStatus};
use minix_sys::misc::MIB_CALL_SYSCTL;
use minix_sys::sysctl::{SysctlOld, sysctl_via};
use minix_types::{
    CTL_KERN, CTL_MINIX, Endpoint, GrantId, KERN_CLOCKRATE, MIB_NODES, MIB_OBJECTS, MIB_REMOTES,
    MINIX_MIB, MessLcMibSysctl, MessLsysMibRegister, Message,
};

const CALLER: Endpoint = Endpoint(20);
const HZ: u32 = 100;
/// `struct clockinfo` size (five `int`s, sys/sys/sysctl.h:206-212; the
/// walker's clockrate reply size, kern.rs:561 `CTLTYPE_STRUCT | RO` = 20).
const CLOCKINFO_LEN: usize = 20;

// ---------------------------------------------------------------------------
// Shared bus: the caller's sendrec and MIB's receive/sendnb meet here
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BusInner {
    /// Caller → MIB arrival queue, carrying the arrival status word with the
    /// message (MIB triage reads it to tell sendrec from one-way send).
    to_mib: RefCell<VecDeque<(Message, u32)>>,
    /// MIB → caller reply queue (`send_nb`'s landing point).
    to_caller: RefCell<VecDeque<Message>>,
}

#[derive(Clone, Default)]
struct Bus(Rc<BusInner>);

impl Bus {
    fn deliver(&self, message: Message, status: u32) {
        self.0.to_mib.borrow_mut().push_back((message, status));
    }

    fn take_reply(&self) -> Option<Message> {
        self.0.to_caller.borrow_mut().pop_front()
    }

    fn replies_pending(&self) -> usize {
        self.0.to_caller.borrow().len()
    }
}

/// MIB server IPC seam ([`MibIpc`] + SEF receive): takes arrivals from the bus
/// and puts replies on the bus.
struct MibSide {
    bus: Bus,
}

impl MibIpc for MibSide {
    fn receive_status(&mut self, message: &mut Message) -> Result<u32, i32> {
        match self.bus.0.to_mib.borrow_mut().pop_front() {
            Some((arriving, status)) => {
                *message = arriving;
                Ok(status)
            }
            None => Err(minix_types::ENOMSG),
        }
    }

    fn send_nb(&mut self, _to: Endpoint, message: &Message) -> Result<(), i32> {
        self.bus.0.to_caller.borrow_mut().push_back(*message);
        Ok(())
    }

    fn send_rec(&mut self, _peer: Endpoint, _message: &mut Message) -> Result<(), i32> {
        Err(minix_types::EIO) // remote relay leg: not reached in this file (T2)
    }

    fn notify(&mut self, _to: Endpoint) -> Result<(), i32> {
        Ok(())
    }
}

impl SefIpc for MibSide {
    fn receive(&mut self, _src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        match self.bus.0.to_mib.borrow_mut().pop_front() {
            // The status word is the only source for triage and for the
            // is_sendrec decision; it must be delivered as is (a real kernel
            // hands it over in receive's third argument).
            Some((arriving, status)) => {
                *msg = arriving;
                Ok(status as i32)
            }
            None => Err(minix_types::ENOMSG),
        }
    }

    fn notify(&mut self, _dest: Endpoint) -> Result<(), i32> {
        Ok(())
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        // The birth reply rides the bus's reply lane like any other send.
        self.bus.0.from_mib.borrow_mut().push_back((dest, *msg));
        Ok(0)
    }
}

/// Kernel verbs: same-address-space move (host replay of `sys_datacopy`) + canned
/// time base.
struct TestKernel;

impl MibKernel for TestKernel {
    fn datacopy_from(&mut self, _src: Endpoint, src_addr: u64, buf: &mut [u8]) -> Result<(), i32> {
        // SAFETY: host harness — src_addr is the address of a buffer inside the
        // caller process (caller and service share a process); the same-address-space
        // replay of the E5 harness precedent.
        let src = unsafe { core::slice::from_raw_parts(src_addr as *const u8, buf.len()) };
        buf.copy_from_slice(src);
        Ok(())
    }

    fn datacopy_to(&mut self, _dest: Endpoint, dest_addr: u64, buf: &[u8]) -> Result<(), i32> {
        // SAFETY: as above — dest_addr points into a buffer inside the caller process.
        let dest = unsafe { core::slice::from_raw_parts_mut(dest_addr as *mut u8, buf.len()) };
        dest.copy_from_slice(buf);
        Ok(())
    }

    fn grant_magic(
        &mut self,
        _whom: Endpoint,
        _addr: u64,
        _len: u64,
        _dir: RelayDir,
    ) -> Result<GrantId, i32> {
        Err(minix_types::EIO) // authorized relay: T2
    }

    fn grant_revoke(&mut self, _grant: GrantId) {}

    fn getproctab(&mut self, _buf: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn getticks(&mut self) -> Result<u64, i32> {
        Ok(1_000_000)
    }

    fn hz(&mut self) -> Result<u32, i32> {
        Ok(HZ)
    }

    fn boottime(&mut self) -> Result<u64, i32> {
        Ok(1_700_000_000)
    }
}

/// Peer verbs: all honestly `-EIO` (the chains in this file do not depend on DS/VM/PM
/// peers).
struct TestServices;

impl MibServices for TestServices {
    fn getnuid(&mut self, _who: Endpoint) -> Result<u32, i32> {
        Ok(0) // root: PRIVATE nodes are readable (this file never touches PRIVATE)
    }

    fn getsysinfo(&mut self, _target: Endpoint, _what: i32, _buf: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn ds_retrieve_label_name(&mut self, _who: Endpoint, buf: &mut [u8]) -> Result<usize, i32> {
        let label = b"rmibtest";
        buf[..label.len()].copy_from_slice(label);
        Ok(label.len())
    }

    fn remote_info(&mut self, _peer: Endpoint, _n: &mut [u8], _d: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn vm_info(&mut self, _what: i32, _ep: Endpoint, _buf: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn remote_call(
        &mut self,
        _peer: Endpoint,
        _call: io::relay::RemoteCall,
        _reply: &mut io::relay::RemoteReplyWire,
    ) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn pm_getparam(&mut self, _param: i32, _buf: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }
}

// ---------------------------------------------------------------------------
// Assembly: an `Rc<RefCell<Server>>` shared handle — after the caller transport
// delivers it pumps one main-loop round, and the test body can inspect server state
// (slots/counters) through the same handle at any time.
// ---------------------------------------------------------------------------

type TestServer = Server<TestKernel, TestServices, MibSide>;

/// Caller-side transport (minix-sys's [`IpcTransport`]).
#[derive(Clone)]
struct CallerSide {
    bus: Bus,
    server: Rc<RefCell<TestServer>>,
}

impl CallerSide {
    fn pair() -> (Self, Rc<RefCell<TestServer>>, Bus) {
        let bus = Bus::default();
        let server = Rc::new(RefCell::new(Server::new(
            MibServer::new(),
            MibSide { bus: bus.clone() },
            TestKernel,
            TestServices,
        )));
        let caller = Self {
            bus: bus.clone(),
            server: server.clone(),
        };
        (caller, server, bus)
    }

    /// Pump one round and take the reply (sendrec's return leg).
    fn round_trip(&self, message: &mut Message) -> Option<Message> {
        message.m_source = CALLER;
        self.bus.deliver(*message, CALL_SENDREC);
        let _ = self.server.borrow_mut().run_once();
        self.bus.take_reply()
    }

    /// One-way send (registration-face protocol): deliver + pump one round; the reply
    /// stays on the bus for assertions.
    fn fire_and_forget(&self, message: &Message) {
        let mut arriving = *message;
        arriving.m_source = CALLER;
        self.bus.deliver(arriving, CALL_SEND);
        let _ = self.server.borrow_mut().run_once();
    }
}

impl IpcTransport for CallerSide {
    fn sendrec(&self, _destination: Endpoint, message: &mut Message) -> Result<(), TrapStatus> {
        match self.round_trip(message) {
            Some(reply) => {
                *message = reply;
                Ok(())
            }
            None => Err(TrapStatus(minix_types::EIO)),
        }
    }

    fn send(&self, _destination: Endpoint, message: &Message) -> Result<(), TrapStatus> {
        self.fire_and_forget(message);
        Ok(())
    }

    fn receive(&self, _s: Endpoint, _m: &mut Message) -> Result<IpcStatus, TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn notify(&self, _d: Endpoint) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn sendnb(&self, _d: Endpoint, _m: &Message) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn senda(&self, _t: &[AsyncSlot]) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn query_kerninfo_page(&self) -> Result<u64, TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
}

/// Read one `minix.mib.*` counter through the real tree (chain 4's observation face).
fn read_stat(caller: &CallerSide, leaf: i32) -> (i64, usize) {
    let mut buffer = [0u8; 16];
    let mut length = buffer.len();
    let name = [CTL_MINIX, MINIX_MIB, leaf];
    sysctl_via(
        caller,
        &name,
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    )
    .expect("minix.mib.* 读数");
    let value = match length {
        4 => i32::from_le_bytes(buffer[0..4].try_into().unwrap()) as i64,
        8 => i64::from_le_bytes(buffer[0..8].try_into().unwrap()),
        other => panic!("计数节点尺寸 {other} 不在 int/intptr 形"),
    };
    (value, length)
}

// ---------------------------------------------------------------------------
// Chain 1 · MIB_SYSCTL round trip
// ---------------------------------------------------------------------------

/// Full clockrate read: real decode → real tree lookup → real copy-out → OK + the
/// complete Clockinfo + length lane 20.
#[test]
fn sysctl_round_trip_returns_full_clockinfo() {
    let (caller, _server, _bus) = CallerSide::pair();
    let mut buffer = [0u8; 32];
    let mut length = buffer.len();
    sysctl_via(
        &caller,
        &[CTL_KERN, KERN_CLOCKRATE],
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    )
    .expect("clockrate 整读");
    assert_eq!(length, CLOCKINFO_LEN, "回程长度车道 = 节点全尺寸");
    // Clockinfo's five ints: hz/tick/tickadj/stathz/profhz — tick=1e6/hz.
    let i32_at = |off: usize| i32::from_le_bytes(buffer[off..off + 4].try_into().unwrap());
    assert_eq!(i32_at(0), HZ as i32, "hz");
    assert_eq!(i32_at(4), 1_000_000 / HZ as i32, "tick = 1e6/hz");
    assert_eq!(i32_at(8), 1_000_000 / HZ as i32, "tickadj = tick");
    assert_eq!(i32_at(12), HZ as i32, "stathz");
    assert_eq!(i32_at(16), HZ as i32, "profhz");
}

/// Short buffer: the server copies out the prefix that fits and answers ENOMEM, but
/// **the full length is still written back** (C main.c:341-356; sysctl_via's
/// `__sysctl.c:29-38` quirk of "write the length back even when the server refuses"
/// holds between the two real ends).
#[test]
fn enomem_partial_copy_still_writes_full_length() {
    let (caller, _server, _bus) = CallerSide::pair();
    let mut buffer = [0u8; 8];
    let mut length = buffer.len();
    let outcome = sysctl_via(
        &caller,
        &[CTL_KERN, KERN_CLOCKRATE],
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    );
    assert_eq!(
        outcome,
        Err(minix_types::Errno::ENOMEM),
        "短缓冲应答 ENOMEM"
    );
    assert_eq!(length, CLOCKINFO_LEN, "失败路径长度车道仍回写全尺寸");
    // Partial copy: the first 8 bytes that fit are real data (hz + the first half of
    // tick).
    assert_eq!(
        i32::from_le_bytes(buffer[0..4].try_into().unwrap()),
        HZ as i32,
        "前缀 hz 已拷出"
    );
}

/// Size probe with `oldp=NULL`: OK (not ENOMEM) and the length lane carries the full
/// size — the first pass of sysctl(3)'s two-step read.
#[test]
fn length_probe_without_oldp_answers_size() {
    let (caller, _server, _bus) = CallerSide::pair();
    let mut message = Message::zeroed();
    {
        // SAFETY: the request overlay is written along the documented lanes (64-bit
        // lane).
        let wire: &mut MessLcMibSysctl = unsafe { &mut message.m_u.m_lc_mib_sysctl };
        wire.oldp = 0;
        wire.oldlen = 64;
        wire.namelen = 2;
        wire.name[..2].copy_from_slice(&[CTL_KERN, KERN_CLOCKRATE]);
    }
    message.m_type = MIB_CALL_SYSCTL;
    let reply = caller.round_trip(&mut message).expect("探测有回执");
    assert_eq!(reply.m_type, minix_types::OK, "尺寸探测应答 OK");
    // SAFETY: the reply overlay is read for the return length lane.
    let replied = unsafe { &reply.m_u.m_mib_lc_sysctl };
    assert_eq!(replied.oldlen as usize, CLOCKINFO_LEN, "长度车道 = 全尺寸");
}

/// `sysctlbyname_via` resolves a name through the real tree's level-by-level
/// `CTL_QUERY` enumeration — a real closed loop of client name walk × server-side
/// query enumeration.
#[test]
fn sysctlbyname_walks_the_real_tree() {
    let (caller, _server, _bus) = CallerSide::pair();
    let mut buffer = [0u8; CLOCKINFO_LEN];
    let mut length = buffer.len();
    let outcome = minix_sys::sysctl::sysctlbyname_via(
        &caller,
        b"kern.clockrate",
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    );
    assert!(outcome.is_ok(), "名字走树解析 kern.clockrate");
    assert_eq!(length, CLOCKINFO_LEN);
    assert_eq!(
        i32::from_le_bytes(buffer[0..4].try_into().unwrap()),
        HZ as i32,
        "走树读得的 hz 与直接 mib 一致"
    );
}

// ---------------------------------------------------------------------------
// Chain 2 · rmibtest registration contract (server half)
// ---------------------------------------------------------------------------

/// A SENDREC-shaped registration is refused: the registration protocol is one-way (C
/// remote.c:210-211, avoiding a MIB→service call cycle) — the refusal has a reply (a
/// blocking caller must not be left hanging), and the slot table stays untouched.
#[test]
fn register_via_sendrec_is_refused() {
    let (caller, server, bus) = CallerSide::pair();
    let mut message = Message::zeroed();
    {
        // SAFETY: the registration overlay is written along the documented lanes.
        let wire: &mut MessLsysMibRegister = unsafe { &mut message.m_u.m_lsys_mib_register };
        wire.miblen = 2;
        wire.mib[..2].copy_from_slice(&[CTL_MINIX, 60]);
    }
    message.m_type = minix_types::MIB_REGISTER;
    let reply = caller.round_trip(&mut message).expect("阻塞注册有回执");
    assert_ne!(reply.m_type, minix_types::OK, "SENDREC 注册被拒");
    assert!(
        server
            .borrow()
            .server
            .slots
            .iter()
            .all(|s| s.endpt.is_none()),
        "被拒注册不留占用槽位"
    );
    let _ = bus.replies_pending();
}

/// Empty-path one-way registration: dropped silently (C :894-897 "nothing to mount") —
/// no reply lands, the slot table stays untouched.
#[test]
fn register_empty_path_is_silently_dropped() {
    let (caller, server, bus) = CallerSide::pair();
    let mut message = Message::zeroed();
    {
        // SAFETY: the registration overlay is written along the documented lanes.
        let wire: &mut MessLsysMibRegister = unsafe { &mut message.m_u.m_lsys_mib_register };
        wire.miblen = 0;
    }
    message.m_type = minix_types::MIB_REGISTER;
    caller.fire_and_forget(&message);
    assert_eq!(bus.replies_pending(), 0, "单向空路径零回执");
    assert!(
        server
            .borrow()
            .server
            .slots
            .iter()
            .all(|s| s.endpt.is_none()),
        "空路径不留占用槽位"
    );
}

// ---------------------------------------------------------------------------
// Chain 4 · minix.mib.* statistics reconciliation
// ---------------------------------------------------------------------------

/// The three counters of the statistics subtree are read out of the real tree and
/// reconciled against the server's mount state: a fresh service has remotes=0;
/// nodes>0, objects≥nodes (objects include nodes); a name walk (enumeration) changes
/// no counter.
#[test]
fn minix_mib_stats_reconcile_with_tree_and_slots() {
    let (caller, server, _bus) = CallerSide::pair();
    let (nodes, _) = read_stat(&caller, MIB_NODES);
    let (objects, _) = read_stat(&caller, MIB_OBJECTS);
    let (remotes, _) = read_stat(&caller, MIB_REMOTES);
    assert!(nodes > 0, "静态树节点计数为正");
    assert_eq!(
        objects, 0,
        "fresh 服务零堆分配（静态树不占对象账，C tree.c:1524-1525）"
    );
    assert_eq!(remotes, 0, "fresh 服务无挂载远端");
    assert!(
        server
            .borrow()
            .server
            .slots
            .iter()
            .all(|s| s.endpt.is_none()),
        "remotes 计数与槽表一致（无占用槽）"
    );
    // Walk idempotence: one complete by-name enumeration resolution changes no counter.
    let mut buffer = [0u8; CLOCKINFO_LEN];
    let mut length = buffer.len();
    minix_sys::sysctl::sysctlbyname_via(
        &caller,
        b"kern.clockrate",
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    )
    .expect("走查");
    let (nodes_after, _) = read_stat(&caller, MIB_NODES);
    let (objects_after, _) = read_stat(&caller, MIB_OBJECTS);
    let (remotes_after, _) = read_stat(&caller, MIB_REMOTES);
    assert_eq!(
        (nodes, objects, remotes),
        (nodes_after, objects_after, remotes_after)
    );
}

// ---------------------------------------------------------------------------
// Chain 3 · ERESTART pure verdicts (the decision logic in remote.rs)
// ---------------------------------------------------------------------------

/// The three branches of reply checking: correct delivery, wrong type, wrong request id
/// (C remote.c:455-459 + :359/:361/:461/:463) — the core of the relay call's "is the
/// service still alive" decision.
#[test]
fn check_reply_verdicts_match_c() {
    use remote::ReplyCheck;
    assert_eq!(remote::check_reply(true, 0, 0), ReplyCheck::Deliver(0));
    assert_eq!(remote::check_reply(true, 0, -5), ReplyCheck::Deliver(-5));
    assert_eq!(remote::check_reply(false, 0, 0), ReplyCheck::WrongType);
    assert_eq!(remote::check_reply(true, 7, 0), ReplyCheck::WrongId);
}

/// Slot lifecycle: Fresh creates an entry, Reuse for the same endpoint and label,
/// ReapThenReuse for a dead slot with the same label (reaping is the first half of
/// mib_down, C remote.c:64-68), and removal by endpoint.
#[test]
fn slot_lifecycle_verdicts_cover_shadowing_and_death() {
    use remote::{EndptSlot, Label, SlotVerdict, locate_slot};
    let label = |name: &[u8]| Label::from_bytes(name).unwrap();
    let slot = |endpt: Option<i32>, name: &[u8]| EndptSlot {
        endpt,
        roots: Vec::new(),
        label: label(name),
    };
    // A new service with a free seat → Fresh (the table pre-allocates seats; Fresh =
    // a free seat was found, C remote.c:136-147).
    let slots = vec![slot(Some(20), b"lwip"), slot(None, b"")];
    assert!(matches!(
        locate_slot(&slots, 21, label(b"netstack")),
        SlotVerdict::Fresh { eid: 1 }
    ));
    // Same endpoint → Reuse, label irrelevant (C :126-127 endpoint wins).
    assert!(matches!(
        locate_slot(&slots, 20, label(b"lwip")),
        SlotVerdict::Reuse { eid: 0 }
    ));
    assert!(matches!(
        locate_slot(&slots, 20, label(b"renamed")),
        SlotVerdict::Reuse { eid: 0 }
    ));
    // Occupied slot + same label + different endpoint → ReapThenReuse (service died →
    // reap → reuse the seat; the table-side shape of mib_down, C :128-136).
    let slots = vec![slot(Some(20), b"lwip"), slot(None, b"")];
    assert!(matches!(
        locate_slot(&slots, 21, label(b"lwip")),
        SlotVerdict::ReapThenReuse { eid: 0 }
    ));
    // All occupied and no same label → Full (C :140-148).
    let full = vec![slot(Some(20), b"lwip")];
    assert!(matches!(
        locate_slot(&full, 21, label(b"netstack")),
        SlotVerdict::Full
    ));
    // Removal by endpoint: a dead service leaves the table.
    let mut slots = vec![slot(Some(20), b"lwip"), slot(Some(21), b"uds")];
    assert_eq!(remote::dereg_slot(&slots, 20), Some(0));
    assert_eq!(remote::dereg_slot(&slots, 99), None);
    let _ = &mut slots;
}

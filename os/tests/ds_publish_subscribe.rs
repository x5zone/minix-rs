//! E5(f) hosted half — the three DS chains (publish, subscribe, fetch) wired to
//! VFS's event classification.
//!
//! When a driver process comes up it first publishes a u32 value under
//! `drv.<class>.<n>` to DS (`DS_DRIVER_UP`); the subscriber (VFS) claims the class
//! prefix it cares about with one pattern string, and from then on every publish
//! lets DS's scan decide whom to wake. This chain's hosted half runs end to end
//! because the DS server hangs its event loop on two small traits: `DsIpc`
//! (receive/send/wake) and `DsKernel` (grant copies).
//!
//! # Chain surface (all real code; only the IPC and grant transports are scripted)
//!
//! 1. **Label face**: the real `minix_ds::boot::apply_boot_map` lays down the
//!    RS/VFS/MFS labels (the fresh anchor of `store.c:273-279`), after which
//!    `resolve_name` can recognize the caller — both publish and subscribe
//!    require the caller to have a name (`store.c:297-299` / `:514-516`).
//! 2. **Subscribe face**: `DsServer::run_once` receives `DS_SUBSCRIBE` →
//!    `plan_subscribe`'s pattern-compile check (`pattern::EreMatcher`, the C
//!    counterpart of `regcomp`) → `apply_subscribe` takes the seat.
//! 3. **Publish face**: `DS_PUBLISH` → `plan_publish`'s six-step verdict → the
//!    commit half writes the entry → `ring` calls `notify::apply_update` to scan
//!    the subscription table; hit seats are marked and `ipc_notify`'d.
//! 4. **Fetch face**: `DS_CHECK` → `plan_check` takes the lowest set bit →
//!    `safecopy_to` writes the event key back to the caller → the reply message's
//!    `flags`/`owner` fields carry the type and the publisher endpoint →
//!    `apply_check` consumes that bit.
//! 5. **Peer seam**: the fetched key and value are exactly the input of VFS's
//!    `ds_event` drain loop (`classify_ds_key` and `ds_event_action` in
//!    `os/servers/vfs/src/misc.rs:782-836`); the test asserts the two mesh.
//!
//! # Division of labor with DS's own unit tests
//!
//! The in-crate DS tests cover the `plan_*`/`apply_*` pure functions and each
//! arm's rejection path one by one; this file does not repeat them, it only
//! strings **one full publish → subscribe → fetch** through the real server and
//! asserts where the event key lands on the VFS side. The on-target three chains
//! (the real publishes as RS starts each service) hang off T2 / E5(f).

use std::cell::RefCell;
use std::collections::VecDeque;

use minix_ds::boot::{BootService, RS_LABEL_LEN, apply_boot_map};
use minix_ds::server::{DsIpc, DsKernel, DsServer, Step};
use minix_types::{
    DS_CHECK, DS_DRIVER_UP, DS_MAX_KEYLEN, DS_PUBLISH, DS_SUBSCRIBE, DsFlags, DsVal, Endpoint,
    ENOENT, GrantId, Message, MessDsReq, OK,
};
use minix_vfs::misc::{DsDriverKind, DsUpTarget, classify_ds_key, ds_event_action};

// ---------------------------------------------------------------------------
// Fixtures: scripted IPC and grant transports
// ---------------------------------------------------------------------------

/// Scripted IPC: received messages dequeue in order; sent replies and wake-ups
/// are booked separately.
///
/// `DsIpc::send`/`notify` take `&self` (C's fire-and-forget, `main.c:123-131` /
/// `store.c:222`), so the bookkeeping goes through `RefCell`.
#[derive(Default)]
struct ScriptedIpc {
    inbox: RefCell<VecDeque<Message>>,
    sent: RefCell<Vec<(Endpoint, Message)>>,
    notified: RefCell<Vec<Endpoint>>,
}

impl ScriptedIpc {
    fn new(messages: Vec<Message>) -> Self {
        Self {
            inbox: RefCell::new(messages.into()),
            ..Self::default()
        }
    }
    fn notified(&self) -> Vec<Endpoint> {
        self.notified.borrow().clone()
    }
    /// The last reply (DS writes the answer into the same message buffer,
    /// `main.c:80-86`).
    fn last_reply(&self) -> (Endpoint, Message) {
        self.sent
            .borrow()
            .last()
            .copied()
            .expect("每次请求都应有一条回复")
    }
}

impl DsIpc for ScriptedIpc {
    fn receive(&mut self) -> Result<Message, i32> {
        self.inbox.borrow_mut().pop_front().ok_or(-1)
    }
    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
        self.sent.borrow_mut().push((to, *message));
        Ok(())
    }
    fn send_rec(&self, to: Endpoint, message: &mut Message) -> Result<(), i32> {
        self.sent.borrow_mut().push((to, *message));
        Ok(())
    }
    fn notify(&self, who: Endpoint) -> Result<(), i32> {
        self.notified.borrow_mut().push(who);
        Ok(())
    }
}

/// Scripted grant transport: key bytes are supplied in order, written-back bytes
/// are booked in order.
#[derive(Default)]
struct ScriptedKernel {
    keys: RefCell<VecDeque<Vec<u8>>>,
    written_back: RefCell<Vec<Vec<u8>>>,
}

impl ScriptedKernel {
    fn with_keys(keys: Vec<Vec<u8>>) -> Self {
        Self {
            keys: RefCell::new(keys.into()),
            ..Self::default()
        }
    }
    fn written_back(&self) -> Vec<Vec<u8>> {
        self.written_back.borrow().clone()
    }
}

impl DsKernel for ScriptedKernel {
    fn safecopy_from(
        &mut self,
        _caller: Endpoint,
        _grant: GrantId,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        let key = self.keys.borrow_mut().pop_front().ok_or(-1)?;
        assert_eq!(key.len(), buf.len(), "脚本 key 长度须与消息 key_len 一致");
        buf.copy_from_slice(&key);
        Ok(())
    }
    fn safecopy_to(&mut self, _caller: Endpoint, _grant: GrantId, buf: &[u8]) -> Result<(), i32> {
        self.written_back.borrow_mut().push(buf.to_vec());
        Ok(())
    }
    fn data_copy_to(
        &mut self,
        _caller: Endpoint,
        _where: u64,
        _buf: &[u8],
    ) -> Result<(), i32> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fixtures: messages and labels
// ---------------------------------------------------------------------------

/// One DS request (the `mess_ds_req` payload, C ipc.h:107).
fn ds_request(
    call: i32,
    from: Endpoint,
    key_len: i32,
    flags: DsFlags,
    val_in: DsVal,
) -> Message {
    let mut msg = Message {
        m_source: from,
        m_type: call,
        ..Message::default()
    };
    msg.m_u.m_ds_req = MessDsReq {
        key_grant: 1,
        key_len,
        flags: flags.bits() as i32,
        val_in,
        val_len: 0,
        owner: 0,
        _padding: [0; 32],
    };
    msg
}

/// A service label (`rprocpub.label[16]`, rs.h:177).
fn label(text: &str) -> [u8; RS_LABEL_LEN] {
    let mut lane = [0u8; RS_LABEL_LEN];
    lane[..text.len()].copy_from_slice(text.as_bytes());
    lane
}

/// The NUL parking lane of an event key (DS's key lane is 80 bytes including
/// the terminator).
fn key_lane(text: &str) -> Vec<u8> {
    let mut lane = vec![0u8; text.len()];
    lane.copy_from_slice(text.as_bytes());
    lane
}

/// An empty server with three service labels laid down (RS is the label
/// registrant, `store.c:242`).
fn server_with_labels() -> DsServer {
    let mut server = DsServer::new();
    let services = [
        BootService {
            in_use: true,
            endpoint: Endpoint::RS,
            label: label("rs"),
        },
        BootService {
            in_use: true,
            endpoint: Endpoint::VFS,
            label: label("vfs"),
        },
        BootService {
            in_use: true,
            endpoint: Endpoint::MFS,
            label: label("mfs"),
        },
    ];
    apply_boot_map(&mut server.store, &mut server.subs, &services).expect("三个标签都落座");
    server
}

/// The pattern the subscriber cares about: block device driver events (C adds
/// the `^…$` anchors automatically on subscribe, `store.c:487-493`; the Rust
/// engine expresses the same semantics by whole-string matching).
const BLK_PATTERN: &str = r"drv\.blk\..*";

// ---------------------------------------------------------------------------
// E5(f).1 All three chains: subscribe → publish → wake → fetch
// ---------------------------------------------------------------------------

#[test]
fn ds_publish_subscribe_check_chain_wakes_subscriber_and_vfs_classifies() {
    let mut server = server_with_labels();

    // ── Step one: VFS subscribes to block device events (`DS_SUBSCRIBE`, store.c:487-508) ──
    let subscribe = ds_request(
        DS_SUBSCRIBE,
        Endpoint::VFS,
        BLK_PATTERN.len() as i32,
        DsFlags::TYPE_U32,
        DsVal::number(0),
    );
    let mut ipc = ScriptedIpc::new(vec![subscribe]);
    let mut kernel = ScriptedKernel::with_keys(vec![key_lane(BLK_PATTERN)]);
    assert_eq!(server.run_once(&mut ipc, &mut kernel), Step::Handled);
    assert_eq!(
        ipc.last_reply().1.m_type,
        OK,
        "订阅成功（回复码写在 m_type，main.c:80-86）"
    );
    assert!(ipc.notified().is_empty(), "订阅本身不唤醒任何人");

    // ── Step two: MFS publishes `drv.blk.0` = DS_DRIVER_UP (store.c:297-378) ──
    let publish = ds_request(
        DS_PUBLISH,
        Endpoint::MFS,
        "drv.blk.0".len() as i32,
        DsFlags::TYPE_U32,
        DsVal::number(DS_DRIVER_UP as u32),
    );
    let mut ipc = ScriptedIpc::new(vec![publish]);
    let mut kernel = ScriptedKernel::with_keys(vec![key_lane("drv.blk.0")]);
    assert_eq!(server.run_once(&mut ipc, &mut kernel), Step::Handled);
    assert_eq!(ipc.last_reply().1.m_type, OK, "发布成功");
    assert_eq!(
        ipc.notified(),
        vec![Endpoint::VFS],
        "扫描命中 VFS 的订阅，唤醒它一次（store.c:222 的 ipc_notify）"
    );

    // Entry seated: key and value are exactly the pair the driver reported.
    let (key_text, published_value) = {
        let entry_slot =
            minix_ds::slots::lookup_entry(&server.store, b"drv.blk.0", DsFlags::TYPE_U32)
                .expect("发布过的 key 可查");
        let entry = entry_slot.get(&server.store).expect("座位有体");
        let end = entry.key.iter().position(|&b| b == 0).unwrap_or(0);
        let key_text = String::from_utf8(entry.key[..end].to_vec()).expect("key 是文本");
        // SAFETY: this publish goes through the TYPE_U32 arm, so the narrow arm
        // is the active one (store.rs:43-48).
        (key_text, unsafe { entry.body.u32 })
    };
    assert_eq!(key_text, "drv.blk.0");
    assert_eq!(published_value, DS_DRIVER_UP as u32, "值是 DS_DRIVER_UP");

    // ── Step three: VFS fetches the event (`DS_CHECK`, store.c:544-578) ──
    let check = ds_request(
        DS_CHECK,
        Endpoint::VFS,
        DS_MAX_KEYLEN as i32,
        DsFlags::empty(),
        DsVal::grant(1),
    );
    let mut ipc = ScriptedIpc::new(vec![check]);
    let mut kernel = ScriptedKernel::default();
    assert_eq!(server.run_once(&mut ipc, &mut kernel), Step::Handled);
    let (reply_to, reply) = ipc.last_reply();
    assert_eq!(reply_to, Endpoint::VFS, "应答回到取回者");
    assert_eq!(reply.m_type, OK);
    assert_eq!(
        kernel.written_back(),
        vec![key_lane("drv.blk.0").into_iter().chain([0u8]).collect::<Vec<_>>()],
        "key 经 grant 写回调用者（含终止符，store.c:561-563）"
    );
    let req = unsafe { reply.m_u.m_ds_req };
    assert_eq!(
        DsFlags::from_bits_truncate(req.flags as u32),
        DsFlags::TYPE_U32,
        "回复的 flags 域带条目类型（store.c:571）"
    );
    assert_eq!(req.owner, Endpoint::MFS.0, "回复的 owner 域带发布者端点（store.c:570）");

    // Fetch consumes: the same bit is never fetched twice (`apply_check`, store.c:575).
    let check_again = ds_request(
        DS_CHECK,
        Endpoint::VFS,
        DS_MAX_KEYLEN as i32,
        DsFlags::empty(),
        DsVal::grant(1),
    );
    let mut ipc = ScriptedIpc::new(vec![check_again]);
    assert_eq!(server.run_once(&mut ipc, &mut kernel), Step::Handled);
    assert_eq!(
        ipc.last_reply().1.m_type,
        -ENOENT,
        "没有未读更新时取回失败（CheckReject::NoUpdate，store.c:556）；回复通道按 C 的 \
         `_SYSTEM`（ds/inc.h:6）把 errno 带负号发出，本树在 `reply_status` 折转"
    );

    // ── Step four: the peer seam — VFS uses this key/value pair for the driver-up decision ──
    // VFS's `ds_event` drain loop is exactly the consumer of this channel: it first
    // classifies by key prefix, then decides from the fetched value whether this is a
    // driver-up event.
    assert_eq!(
        classify_ds_key(&key_text),
        Some(DsDriverKind::Blk),
        "`drv.blk.` 前缀归块设备类（misc.c:958-968）"
    );
    assert_eq!(
        ds_event_action(DsDriverKind::Blk, published_value),
        Some(DsUpTarget::Dmap { is_blk: true }),
        "DS_DRIVER_UP 触发 dmap 上线（misc.c:976-982）"
    );
    // Negative control: switch the class prefix and the same decision gives the
    // other branch; a value that is not an up event produces no action.
    assert_eq!(
        classify_ds_key("drv.chr.4"),
        Some(DsDriverKind::Chr),
        "`drv.chr.` 归字符设备类"
    );
    assert_eq!(
        ds_event_action(DsDriverKind::Blk, DS_DRIVER_UP as u32 + 1),
        None,
        "非 DS_DRIVER_UP 的值不触发上线"
    );
}

// ---------------------------------------------------------------------------
// E5(f).2 A publish outside the pattern wakes no subscriber
// ---------------------------------------------------------------------------

#[test]
fn ds_publish_outside_subscribed_pattern_does_not_wake_subscriber() {
    let mut server = server_with_labels();

    // Subscribe to the block device class.
    let subscribe = ds_request(
        DS_SUBSCRIBE,
        Endpoint::VFS,
        BLK_PATTERN.len() as i32,
        DsFlags::TYPE_U32,
        DsVal::number(0),
    );
    let mut ipc = ScriptedIpc::new(vec![subscribe]);
    let mut kernel = ScriptedKernel::with_keys(vec![key_lane(BLK_PATTERN)]);
    assert_eq!(server.run_once(&mut ipc, &mut kernel), Step::Handled);
    assert_eq!(ipc.last_reply().1.m_type, OK);

    // Publish a character device event: same type arm, but the pattern does not
    // match.
    let publish = ds_request(
        DS_PUBLISH,
        Endpoint::MFS,
        "drv.chr.4".len() as i32,
        DsFlags::TYPE_U32,
        DsVal::number(DS_DRIVER_UP as u32),
    );
    let mut ipc = ScriptedIpc::new(vec![publish]);
    let mut kernel = ScriptedKernel::with_keys(vec![key_lane("drv.chr.4")]);
    assert_eq!(server.run_once(&mut ipc, &mut kernel), Step::Handled);
    assert_eq!(ipc.last_reply().1.m_type, OK, "发布本身成功");
    assert!(
        ipc.notified().is_empty(),
        "模式不匹配的发布不唤醒订阅者（entry_matches 的第三道门，store.c:190-193）"
    );

    // The subscriber also has no unread update.
    let check = ds_request(
        DS_CHECK,
        Endpoint::VFS,
        DS_MAX_KEYLEN as i32,
        DsFlags::empty(),
        DsVal::grant(1),
    );
    let mut ipc = ScriptedIpc::new(vec![check]);
    assert_eq!(server.run_once(&mut ipc, &mut kernel), Step::Handled);
    assert_eq!(ipc.last_reply().1.m_type, -ENOENT);
}

//! E5(f) 宿主半 — DS 发布/订阅/取回三链，接到 VFS 的事件分类上。
//!
//! 驱动进程起来时先向 DS 发布一个 `drv.<类>.<n>` 的 u32 值（`DS_DRIVER_UP`），
//! 订阅方（VFS）用一条模式串订下自己关心的那类前缀，之后每次发布都由 DS 的
//! 扫描决定唤醒谁。这条链的宿主半能整段跑起来，因为 DS 服务器把事件循环开在了
//! 两个小 trait 上：`DsIpc`（收/发/唤醒）与 `DsKernel`（grant 拷贝）。
//!
//! # 链路面（全部真代码，只有 IPC 与 grant 传输是脚本）
//!
//! 1. **标签面**：`minix_ds::boot::apply_boot_map` 真函数铺 RS/VFS/MFS 的标签
//!    （`store.c:273-279` 的 fresh 锚），之后 `resolve_name` 才认得出调用者——
//!    发布与订阅都要求调用者有名字（`store.c:297-299` / `:514-516`）。
//! 2. **订阅面**：`DsServer::run_once` 收 `DS_SUBSCRIBE` → `plan_subscribe` 的
//!    模式编译检查（`pattern::EreMatcher`，C 的 `regcomp` 对应物）→ `apply_subscribe`
//!    落座。
//! 3. **发布面**：`DS_PUBLISH` → `plan_publish` 的六步裁决 → 提交半写条目 →
//!    `ring` 调 `notify::apply_update` 扫描订阅表，命中的座位置位并 `ipc_notify`。
//! 4. **取回面**：`DS_CHECK` → `plan_check` 取最低置位 → `safecopy_to` 把事件
//!    key 写回调用者 → 回复消息的 `flags`/`owner` 域带类型与发布者端点 →
//!    `apply_check` 消费掉这一位。
//! 5. **对端接缝**：取回的 key 与值正是 VFS `ds_event` 排空循环的输入
//!    （`os/servers/vfs/src/misc.rs:782-836` 的 `classify_ds_key` 与
//!    `ds_event_action`），断言两者咬合。
//!
//! # 与 DS 自有单测的分工
//!
//! DS crate 内测试逐个覆盖 `plan_*`/`apply_*` 纯函数与各 arm 的拒绝路；本文件
//! 不重复它们，只把 **一次完整的发布→订阅→取回** 串起来跑真服务器，并断言
//! 事件 key 在 VFS 侧的归宿。真机三链（RS 启动各服务时的真实发布）挂 T2 / E5(f)。

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
// 夹具：脚本化 IPC 与 grant 传输
// ---------------------------------------------------------------------------

/// 脚本化 IPC：收到的消息按序出队，发出的回复与唤醒分别记账。
///
/// `DsIpc::send`/`notify` 收 `&self`（C 的一发不等，`main.c:123-131` /
/// `store.c:222`），所以记账走 `RefCell`。
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
    /// 最后一条回复（DS 把应答写在同一个消息缓冲里，`main.c:80-86`）。
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
    fn notify(&self, who: Endpoint) -> Result<(), i32> {
        self.notified.borrow_mut().push(who);
        Ok(())
    }
}

/// 脚本化 grant 传输：key 的字节按序供给，写回的字节按序记账。
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
// 夹具：消息与标签
// ---------------------------------------------------------------------------

/// 一条 DS 请求（`mess_ds_req` 载荷，C ipc.h:107）。
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

/// 服务标签（`rprocpub.label[16]`，rs.h:177）。
fn label(text: &str) -> [u8; RS_LABEL_LEN] {
    let mut lane = [0u8; RS_LABEL_LEN];
    lane[..text.len()].copy_from_slice(text.as_bytes());
    lane
}

/// 事件 key 的 NUL 停车道（DS 的 key 车道是 80 字节含终止符）。
fn key_lane(text: &str) -> Vec<u8> {
    let mut lane = vec![0u8; text.len()];
    lane.copy_from_slice(text.as_bytes());
    lane
}

/// 铺好三个服务标签的空服务器（RS 是标签的登记方，`store.c:242`）。
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

/// 订阅方关心的模式：块设备驱动事件（C 侧订阅时自动加 `^…$` 锚，
/// `store.c:487-493`；Rust 的引擎按整串匹配表达同一语义）。
const BLK_PATTERN: &str = r"drv\.blk\..*";

// ---------------------------------------------------------------------------
// E5(f).1 三链全通：订阅 → 发布 → 唤醒 → 取回
// ---------------------------------------------------------------------------

#[test]
fn ds_publish_subscribe_check_chain_wakes_subscriber_and_vfs_classifies() {
    let mut server = server_with_labels();

    // ── 第一步：VFS 订阅块设备事件（`DS_SUBSCRIBE`，store.c:487-508）──
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

    // ── 第二步：MFS 发布 `drv.blk.0` = DS_DRIVER_UP（store.c:297-378）──
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

    // 条目落座：key 与值就是驱动上报的那一对。
    let (key_text, published_value) = {
        let entry_slot =
            minix_ds::slots::lookup_entry(&server.store, b"drv.blk.0", DsFlags::TYPE_U32)
                .expect("发布过的 key 可查");
        let entry = entry_slot.get(&server.store).expect("座位有体");
        let end = entry.key.iter().position(|&b| b == 0).unwrap_or(0);
        let key_text = String::from_utf8(entry.key[..end].to_vec()).expect("key 是文本");
        // SAFETY: 本次发布走的是 TYPE_U32 臂，narrow arm 即活跃臂（store.rs:43-48）。
        (key_text, unsafe { entry.body.u32 })
    };
    assert_eq!(key_text, "drv.blk.0");
    assert_eq!(published_value, DS_DRIVER_UP as u32, "值是 DS_DRIVER_UP");

    // ── 第三步：VFS 取回事件（`DS_CHECK`，store.c:544-578）──
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

    // 取回即消费：同一位不会被取两次（`apply_check`，store.c:575）。
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
        ENOENT,
        "没有未读更新时取回失败（CheckReject::NoUpdate，store.c:556）"
    );

    // ── 第四步：对端接缝——VFS 拿这对 key/值做驱动上线决策 ──
    // VFS 的 `ds_event` 排空循环正是这条通道的消费方：先按 key 前缀分类，
    // 再按取回的值判断是否为驱动上线事件。
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
    // 反面对照：换个类前缀，同一条决策给出另一支；值不是上线事件则无动作。
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
// E5(f).2 模式不匹配的发布不唤醒订阅者
// ---------------------------------------------------------------------------

#[test]
fn ds_publish_outside_subscribed_pattern_does_not_wake_subscriber() {
    let mut server = server_with_labels();

    // 订阅块设备类。
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

    // 发布一个字符设备事件：类型臂相同，但模式串不匹配。
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

    // 订阅者名下也没有未读更新。
    let check = ds_request(
        DS_CHECK,
        Endpoint::VFS,
        DS_MAX_KEYLEN as i32,
        DsFlags::empty(),
        DsVal::grant(1),
    );
    let mut ipc = ScriptedIpc::new(vec![check]);
    assert_eq!(server.run_once(&mut ipc, &mut kernel), Step::Handled);
    assert_eq!(ipc.last_reply().1.m_type, ENOENT);
}

//! VFS↔MFS 跨包桥 v0 —— LOOKUP 单段探针(四层真代码)。
//!
//! # 链路
//!
//! 用户侧 `minix_sys::vfs::open_via`(C libc open 的 Rust 对位)发出 open
//! 请求 → `VfsState::run_once` 走真主循环:路径解析、worker 绑定、grant
//! 登记(路径 grant 指向**真实宿主地址**)→ `pending_fs` 挂起 LOOKUP 请求
//! → 桥(`task::run` + 本文件 [`Bridge`])用 fs-rt 的 `decode_body` 解码、
//! `task::dispatch` 驱动**真 MfsServer 真盘**、`encode_reply` 编回 →
//! `handle_fs_reply` 落地 → open 本地完成,应答用户。
//!
//! # 定案(桥的两项设计)
//!
//! 1. **grant 宿主模型 = 真地址直读**:VFS 的 `GrantTable` 登记本就持有
//!    真实宿主地址(worker scratch、用户缓冲),桥经 `GrantTable::probe`
//!    (内核视角读取,`minix-sys`)取窗后按裸指针直读直写——与 VFS 自身
//!    宿主模型及 mib_sysctl 裸指针回放判例一致。
//! 2. **请求映射 = FS_BASE 偏移**:VFS `REQ_*` 与 fs-rt `RequestNumber`
//!    同源(C `vfsif.h` 索引),`m_type - FS_BASE` 即表索引。
//!
//! # 覆盖边界
//!
//! 本探针覆盖 LOOKUP 一段;open 的数据面(读写)、非路径请求(ReadSuper
//! 等)随后续段铺开。用户 sendrec 的回程在探针里以 EIO 失败收场——真实
//! 应答落在 `VfsState` 的回复队列,断言以它为准。

use minix_fs::driver::{FsDriver, Server as TaskServer};
use minix_fs::protocol::{MountFlags, RequestNumber};
use minix_fs::task::{self, Envelope, FsTransport as TaskTransport, Incoming};
use minix_fs_mfs::server::MfsServer;
use minix_fs_rt::ipc::RtIpc;
use minix_fs_rt::source::ImgrdBlockSource;
use minix_fs_rt::wire::{decode_body, encode_reply};
use minix_sys::grant::GrantTable;
use minix_sys::ipc::CannedTransport;
use minix_sys::vfs::open_via;
use minix_types::{Endpoint, Message};
use minix_vfs::fs_comm::VfsTransIdCodec;
use minix_vfs::main_loop::VfsState;
use minix_vfs::vnode::VnodeId;

const DEVICE: u64 = 0x301;
const BLOCK_SIZE: usize = 4096;
const ROOT: u64 = 1;

const MFS_EP: Endpoint = Endpoint::MFS;
const USER_EP: Endpoint = Endpoint::from_generation_slot(2, 0); // slot 0 = 种子进程槽

// ---------------------------------------------------------------------------
// 桥的 grant 通道:内核视角取窗,真地址直读直写
// ---------------------------------------------------------------------------

struct BridgeIpc<'a> {
    grants: &'a GrantTable,
}

impl RtIpc for BridgeIpc<'_> {
    fn receive(
        &mut self,
        _src: Endpoint,
        _msg: &mut Message,
    ) -> Result<minix_fs_rt::ipc::Receipt, i32> {
        Err(minix_types::EIO) // 桥不走 SEF 接收半(请求由 VFS 侧喂入)
    }
    fn send(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), i32> {
        Ok(())
    }
    fn copy_from(
        &mut self,
        _granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        let probe = self.grants.probe(grant).ok_or(minix_types::EACCES)?;
        let (start, len) = probe.granter_window;
        if offset
            .checked_add(buf.len() as u64)
            .ok_or(minix_types::EINVAL)?
            > len
        {
            return Err(minix_types::EINVAL); // 越出授权窗:C 内核同判
        }
        // SAFETY: 窗口地址由 VFS 的 grant 登记持有(真实宿主内存),
        // 边界经上方检查对齐授权长度。
        unsafe {
            buf.copy_from_slice(core::slice::from_raw_parts(
                (start + offset) as *const u8,
                buf.len(),
            ));
        }
        Ok(())
    }
    fn copy_to(
        &mut self,
        _granter: Endpoint,
        grant: i32,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), i32> {
        let probe = self.grants.probe(grant).ok_or(minix_types::EACCES)?;
        if !probe.writable {
            return Err(minix_types::EACCES);
        }
        let (start, len) = probe.granter_window;
        if offset
            .checked_add(bytes.len() as u64)
            .ok_or(minix_types::EINVAL)?
            > len
        {
            return Err(minix_types::EINVAL);
        }
        // SAFETY: 同 copy_from,写半。
        unsafe {
            core::slice::from_raw_parts_mut((start + offset) as *mut u8, bytes.len())
                .copy_from_slice(bytes);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 桥:VFS pending_fs ↔ task::run 的双向缝
// ---------------------------------------------------------------------------

struct Bridge<'a> {
    state: &'a mut VfsState,
    blocking: &'a mut minix_vfs::fs_comm::BlockingTransport,
    rounds: u32,
    last_rn: Option<RequestNumber>,
}

impl TaskTransport for Bridge<'_> {
    fn receive(&mut self) -> Incoming {
        // 先捕获臂登记的请求( flush 会消费 pending ),再走发送半。
        let Some((raw_req, worker)) = self
            .state
            .pending_fs
            .as_ref()
            .map(|p| (p.req.clone(), p.worker))
        else {
            return Incoming::Cancelled; // 挂起清空:syscall 已走完
        };
        self.state.flush_pending_fs(self.blocking);
        // 复现生产发送半的 transid 盖章(fs_sendrec 经 TransId::add 把
        // worker slot 编入 m_type 高 16 位,com.h:909-911)——线上形态。
        let mut wire = raw_req;
        wire.m_type = minix_vfs::fs_comm::TransId::add(wire.m_type as u32, worker as usize) as i32;
        let envelope = Envelope {
            source: MFS_EP.0,
            is_notification: false,
            message_type: wire.m_type,
        };
        let (call, _) = minix_fs::protocol::TransactionId::decode(wire.m_type);
        let Some(rn) = RequestNumber::try_from((call - minix_types::FS_BASE) as u32).ok() else {
            return Incoming::Unserved(envelope);
        };
        let mut ipc = BridgeIpc {
            grants: &self.state.grants,
        };
        match decode_body(rn, &wire, &mut ipc, USER_EP) {
            Ok(body) => {
                self.rounds += 1;
                self.last_rn = Some(rn);
                Incoming::Request(envelope, body)
            }
            Err(e) => panic!("桥解码失败({rn:?}): errno {}", e.to_i32()),
        }
    }

    fn reply(&mut self, _to: i32, reply: minix_fs::task::FsReply) {
        let rn = self.last_rn.expect("reply 前必有请求");
        let mut msg = encode_reply(rn, &reply);
        // 内核在送达时为应答盖章来源(VFS 按来源找 vmnt,C main.c:191)。
        msg.m_source = MFS_EP;
        self.state
            .handle_fs_reply(&msg, &VfsTransIdCodec)
            .expect("VFS 接受桥回灌的 FS 应答");
        // 续接推进:lookup 回复驱动 walk/open 收尾,可能登记下一个请求。
        self.state.run_worker_continuations();
    }

    fn copy_in(&mut self, _offset: usize, _out: &mut [u8]) -> Result<(), minix_types::Errno> {
        Err(minix_types::Errno::from_i32(minix_types::ENOSYS)) // LOOKUP 无数据面
    }
    fn copy_out(&mut self, _offset: usize, _bytes: &[u8]) {}
}

// ---------------------------------------------------------------------------
// 夹具:真盘 MFS + 种子好的 VfsState
// ---------------------------------------------------------------------------

/// 真盘 MFS:mkfs 产镜像 → ImgrdBlockSource → mount → 根下建 "hello"。
fn mfs_with_hello() -> (MfsServer<ImgrdBlockSource>, u64) {
    use minix_fs::protocol::CapabilityFlags;
    use minix_fs_mfs::mkfs::{build_image, plan_layout};

    let plan = plan_layout(64, Some(64), BLOCK_SIZE as u64).expect("布局合法");
    let image = build_image(&plan, 1_000).expect("mkfs 成功");
    let source = ImgrdBlockSource::new(image, BLOCK_SIZE).expect("imgrd 源合法");
    let mut server = MfsServer::with_pool(source, 8, || 0);
    let mut capabilities = CapabilityFlags::EMPTY;
    let root = server
        .mount(DEVICE, MountFlags::EMPTY, &mut capabilities)
        .expect("挂载成功");
    let hello = server
        .create(ROOT, "hello", 0o100644, 0, 0)
        .expect("建 hello");
    server.write(hello.inode_number, 0, b"hi").expect("写入");
    (server, hello.inode_number)
}

/// 种子 VfsState:vmnt 一行、根 vnode、用户进程槽、grant 热身。
fn seeded_state() -> VfsState {
    let mut state = VfsState::new();
    state.initialized = true;
    {
        let fp = state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .expect("slot 0");
        fp.pid = 200;
        fp.endpoint = USER_EP;
        fp.root_dir = Some(0);
        fp.work_dir = Some(0);
    }
    {
        let v = state.vnode_table.get_mut(VnodeId(0)).expect("vnode slot 0");
        v.fs = MFS_EP;
        v.ino = ROOT;
        v.mode = 0o040755;
        v.ref_count = 2;
        v.size = 2 * 64;
        v.dev = DEVICE;
    }
    {
        let m = state.vmnt_table.alloc().expect("vmnt 表有空位");
        let row = state.vmnt_table.get_mut(m).expect("vmnt 存在");
        row.fs = MFS_EP;
        row.dev = DEVICE;
    }
    // grant 热身:宿主构建下表增长后的首次登记失败(freelist 已铺好),
    // 第二次起成功——与 crate 内 seed_ready_state 的判例一致。
    loop {
        match state.grants.grant_direct(
            &minix_sys::syscall::DirectKernelCallTransport,
            MFS_EP.0,
            0,
            1,
            minix_types::CpFlags::READ,
        ) {
            Ok(g) => {
                state.grants.revoke(g);
                break;
            }
            Err(_) => continue,
        }
    }
    state
}

// ---------------------------------------------------------------------------
// 探针
// ---------------------------------------------------------------------------

#[test]
fn lookup_bridge_resolves_open() {
    let (mfs, hello_ino) = mfs_with_hello();
    let mut state = seeded_state();

    // 用户侧真请求:minix_sys open 包装发出,路径 grant 指向真实缓冲。
    let mut path = b"hello\0".to_vec();
    let mut canned = CannedTransport::new();
    canned.reply_sendrec(Err(minix_sys::ipc::TrapStatus(minix_types::EIO)));
    let _ = open_via(&canned, path.as_ptr() as u64, path.len(), 0, 0);
    let (_src, mut req) = canned.sent.borrow()[0].clone();
    // 内核在送达时盖章 m_source;探针以 USER_EP 充当内核章。
    req.m_source = USER_EP;

    // VFS 真主循环:open 挂起在 LOOKUP。
    eprintln!("诊断: grants_before={}", state.grants.len());
    eprintln!(
        "诊断: find_by_fs(MFS)={:?}",
        state.vmnt_table.find_by_fs(MFS_EP).is_some()
    );
    let route = state.run_once(&req, &VfsTransIdCodec);
    eprintln!(
        "诊断: route={route:?} grants_after={} pending={}",
        state.grants.len(),
        state.pending_fs.is_some()
    );
    if state.pending_fs.is_none() {
        if let Some((t, r)) = state.take_reply() {
            panic!(
                "open 提前结束:target={t:?} m_type={} errno_lane={}",
                r.m_type,
                unsafe { i32::from_le_bytes(r.m_u.raw[0..4].try_into().unwrap()) }
            );
        }
        panic!("open 既未挂起也无应答(route={route:?})");
    }

    // 桥驱动真 MfsServer:外层循环"发送半→task::run→续接",直到 open 完成。
    let mut task_server = TaskServer::new(mfs);
    task_server.did_mount(
        DEVICE,
        minix_fs::protocol::FileNode {
            inode_number: ROOT,
            mode: 0o040755,
            size: 2 * 64,
            owner: 0,
            group: 0,
            device: DEVICE,
        },
    );
    let mut blocking = minix_vfs::fs_comm::BlockingTransport;
    let mut rounds = 0u32;
    for _ in 0..8 {
        if state.pending_fs.is_none() {
            break;
        }
        let mut bridge = Bridge {
            state: &mut state,
            blocking: &mut blocking,
            rounds: 0,
            last_rn: None,
        };
        task::run(&mut task_server, &mut bridge);
        rounds += bridge.rounds;
    }
    assert!(rounds >= 1, "桥至少服务一轮 LOOKUP");
    assert!(state.pending_fs.is_none(), "桥循环后不得残留未服务请求");

    // open 本地完成:应答用户 OK + fd;hello 的 vnode 已入表。
    let (target, reply) = state.take_reply().expect("open 应有用户应答");
    assert_eq!(target, USER_EP);
    assert_eq!(reply.m_type, minix_types::OK, "open 成功");
    // SAFETY(test): fd 在回复载荷首字(mess_vfs_lc_open 的 fd 车道)。
    let fd = unsafe { i32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()) };
    assert!(fd >= 0, "回复载荷带非负 fd,得 {fd}");

    let found = (0..state.vnode_table.len())
        .map(|i| state.vnode_table.get(minix_vfs::vnode::VnodeId(i)).unwrap())
        .find(|v| v.fs == MFS_EP && v.ino == hello_ino);
    assert!(found.is_some(), "hello 的 vnode 已被 lookup 落表");
}

//! 生产传输：devman 作为"文件系统外形的服务器"接在 VFS 的 fsdriver
//! 协议上，同时在同一个循环里收 DEVMAN 消息（C `fsdriver_task` +
//! `fdr_other` 的单循环形态，DM-P1-2）。
//!
//! # 分层
//!
//! - [`KernelIpc`]：内核面（收/发/往返/通知/grant 两向拷贝）——生产实现
//!   是 [`SysKernel`]（`minix-sys` 的 `DirectTrapTransport` +
//!   `sys_safecopyfrom/to`），测试注入脚本替身。**这是本模块能测的全部
//!   原因**：wire 编解码、分类、回复成型都不碰内核。
//! - [`MinixTransport`]：`Transport` 的生产实现——`next()` 收包并分类，
//!   `reply()/send()/sendrec()` 按 fsdriver 协议成型后发出。
//!
//! # 协议要点（逐条对 C 锚点）
//!
//! - **transid**：VFS 的请求 `m_type = TRNS_ADD_ID(call_nr, transid)`，
//!   回复 `m_type = TRNS_ADD_ID(result, transid)`（`fsdriver.c:34-50`）；
//!   算式用 `minix_types` 的 `trns_*`（vfsif.h:79-81 的唯一权威）。
//! - **入口分流**：`is_ipc_notify(status) || source != VFS` → `fdr_other`
//!   且**不回信**（`fsdriver.c:26-31`）；devman 的 `fdr_other` 就是
//!   DEVMAN 消息面（`DevmanMsg::classify`）。
//! - **mount 门**：未挂载时除 `REQ_READSUPER` 外一律 `EINVAL`
//!   （`fsdriver.c:40-46`）；请求号经 `RequestNumber`（含 `GetNode` 空槽
//!   → `ENOSYS`）。
//! - **数据面是 grant**：`REQ_READ`/`REQ_GETDENTS` 的 `grant` 指向**调用方**
//!   缓冲，数据由 FS 侧 `sys_safecopyto` 写出（`libfsdriver` 的
//!   `fsdriver_copyout`）；`REQ_LOOKUP` 的名字由 FS 侧
//!   `sys_safecopyfrom` 读入（C 的 `fsdriver_getname`，`call.c:36-41`）。

use minix_types::{
    is_fs_rq, lookup_req_off, readsuper_req_off, transfer_req_off, trns_add_id, trns_del_id,
    trns_get_id, Endpoint, Message, ENOSYS, EINVAL, REQ_GETDENTS, REQ_LOOKUP, REQ_READ,
    REQ_READSUPER, REQ_UNMOUNT,
};

use crate::ipc::DevmanMsg;
use crate::vtreefs::{Incoming, Reply, Request, Transport};

/// 内核面：收包/发送/往返/通知/grant 两向拷贝。
///
/// 生产实现 [`SysKernel`]；测试用脚本替身（本模块的测试与
/// `server.rs` 的集成测试都靠它）。
pub trait KernelIpc {
    /// 阻塞收包（`ipc_receive` 语义：返回状态字）。
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32>;
    /// 通知（SEF ping 的 pong 走这里，`sef_ping.c:61`）。
    fn notify(&mut self, dest: Endpoint) -> Result<(), i32>;
    /// 非阻塞发（fsdriver 回复用 `ipc_send`，devman 的 DEVMAN 回复用
    /// `ipc_send`，转发的发起用 `ipc_sendrec`）。
    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), i32>;
    /// 阻塞往返（C `ipc_sendrec`，bind.c:32/80）。
    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), i32>;
    /// `sys_safecopyfrom(granter, grant, off, buf)` —— 把调用方缓冲的
    /// 字节读进来（加名字/写载荷）。
    fn safecopy_from(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32>;
    /// `sys_safecopyto(granter, grant, off, buf)` —— 把结果字节写回调用方
    /// 缓冲（读数据/目录项）。
    fn safecopy_to(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32>;
}

/// 生产内核面：`minix-sys` 的 trap 传输 + 内核调用。
#[derive(Debug, Default)]
pub struct SysKernel;

impl KernelIpc for SysKernel {
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        use minix_sys::ipc::IpcTransport;
        match minix_sys::ipc::DirectTrapTransport.receive(src, msg) {
            Ok(status) => Ok(status.0 as i32),
            Err(_) => Err(minix_types::EIO),
        }
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport.notify(dest).map_err(|_| minix_types::EIO)
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport.send(dest, msg).map_err(|_| minix_types::EIO)
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .sendrec(dest, msg)
            .map_err(|_| minix_types::EIO)
    }

    fn safecopy_from(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        minix_sys::syscall::sys_safecopyfrom(
            &minix_sys::syscall::DirectKernelCallTransport,
            granter.get(),
            grant,
            offset,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    }

    fn safecopy_to(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32> {
        minix_sys::syscall::sys_safecopyto(
            &minix_sys::syscall::DirectKernelCallTransport,
            granter.get(),
            grant,
            offset,
            buf.as_ptr() as u64,
            buf.len() as u64,
        )
    }
}

/// `KernelIpc` → `minix_sef::SefIpc` 的桥（SEF 收包只需要收与通知两动词）。
struct SefBridge<'a, K: KernelIpc>(&'a mut K);

impl<K: KernelIpc> minix_sef::SefIpc for SefBridge<'_, K> {
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        self.0.receive(src, msg)
    }
    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.0.notify(dest)
    }
}

/// 生产 `Transport`：一收一分类一回复，FS 面与 DEVMAN 面同循环。
pub struct MinixTransport<K: KernelIpc> {
    kernel: K,
    /// 当前请求的 grant 与对端（`reply()` 要用：数据面写回调用方缓冲）。
    current_grant: i32,
    current_granter: Endpoint,
    /// 当前请求的 transid（回复回显）。
    current_transaction: u32,
    /// 当前请求的调用方（回复目的地；C 的 `m_ptr->m_source`）。
    current_source: Endpoint,
    /// 已挂载？（mount 门；与 `Server::process_fs` 的 `VTreeFs` 状态分开
    /// 保存——C 的 `fsdriver_mounted` 是库全局，Rust 由传输持有一份，
    /// `Server::run` 的挂载语义不变）。
    mounted: bool,
}

impl<K: KernelIpc> MinixTransport<K> {
    pub const fn new(kernel: K) -> Self {
        Self {
            kernel,
            current_grant: 0,
            current_granter: Endpoint::NONE,
            current_transaction: 0,
            current_source: Endpoint::NONE,
            mounted: false,
        }
    }

    /// 借出内核面（测试与装配用）。
    pub fn kernel_mut(&mut self) -> &mut K {
        &mut self.kernel
    }

    /// 收一条消息（SEF 语义：ping 透明、信号回调）。
    fn receive_message(&mut self, msg: &mut Message) -> Option<i32> {
        let mut bridge = SefBridge(&mut self.kernel);
        match minix_sef::sef_receive_status(&mut bridge, Endpoint::ANY, msg, &mut |_| {}) {
            Ok(recv) => Some(recv.status),
            // 收包失败＝传输损坏：C `panic`（fsdriver.c:92），Rust 由
            // 调用方（main）决定——这里返回 None 让循环退出。
            Err(_) => None,
        }
    }

    /// `REQ_LOOKUP` 的载荷解码：名字经 grant 读入（C `fsdriver_getname`）。
    ///
    /// 载荷布局（C `mess_vfs_fs_lookup` — ipc.h:2011-2035；Rust 侧按
    /// 同一字段序的 LP64 视图）：
    /// `dir_ino@0`、`root_ino@8`、`flags@16`、`path_len@24`、
    /// `path_size@32`、`ucred_size@40`、`grant_path@48`、`grant_ucred@56`。
    fn decode_lookup(&mut self, msg: &Message) -> Option<Request> {
        // SAFETY: VFS 的 REQ_LOOKUP 载荷按上述域序写在消息负载区。
        let raw = unsafe { &msg.m_u.raw };
        let dir = read_i64(raw, lookup_req_off::DIR_INO) as u64;
        let path_len = read_i64(raw, lookup_req_off::PATH_LEN) as usize;
        let grant_path = read_i64(raw, lookup_req_off::GRANT_PATH) as i32;
        // 名字长度门：0 或超上限即拒（C 的 `path_len = strlen + 1`，
        // `PATH_MAX 255` — sys/sys/syslimits.h）。
        if path_len == 0 || path_len > PATH_BUF_MAX {
            return None;
        }
        let mut name = alloc::vec![0u8; path_len];
        if self
            .kernel
            .safecopy_from(msg.m_source, grant_path, 0, &mut name)
            .is_err()
        {
            return None;
        }
        // C 的路径是 NUL 结尾（`path_len = strlen + 1`，request.c:447）。
        let end = name.iter().position(|b| *b == 0).unwrap_or(name.len());
        let name = alloc::string::String::from_utf8_lossy(&name[..end]).into_owned();
        self.current_grant = grant_path;
        self.current_granter = msg.m_source;
        Some(Request::Lookup {
            dir: crate::vtreefs::Ino(dir as u32),
            name,
        })
    }

    /// `REQ_READ`/`REQ_GETDENTS` 的公有前缀：`inode@0`、`seek_pos@8`、
    /// `grant@16`、`nbytes/mem_size@24`（C `mess_vfs_fs_readwrite` —
    /// ipc.h:2121-2131、`mess_vfs_fs_getdents` — ipc.h:1992-2002）。
    fn decode_transfer(&mut self, msg: &Message) -> (u64, i64, i32, usize) {
        // SAFETY: 同上，按字段序读。
        let raw = unsafe { &msg.m_u.raw };
        let ino = read_i64(raw, transfer_req_off::INODE) as u64;
        let pos = read_i64(raw, transfer_req_off::SEEK_POS);
        let grant = read_i64(raw, transfer_req_off::GRANT) as i32;
        let bytes = read_i64(raw, transfer_req_off::BYTES) as usize;
        self.current_grant = grant;
        self.current_granter = msg.m_source;
        (ino, pos, grant, bytes)
    }
}

/// 名字缓冲上限：C `PATH_MAX 255`（sys/sys/syslimits.h）含 NUL。
const PATH_BUF_MAX: usize = 256;

/// 从消息负载的字节区读一个 8 字节小端字（越界给 0）。
fn read_i64(raw: &[u8], at: usize) -> i64 {
    let mut b = [0u8; 8];
    if at + 8 <= raw.len() {
        b.copy_from_slice(&raw[at..at + 8]);
    }
    i64::from_le_bytes(b)
}

impl<K: KernelIpc> Transport for MinixTransport<K> {
    fn next(&mut self) -> Option<Incoming> {
        loop {
            let mut msg = Message::default();
            let status = self.receive_message(&mut msg)?;
            let notify = minix_sef::is_ipc_notify(status);
            let source = msg.m_source;

            // **出生面**（全树共同的 RS_INIT 握手，卡K/S25 收口）：RS 的
            // init 请求不进业务循环——fresh 就地应答 `RS_INIT+OK`（devman
            // 的树在 `Server::new` 构造期已建，等价 C `init_hook` 的首次
            // 构建语义），LU/RESTART 诚实拒 `ENOSYS` 并终止循环（C 对
            // init 失败 panic；单线程服务器 fail-closed 停机，RS 按崩溃
            // 处置）。语义与 `fs/fs-rt` 的 `run_birth` 同源
            // （C `sef_startup` 尾部 + `do_sef_init_request`，
            // sef_init.c:193-215）；两族各持一份是分层使然（fs-rt 依赖
            // minix-fs 的 FsDriver 面，devman 不在此族），第三处出现时再
            // 裁决上收。
            if !notify && msg.m_type == minix_types::RS_INIT && source == Endpoint::RS {
                // SAFETY: 出生请求的活跃 union 臂是 `m_rs_init`
                // （m_type == RS_INIT 且来源 RS）。
                let kind = unsafe { msg.m_u.m_rs_init.type_ };
                let result = if kind == 0 { minix_types::OK } else { minix_types::ENOSYS };
                let mut reply = Message {
                    m_type: minix_types::RS_INIT,
                    ..Message::default()
                };
                // union 字段写是 safe 的（只有读才 unsafe）；回信臂同
                // `m_rs_init`（process_init 尾部：
                // `m.m_type = RS_INIT; m.m_rs_init.result = result;`）。
                reply.m_u.m_rs_init.result = result;
                // C 经 `sef_cb_init_response` 的 `ipc_sendnb`（sef_init.c
                // 尾部）；RS 在 sendrec 里等，`send` 的阻塞语义在此等价。
                let _ = self.kernel.send(Endpoint::RS, &reply);
                if result == minix_types::OK {
                    continue; // 出生已应答，吞掉这条，服务循环继续
                }
                return None; // init 被拒：fail-closed 停机
            }

            // C `fsdriver_process:26-31`：非请求（通知或别的服务发来的消息）
            // 走 `other` 且不回信。devman 的 other 就是 DEVMAN 消息面。
        if notify || source != Endpoint::VFS {
            if notify {
                return Some(Incoming::Devman { source, msg: None });
            }
            let m4 = unsafe { &msg.m_u.m_m4 };
            let (word2, word3) = (m4.m4l2 as i32, m4.m4l3 as i32);
            // ADD 的载荷在调用方的 grant 里（05 §2.2）：先按 grant 读入。
            let mut body = alloc::vec::Vec::new();
            if crate::ipc::dispatch(msg.m_type) == crate::ipc::Handler::Add {
                let grant = crate::ipc::grant_id(&msg);
                let size = crate::ipc::grant_size(&msg) as usize;
                if size > 0 {
                    let mut buf = alloc::vec![0u8; size];
                    if self
                        .kernel
                        .safecopy_from(source, grant, 0, &mut buf)
                        .is_ok()
                    {
                        body = buf;
                    }
                }
            }
            let decoded = DevmanMsg::classify(msg.m_type, &body, word2, Endpoint(word3));
            return Some(Incoming::Devman { source, msg: decoded });
        }

        // FS 面：先剥 transid，再按请求号分类（`GetNode` 空槽 → Unserved
        // 语义由 C 的 `fsdriver_callvec` 空槽给出）。
        // 线上 m_type 的高 16 位是调用号（vfsif.h:79-81）。
        if !is_fs_rq(trns_del_id(msg.m_type)) {
            // 号不在 FS 段：C 会按 `call_nr - FS_BASE` 越界 → ENOSYS；
            // 这里交回 DEVMAN 面（`DevmanMsg::classify` 会给 None = 不回信）。
            return Some(Incoming::Devman { source, msg: None });
        }
        let call = trns_del_id(msg.m_type);
        self.current_transaction = trns_get_id(msg.m_type);
        self.current_source = source;

        // mount 门（`fsdriver.c:40-46`）：未挂载时除 ReadSuper 外一律
        // EINVAL——由 `reply()` 按 `mount_refused` 回答，这里用一个专用
        // Incoming 变体表达不了，故走 `Request::Unmount` 之外的路径：
        // 决策放在本函数，返回一个"被拒请求"由 run 直接回信。
        let request = match call {
            REQ_READSUPER => {
                // 载荷：device@0、flags@8（C `mess_vfs_fs_readsuper` —
                // ipc.h:2112-2119；`REQ_ISROOT` 位在 flags）。
                let raw = unsafe { &msg.m_u.raw };
                let flags = read_i64(raw, readsuper_req_off::FLAGS) as u32;
                let is_root = minix_fs::protocol::MountFlags(flags).is_root();
                self.mounted = true;
                Request::Mount { is_root }
            }
            REQ_UNMOUNT => {
                self.mounted = false;
                Request::Unmount
            }
            REQ_LOOKUP if self.mounted => self.decode_lookup(&msg)?,
            REQ_READ if self.mounted => {
                let (ino, pos, _g, bytes) = self.decode_transfer(&msg);
                Request::Read {
                    ino: crate::vtreefs::Ino(ino as u32),
                    len: bytes,
                    pos: pos as u64,
                }
            }
            REQ_GETDENTS if self.mounted => {
                let (ino, pos, _g, _bytes) = self.decode_transfer(&msg);
                Request::Readdir {
                    dir: crate::vtreefs::Ino(ino as u32),
                    start: pos as u64,
                }
            }
            _ => {
                // 未挂载（非 ReadSuper）或 devman 不服务的请求号：
                // C 按 EINVAL / ENOSYS 两分（`fsdriver.c:40-56`）。
                let status = if self.mounted { ENOSYS } else { EINVAL };
                let _ = self
                    .kernel
                    .send(source, &reply_msg(status, self.current_transaction));
                return Some(Incoming::Refused);
            }
        };
            return Some(Incoming::Fs(request));
        }
    }

    fn reply(&mut self, reply: Reply) {
        let (status, payload) = match &reply {
            Reply::Mounted(Ok(ino)) => (0, Some(PayloadKind::Node(*ino))),
            Reply::Mounted(Err(e)) => (e.to_i32(), None),
            Reply::Unmounted => (0, None),
            Reply::Found(Ok(ino)) => (0, Some(PayloadKind::Node(*ino))),
            Reply::Found(Err(e)) => (e.to_i32(), None),
            Reply::Data(Ok(bytes)) => (bytes.len() as i32, Some(PayloadKind::Data(bytes.clone()))),
            Reply::Data(Err(e)) => (e.to_i32(), None),
            Reply::Entries(Ok(entries)) => (entries.len() as i32, Some(PayloadKind::Dents)),
            Reply::Entries(Err(e)) => (e.to_i32(), None),
        };
        // 数据面：读/目录项经 grant 写回调用方缓冲（C `fsdriver_copyout`）。
        if let Some(PayloadKind::Data(bytes)) = &payload
            && self
                .kernel
                .safecopy_to(self.current_granter, self.current_grant, 0, bytes)
                .is_err()
        {
            // 拷出失败：回 EFAULT（C 的 `fsdriver_copyout` 失败即该 errno）。
            let _ = self.kernel.send(
                self.current_source,
                &reply_msg(minix_types::EFAULT, self.current_transaction),
            );
            return;
        }
        let mut m = reply_msg(status, self.current_transaction);
        if let Some(PayloadKind::Node(ino)) = payload {
            // `struct fsdriver_node` 的回复字段（C call.c:57-62：node
            // 号 + 属性在同一条消息里）——devman 的节点属性由 VFS 另行
            // stat，这里只回节点号（C 的 `fsdriver_node` 首字段）。
            // SAFETY: 回复负载的字节区。
            unsafe {
                m.m_u.raw[0..8].copy_from_slice(&(ino.0 as u64).to_le_bytes());
            }
        }
        let _ = self.kernel.send(self.current_source, &m);
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) {
        let _ = self.kernel.send(dest, msg);
    }

    fn sendrec(&mut self, owner: Endpoint, msg: &mut Message) -> Result<(), minix_types::Errno> {
        self.kernel
            .sendrec(owner, msg)
            .map_err(|_| minix_types::Errno::from_i32(minix_types::EIO))
    }
}

/// 回复里状态之外还要送什么（数据面/节点号）。
enum PayloadKind {
    Data(alloc::vec::Vec<u8>),
    Node(crate::vtreefs::Ino),
    Dents,
}

/// 成型一条 fsdriver 回复：`m_type = TRNS_ADD_ID(status, transid)`。
fn reply_msg(status: i32, transaction: u32) -> Message {
    Message {
        m_type: trns_add_id(status, transaction),
        ..Message::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vtreefs::{Ino, Transport as _};
    use alloc::collections::VecDeque;

    /// 脚本化内核面：收包队列 + 记录发送 + 可注入的 safecopy 失败。
    #[derive(Default)]
    struct ScriptedKernel {
        inbox: VecDeque<(Message, i32)>,
        sent: alloc::vec::Vec<(Endpoint, Message)>,
        sendrecs: alloc::vec::Vec<(Endpoint, Message)>,
        /// 下一次 `safecopy_from` 交给调用方的字节（名字等）。
        from_bytes: alloc::vec::Vec<u8>,
        /// `safecopy_to` 收到的字节（读/目录项回写）。
        to_bytes: alloc::vec::Vec<u8>,
        to_grant: Option<(Endpoint, i32, u64)>,
        fail_from: bool,
    }

    impl KernelIpc for ScriptedKernel {
        fn receive(&mut self, _src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
            match self.inbox.pop_front() {
                Some((m, s)) => {
                    *msg = m;
                    Ok(s)
                }
                None => Err(minix_types::EIO),
            }
        }
        fn notify(&mut self, _d: Endpoint) -> Result<(), i32> {
            Ok(())
        }
        fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), i32> {
            self.sent.push((dest, *msg));
            Ok(())
        }
        fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sendrecs.push((dest, *msg));
            Ok(())
        }
        fn safecopy_from(
            &mut self,
            _g: Endpoint,
            _grant: i32,
            _off: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            if self.fail_from {
                return Err(minix_types::EFAULT);
            }
            let n = buf.len().min(self.from_bytes.len());
            buf[..n].copy_from_slice(&self.from_bytes[..n]);
            Ok(())
        }
        fn safecopy_to(
            &mut self,
            g: Endpoint,
            grant: i32,
            off: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.to_grant = Some((g, grant, off));
            self.to_bytes.extend_from_slice(buf);
            Ok(())
        }
    }

    /// 造一条 VFS 发来的请求：`m_type = TRNS_ADD_ID(call, transid)`。
    fn vfs_request(call: i32, transid: u32, payload: &[(usize, i64)]) -> Message {
        let mut m = Message {
            m_source: Endpoint::VFS,
            m_type: trns_add_id(call, transid),
            ..Message::default()
        };
        // SAFETY(test): 按偏移表填载荷。
        unsafe {
            for (at, v) in payload {
                m.m_u.raw[*at..*at + 8].copy_from_slice(&v.to_le_bytes());
            }
        }
        m
    }

    fn ready(kernel: ScriptedKernel) -> MinixTransport<ScriptedKernel> {
        let mut t = MinixTransport::new(kernel);
        // 先挂载（C 的 `fsdriver_mounted`）：框架语义要求除 ReadSuper 外
        // 的请求都在挂载后才被服务。
        t.kernel_mut().inbox.push_back((
            vfs_request(REQ_READSUPER, 0xB01, &[(readsuper_req_off::DEVICE, 0)]),
            0,
        ));
        match t.next() {
            Some(Incoming::Fs(Request::Mount { is_root: false })) => {}
            other => panic!("mount 未分类: {other:?}"),
        }
        t.reply(Reply::Mounted(Ok(Ino(1))));
        t
    }

    /// 未挂载时除 ReadSuper 外一律 EINVAL（`fsdriver.c:40-46`），且回复已
    /// 由传输发出（循环收到 `Refused`，不再动手）。
    #[test]
    fn test_mount_gate_refuses_before_mount() {
        let mut k = ScriptedKernel::default();
        k.inbox.push_back((vfs_request(REQ_READ, 0xB02, &[]), 0));
        let mut t = MinixTransport::new(k);
        assert_eq!(t.next(), Some(Incoming::Refused));
        let sent = &t.kernel_mut().sent;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1.m_type, trns_add_id(EINVAL, 0xB02), "EINVAL 且回显 transid");
    }

    /// ReadSuper：挂载请求分类 + 回复盖 transid；`REQ_ISROOT` 位进 `is_root`。
    #[test]
    fn test_readsuper_encodes_is_root_flag() {
        let flags = minix_fs::protocol::MountFlags::IS_ROOT.0 as i64;
        let mut k = ScriptedKernel::default();
        k.inbox.push_back((vfs_request(REQ_READSUPER, 0xB07, &[(readsuper_req_off::FLAGS, flags)]), 0));
        let mut t = MinixTransport::new(k);
        assert_eq!(t.next(), Some(Incoming::Fs(Request::Mount { is_root: true })));
        t.reply(Reply::Mounted(Err(minix_types::Errno::from_i32(minix_types::EBUSY))));
        let sent = &t.kernel_mut().sent;
        assert_eq!(
            sent[0].1.m_type,
            trns_add_id(minix_types::EBUSY, 0xB07),
            "错误也走 TRNS_ADD_ID（结果值在高 16 位）"
        );
    }

    /// Lookup：名字经 grant 读入（`fsdriver_getname`），NUL 截断。
    #[test]
    fn test_lookup_reads_name_via_grant() {
        let mut t = ready(ScriptedKernel::default());
        t.kernel_mut().from_bytes = b"usb\0".to_vec();
        t.kernel_mut().inbox.push_back((
            vfs_request(
                REQ_LOOKUP,
                0xB03,
                &[
                    (lookup_req_off::DIR_INO, 7),
                    (lookup_req_off::PATH_LEN, 4),
                    (lookup_req_off::GRANT_PATH, 42),
                ],
            ),
            0,
        ));
        match t.next() {
            Some(Incoming::Fs(Request::Lookup { dir, name })) => {
                assert_eq!(dir, Ino(7));
                assert_eq!(name, "usb");
            }
            other => panic!("lookup 未分类: {other:?}"),
        }
        t.reply(Reply::Found(Err(minix_types::Errno::from_i32(minix_types::ENOENT))));
        let sent = &t.kernel_mut().sent;
        // `ready()` 已发过挂载回复，故看最后一条。
        assert_eq!(
            sent.last().unwrap().1.m_type,
            trns_add_id(minix_types::ENOENT, 0xB03)
        );
    }

    /// Read：数据面经 grant 写回调用方（`fsdriver_copyout`），状态是字节数。
    #[test]
    fn test_read_copies_out_via_grant() {
        let mut t = ready(ScriptedKernel::default());
        t.kernel_mut().inbox.push_back((
            vfs_request(
                REQ_READ,
                0xB04,
                &[
                    (transfer_req_off::INODE, 3),
                    (transfer_req_off::SEEK_POS, 0),
                    (transfer_req_off::GRANT, 9),
                    (transfer_req_off::BYTES, 64),
                ],
            ),
            0,
        ));
        assert_eq!(
            t.next(),
            Some(Incoming::Fs(Request::Read { ino: Ino(3), len: 64, pos: 0 }))
        );
        t.reply(Reply::Data(Ok(b"hello".to_vec())));
        let k = t.kernel_mut();
        assert_eq!(k.to_grant, Some((Endpoint::VFS, 9, 0)), "写回调用方 grant");
        assert_eq!(k.to_bytes, b"hello");
        assert_eq!(
            k.sent.last().unwrap().1.m_type,
            trns_add_id(5, 0xB04),
            "状态＝字节数"
        );
    }

    /// 非 VFS 来源的消息走 DEVMAN 面（`fsdriver.c:26-31`），不回信。
    #[test]
    fn test_non_vfs_source_goes_to_devman_face() {
        let mut k = ScriptedKernel::default();
        // DEVMAN_DEL：m4l2 = 设备 id。
        let mut m = Message {
            m_source: Endpoint(5),
            m_type: minix_types::DEVMAN_DEL_DEV,
            ..Message::default()
        };
        // union 字段写是 safe 的（只有读才 unsafe）；DEVMAN 的
        // BIND/UNBIND/DEL 用 m4 词（05 相位表）。
        m.m_u.m_m4.m4l2 = 11;
        k.inbox.push_back((m, 0));
        let mut t = MinixTransport::new(k);
        match t.next() {
            Some(Incoming::Devman { source, msg: Some(DevmanMsg::Del { device }) }) => {
                assert_eq!(source, Endpoint(5));
                assert_eq!(device, crate::structs::DeviceId(11));
            }
            other => panic!("DEVMAN 未分类: {other:?}"),
        }
        assert!(t.kernel_mut().sent.is_empty(), "other 面不回信");
    }

    /// 收包失败＝传输损坏：`next()` 给 None 让循环退出（调用方决定 panic）。
    #[test]
    fn test_receive_failure_ends_loop() {
        let mut t = MinixTransport::new(ScriptedKernel::default());
        assert_eq!(t.next(), None);
    }

    /// 出生面：RS_INIT（fresh）就地应答 `RS_INIT+OK` 并被吞掉，下一条
    /// 消息正常进入分类（C `sef_startup` 尾部 + `do_sef_init_request`）。
    #[test]
    fn test_rs_birth_fresh_replies_ok_and_continues() {
        let mut birth = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint::RS,
            ..Message::default()
        };
        // union 字段写是 safe 的；活跃臂 `m_rs_init`（type_=0 即
        // SEF_INIT_FRESH）。
        birth.m_u.m_rs_init.type_ = 0;
        let follow = Message {
            m_type: 0x1000 + 3, // 通知 → Devman 面提示
            m_source: Endpoint(7),
            ..Message::default()
        };

        let kernel = ScriptedKernel {
            inbox: VecDeque::from(vec![(birth, 0), (follow, 0)]),
            ..ScriptedKernel::default()
        };
        let mut t = MinixTransport::new(kernel);
        let incoming = t.next().expect("birth swallowed; next delivery lands");
        assert!(matches!(incoming, Incoming::Devman { msg: None, .. }));

        let kernel = t.kernel;
        assert_eq!(kernel.sent.len(), 1, "出生回信恰好一条");
        let (dest, reply) = &kernel.sent[0];
        assert_eq!(*dest, Endpoint::RS);
        assert_eq!(reply.m_type, minix_types::RS_INIT);
        // SAFETY(test): 回信臂 `m_rs_init.result`。
        unsafe {
            assert_eq!(reply.m_u.m_rs_init.result, minix_types::OK);
        }
    }

    /// 出生面：LU/RESTART 诚实拒 `ENOSYS` 并终止循环（fail-closed）。
    #[test]
    fn test_rs_birth_stateful_refused_ends_loop() {
        let mut birth = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint::RS,
            ..Message::default()
        };
        // type_=1 即 SEF_INIT_LU。
        birth.m_u.m_rs_init.type_ = 1;
        let kernel = ScriptedKernel {
            inbox: VecDeque::from(vec![(birth, 0)]),
            ..ScriptedKernel::default()
        };
        let mut t = MinixTransport::new(kernel);
        assert!(t.next().is_none(), "init 被拒 → 循环停机");
        // 回信仍是 RS_INIT + ENOSYS（RS 按崩溃处置，不假装就绪）。
        let kernel = t.kernel;
        let (dest, reply) = &kernel.sent[0];
        assert_eq!(*dest, Endpoint::RS);
        // SAFETY(test): 同上。
        unsafe {
            assert_eq!(reply.m_u.m_rs_init.result, minix_types::ENOSYS);
        }
    }

    /// 非 RS 源的 RS_INIT 不拦截（走既有 DEVMAN 面）——出生判定同时看
    /// 类型与来源（C `IS_SEF_INIT_REQUEST`，sef.h:33-34）。
    #[test]
    fn test_rs_init_from_other_source_not_intercepted() {
        let mut fake = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint(5), // 非 RS
            ..Message::default()
        };
        let kernel = ScriptedKernel {
            inbox: VecDeque::from(vec![(fake, 0)]),
            ..ScriptedKernel::default()
        };
        let mut t = MinixTransport::new(kernel);
        let incoming = t.next().expect("非 RS 源不拦截");
        assert!(matches!(incoming, Incoming::Devman { .. }));
    }
}

//! VFS↔MFS cross-package bridge v0 — a single-segment LOOKUP probe (four real layers).
//!
//! # Chain
//!
//! The user side's `minix_sys::vfs::open_via` (the Rust counterpart of C libc open)
//! issues an open request → `VfsState::run_once` runs the real main loop: path
//! resolution, worker binding, grant registration (the path grant points at a **real
//! host address**) → `pending_fs` suspends a LOOKUP request → the bridge (`task::run` +
//! this file's [`Bridge`]) decodes with fs-rt's `decode_body`, `task::dispatch` drives
//! **a real MfsServer on a real disk**, `encode_reply` encodes back → `handle_fs_reply`
//! lands it → open completes locally and answers the user.
//!
//! # Rulings (the bridge's two design points)
//!
//! 1. **Grant host model = direct reads of real addresses**: VFS's `GrantTable`
//!    registration already holds real host addresses (worker scratch, user buffers); the
//!    bridge takes the window through `GrantTable::probe` (a kernel-view read,
//!    `minix-sys`) and reads/writes through the bare pointer — consistent with VFS's own
//!    host model and the mib_sysctl bare-pointer replay case.
//! 2. **Request mapping = FS_BASE offset**: VFS `REQ_*` and fs-rt `RequestNumber` share
//!    one origin (the C `vfsif.h` index), and `m_type - FS_BASE` is the table index.
//!
//! # Coverage boundary
//!
//! This probe covers the LOOKUP segment; the data plane of open (read/write) and
//! non-path requests (ReadSuper and the like) are laid out in later segments. The user
//! sendrec round trip ends in failure with EIO in the probe — the real answer lands in
//! `VfsState`'s reply queue, and assertions use that.

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
const USER_EP: Endpoint = Endpoint::from_generation_slot(2, 0); // slot 0 = the seeded process slot

// ---------------------------------------------------------------------------
// The bridge's grant channel: take the window from the kernel view, read and write
// real addresses directly
// ---------------------------------------------------------------------------

struct BridgeIpc<'a> {
    grants: &'a GrantTable,
}

impl RtIpc for BridgeIpc<'_> {
    fn receive(
        &mut self,
        _src: Endpoint,
        _msg: &mut Message,
        _on_signal: &mut dyn FnMut(i32),
        _cancel: &minix_sef::SefCancel,
    ) -> Result<minix_fs_rt::ipc::Receipt, i32> {
        Err(minix_types::EIO) // the bridge does not use the SEF receive half (requests are fed in by the VFS side), so there is no signal number to return
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
            return Err(minix_types::EINVAL); // beyond the grant window: the C kernel rules the same
        }
        // SAFETY: the window address is held by VFS's grant registration (real host
        // memory), and the bounds were checked against the grant length above.
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
        // SAFETY: same as copy_from, write half.
        unsafe {
            core::slice::from_raw_parts_mut((start + offset) as *mut u8, bytes.len())
                .copy_from_slice(bytes);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The bridge: the two-way seam between VFS pending_fs and task::run
// ---------------------------------------------------------------------------

struct Bridge<'a> {
    state: &'a mut VfsState,
    blocking: &'a mut minix_vfs::fs_comm::BlockingTransport,
    rounds: u32,
    last_rn: Option<RequestNumber>,
}

impl TaskTransport for Bridge<'_> {
    fn receive(&mut self) -> Incoming {
        // Capture the request registered by the arm first (flush consumes pending), then run the send half.
        let Some((raw_req, worker)) = self
            .state
            .pending_fs
            .as_ref()
            .map(|p| (p.req.clone(), p.worker))
        else {
            return Incoming::Cancelled; // suspended state cleared: the syscall has already finished
        };
        self.state.flush_pending_fs(self.blocking);
        // Reproduce the transid stamping of the production send half (fs_sendrec encodes
        // the worker slot into the top 16 bits of m_type via TransId::add, com.h:909-911)
        // — the wire shape.
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
        // The kernel stamps the source on the reply when delivering it (VFS finds the vmnt by source, C main.c:191).
        msg.m_source = MFS_EP;
        self.state
            .handle_fs_reply(&msg, &VfsTransIdCodec)
            .expect("VFS 接受桥回灌的 FS 应答");
        // Continuation progress: the lookup reply drives walk/open to completion and may register the next request.
        self.state.run_worker_continuations();
    }

    fn copy_in(&mut self, _offset: usize, _out: &mut [u8]) -> Result<(), minix_types::Errno> {
        Err(minix_types::Errno::from_i32(minix_types::ENOSYS)) // LOOKUP has no data plane
    }
    fn copy_out(&mut self, _offset: usize, _bytes: &[u8]) -> Result<(), minix_types::Errno> {
        Ok(()) // the bridge test only verifies the request/reply chain; the data plane does not land
    }
}

// ---------------------------------------------------------------------------
// Fixture: a real-disk MFS plus a seeded VfsState
// ---------------------------------------------------------------------------

/// Real-disk MFS: mkfs builds the image → ImgrdBlockSource → mount → create "hello" under the root.
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

/// Seeded VfsState: one vmnt row, the root vnode, a user process slot, and a grant warm-up.
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
    // Grant warm-up: under a host build the first registration after the table grows
    // fails (the freelist is already laid out) and the second succeeds — consistent with
    // the crate's seed_ready_state case.
    loop {
        match state.grants.grant_direct(
            &minix_sys::syscall::DirectKernelCallTransport,
            MFS_EP.0,
            0,
            1,
            minix_types::CpFlags::READ,
        ) {
            Ok(g) => {
                // [SD19-gate] revoke the grant inside the test fixture: best effort; a failure shows up in later assertions
                #[allow(clippy::let_underscore_must_use)]
                let _ = state.grants.revoke(g);
                break;
            }
            Err(_) => continue,
        }
    }
    state
}

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

#[test]
fn lookup_bridge_resolves_open() {
    let (mfs, hello_ino) = mfs_with_hello();
    let mut state = seeded_state();

    // A real request from the user side: the minix_sys open wrapper issues it, with the path grant pointing at a real buffer.
    let mut path = b"hello\0".to_vec();
    let mut canned = CannedTransport::new();
    canned.reply_sendrec(Err(minix_sys::ipc::TrapStatus(minix_types::EIO)));
    let _ = open_via(&canned, path.as_ptr() as u64, path.len(), 0, 0);
    let (_src, mut req) = canned.sent.borrow()[0].clone();
    // The kernel stamps m_source on delivery; the probe uses USER_EP as the kernel stamp.
    req.m_source = USER_EP;

    // The real VFS main loop: open suspends on LOOKUP.
    let route = state.run_once(&req, &VfsTransIdCodec);
    assert!(
        state.pending_fs.is_some(),
        "open 应挂起在 LOOKUP 请求上(route={route:?})"
    );

    // The bridge drives the real MfsServer: the outer loop is "send half → task::run → continuation" until open completes.
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

    // open completes locally: answer the user OK + fd; hello's vnode is in the table.
    let (target, reply) = state.take_reply().expect("open 应有用户应答");
    assert_eq!(target, USER_EP);
    assert_eq!(reply.m_type, minix_types::OK, "open 成功");
    // SAFETY(test): fd is the first word of the reply payload (the fd lane of mess_vfs_lc_open).
    let fd = unsafe { i32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()) };
    assert!(fd >= 0, "回复载荷带非负 fd,得 {fd}");

    let found = (0..state.vnode_table.len())
        .map(|i| state.vnode_table.get(minix_vfs::vnode::VnodeId(i)).unwrap())
        .find(|v| v.fs == MFS_EP && v.ino == hello_ino);
    assert!(found.is_some(), "hello 的 vnode 已被 lookup 落表");
}

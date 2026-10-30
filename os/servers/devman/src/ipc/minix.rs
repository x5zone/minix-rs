//! The production transport: devman sits on VFS's fsdriver protocol as a
//! "filesystem-shaped server" and receives DEVMAN messages in the same loop
//! (C's single-loop shape of `fsdriver_task` plus `fdr_other`).
//!
//! # Layering
//!
//! - [`KernelIpc`]: the kernel face (receive/send/round trip/notify/both
//!   directions of grant copy) — the production implementation is [`SysKernel`]
//!   (`minix-sys`'s `DirectTrapTransport` plus `sys_safecopyfrom/to`), and tests
//!   inject a scripted stand-in. **This is the whole reason the module is
//!   testable**: wire encode/decode, classification and reply shaping never
//!   touch the kernel.
//! - [`MinixTransport`]: the production `Transport` — `next()` receives and
//!   classifies, `reply()/send()/sendrec()` shape a message per the fsdriver
//!   protocol and send it.
//!
//! # Protocol points (each against a C anchor)
//!
//! - **transid**: a VFS request carries `m_type = TRNS_ADD_ID(call_nr, transid)`
//!   and the reply `m_type = TRNS_ADD_ID(result, transid)` (`fsdriver.c:34-50`);
//!   the arithmetic uses `minix_types`' `trns_*` (the single authority,
//!   vfsif.h:79-81).
//! - **Entry split**: `is_ipc_notify(status) || source != VFS` → `fdr_other`
//!   with **no reply** (`fsdriver.c:26-31`); devman's `fdr_other` is the DEVMAN
//!   message face (`DevmanMsg::classify`).
//! - **mount gate**: while unmounted everything except `REQ_READSUPER` gets
//!   `EINVAL` (`fsdriver.c:40-46`); request numbers go through `RequestNumber`
//!   (an empty `GetNode` slot → `ENOSYS`).
//! - **The data face is a grant**: the `grant` of `REQ_READ`/`REQ_GETDENTS`
//!   points at the **caller's** buffer, and the FS side writes the data with
//!   `sys_safecopyto` (`libfsdriver`'s `fsdriver_copyout`); `REQ_LOOKUP` reads
//!   the name with `sys_safecopyfrom` (C's `fsdriver_getname`, `call.c:36-41`).

use minix_types::{
    is_fs_rq, lookup_req_off, readsuper_req_off, transfer_req_off, trns_add_id, trns_del_id,
    trns_get_id, Endpoint, Message, ENOSYS, EINVAL, REQ_GETDENTS, REQ_LOOKUP, REQ_READ,
    REQ_READSUPER, REQ_UNMOUNT,
};

use crate::ipc::DevmanMsg;
use crate::vtreefs::{Incoming, Reply, Request, Transport};

/// The kernel face: receive/send/round trip/notify/both directions of grant
/// copy.
///
/// The production implementation is [`SysKernel`]; tests use a scripted
/// stand-in (both this module's tests and `server.rs`'s integration tests rely
/// on it).
pub trait KernelIpc {
    /// Blocking receive (`ipc_receive` semantics: returns the status word).
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32>;
    /// Notify (SEF ping's pong goes through here, `sef_ping.c:61`).
    fn notify(&mut self, dest: Endpoint) -> Result<(), i32>;
    /// Non-blocking send (fsdriver replies use `ipc_send`, devman's DEVMAN
    /// replies use `ipc_send`, and starting a forward uses `ipc_sendrec`).
    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), i32>;
    /// Blocking round trip (C `ipc_sendrec`, bind.c:32/80).
    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), i32>;
    /// The state-control verb (C `sys_statectl`, libsys sys_statectl.c:3-11) —
    /// the birth protocol's first section clears its IPC filter through this
    /// (`process_init`). Returns the kernel result code (OK or a negative
    /// errno).
    fn statectl(&mut self, request: i32, address: u64, length: i32) -> i32;
    /// `sys_safecopyfrom(granter, grant, off, buf)` — read bytes in from the
    /// caller's buffer (for names and payloads).
    fn safecopy_from(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32>;
    /// `sys_safecopyto(granter, grant, off, buf)` — write result bytes back to
    /// the caller's buffer (read data and directory entries).
    fn safecopy_to(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32>;
}

/// The production kernel face: `minix-sys`'s trap transport plus kernel calls.
#[derive(Debug, Default)]
pub struct SysKernel;

impl KernelIpc for SysKernel {
    fn statectl(&mut self, request: i32, address: u64, length: i32) -> i32 {
        minix_sys::syscall::sys_statectl(
            &minix_sys::syscall::DirectKernelCallTransport,
            request,
            address,
            length,
        )
    }

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

/// The `KernelIpc` → `minix_sef::SefIpc` bridge (SEF receive needs only the
/// receive and notify verbs).
struct SefBridge<'a, K: KernelIpc>(&'a mut K);

impl<K: KernelIpc> minix_sef::SefIpc for SefBridge<'_, K> {
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        self.0.receive(src, msg)
    }
    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.0.notify(dest)
    }
}

/// The production `Transport`: receive one, classify it, reply — the FS face
/// and the DEVMAN face share one loop.
pub struct MinixTransport<K: KernelIpc> {
    kernel: K,
    /// The SIGTERM latch (C libvtreefs `got_signal` → `fsdriver_terminate()`
    /// sets libfsdriver's `running = FALSE`, vtreefs.c:39-46 + fsdriver.c:68-74
    /// — `running` is exactly the transport loop's stop flag, and the Rust
    /// counterpart lives on this layer).
    terminate: bool,
    /// The current request's grant and granter (needed by `reply()`: the data
    /// face writes back to the caller's buffer).
    current_grant: i32,
    current_granter: Endpoint,
    /// The current request's transid (echoed in the reply).
    current_transaction: u32,
    /// The current request's caller (the reply destination; C's
    /// `m_ptr->m_source`).
    current_source: Endpoint,
    /// Mounted yet? (the mount gate; kept separately from the `VTreeFs` state
    /// in `Server::process_fs` — C's `fsdriver_mounted` is a library global,
    /// Rust keeps one copy in the transport, and `Server::run`'s mount
    /// semantics are unchanged).
    mounted: bool,
}

impl<K: KernelIpc> MinixTransport<K> {
    pub const fn new(kernel: K) -> Self {
        Self {
            kernel,
            terminate: false,
            current_grant: 0,
            current_granter: Endpoint::NONE,
            current_transaction: 0,
            current_source: Endpoint::NONE,
            mounted: false,
        }
    }

    /// Lend out the kernel face (for tests and assembly).
    pub fn kernel_mut(&mut self) -> &mut K {
        &mut self.kernel
    }

    /// Receive one message (SEF semantics: a ping is transparent; a signal
    /// request is intercepted inside the library and handed to the callback).
    ///
    /// The ground truth for the signal face: devman's C main goes through
    /// `run_vtreefs` (main.c:88), and libvtreefs's `sef_local_startup`
    /// registered `got_signal` (registered at vtreefs.c:59, implemented at
    /// :39-46 — anything but SIGTERM is ignored, SIGTERM turns into
    /// `fsdriver_terminate()`). So "devman registers no signal handler" is a
    /// false statement — the registration happens in the library it links,
    /// the same family as procfs; a directory-level grep of `servers/devman/`
    /// cannot see the library's registration point and is no evidence about the
    /// registration surface. The Rust counterpart: SIGTERM latches `terminate`
    /// in the callback and cancels the pending receive
    /// (`fsdriver_terminate()` = `running=FALSE` plus `sef_cancel`,
    /// fsdriver.c:68-74 — the escape hatch makes this call return `Err(EINTR)`
    /// on the spot, and the loop stops at `next()`'s termination check).
    fn receive_message(&mut self, msg: &mut Message) -> Option<i32> {
        let cancel = minix_sef::SefCancel::new();
        // Split the field borrows: the bridge mutably borrows `kernel` and the
        // callback writes `terminate` — two disjoint fields, so the closure and
        // the bridge can coexist.
        let Self {
            kernel, terminate, ..
        } = self;
        let mut bridge = SefBridge(kernel);
        let mut on_signal = |signo: i32| {
            // C `got_signal` (vtreefs.c:39-46): everything but SIGTERM is
            // ignored.
            if signo == minix_types::SIGNAL_TERMINATE {
                *terminate = true;
                cancel.cancel();
            }
        };
        match minix_sef::sef_receive_status(
            &mut bridge,
            Endpoint::ANY,
            msg,
            &mut on_signal,
            &cancel,
        ) {
            Ok(recv) => Some(recv.status),
            // `Err(EINTR)` is the escape hatch's clean stop: the termination
            // decision is already latched in the callback, and C's counterpart
            // is the loop exit after `fsdriver_terminate()`, not corruption.
            // Any other receive failure is transport corruption: C panics
            // (fsdriver.c:92), Rust leaves it to the caller (main) — here it
            // likewise returns None so the loop exits.
            Err(minix_types::EINTR) => None,
            Err(_) => None,
        }
    }

    /// Decode the `REQ_LOOKUP` payload: the name is read in through the grant
    /// (C `fsdriver_getname`).
    ///
    /// Payload layout (C `mess_vfs_fs_lookup` — ipc.h:2011-2035; the Rust side
    /// is the LP64 view of the same field order):
    /// `dir_ino@0`, `root_ino@8`, `flags@16`, `path_len@24`, `path_size@32`,
    /// `ucred_size@40`, `grant_path@48`, `grant_ucred@56`.
    fn decode_lookup(&mut self, msg: &Message) -> Option<Request> {
        // SAFETY: VFS writes the REQ_LOOKUP payload into the message payload
        // area in the field order given above.
        let raw = unsafe { &msg.m_u.raw };
        let dir = read_i64(raw, lookup_req_off::DIR_INO) as u64;
        let path_len = read_i64(raw, lookup_req_off::PATH_LEN) as usize;
        let grant_path = read_i64(raw, lookup_req_off::GRANT_PATH) as i32;
        // Name length gate: 0 or above the limit is rejected (C's
        // `path_len = strlen + 1`, `PATH_MAX 255` — sys/sys/syslimits.h).
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
        // C's path is NUL-terminated (`path_len = strlen + 1`,
        // request.c:447).
        let end = name.iter().position(|b| *b == 0).unwrap_or(name.len());
        let name = alloc::string::String::from_utf8_lossy(&name[..end]).into_owned();
        self.current_grant = grant_path;
        self.current_granter = msg.m_source;
        Some(Request::Lookup {
            dir: crate::vtreefs::Ino(dir as u32),
            name,
        })
    }

    /// The common prefix of `REQ_READ`/`REQ_GETDENTS`: `inode@0`, `seek_pos@8`,
    /// `grant@16`, `nbytes/mem_size@24` (C `mess_vfs_fs_readwrite` —
    /// ipc.h:2121-2131, `mess_vfs_fs_getdents` — ipc.h:1992-2002).
    fn decode_transfer(&mut self, msg: &Message) -> (u64, i64, i32, usize) {
        // SAFETY: as above, read in field order.
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

/// The name buffer limit: C `PATH_MAX 255` (sys/sys/syslimits.h) plus the
/// NUL.
const PATH_BUF_MAX: usize = 256;

/// Read one 8-byte little-endian word from the message payload's byte area
/// (0 when out of range).
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
            // C `while (running)` (the loop condition in fsdriver.c; running is
            // cleared by `fsdriver_terminate()`): after termination nothing is
            // received or sent, and the loop stops.
            if self.terminate {
                return None;
            }
            let mut msg = Message::default();
            let status = self.receive_message(&mut msg)?;
            let notify = minix_sef::is_ipc_notify(status);
            let source = msg.m_source;

            // **Birth face** (the RS_INIT handshake shared across the tree): the
            // RS send leg (boot step2, matching main.c:365/387, plus do_up's
            // `rs_asynsend`, utility.c:62) already lives in the library, so this
            // interception is the answering half of the handshake — C
            // `sef_startup`'s tail plus `do_sef_init_request`
            // (sef_init.c:193-215). A fresh request is answered in place with
            // `RS_INIT+OK` (devman's tree is already built by `Server::new`,
            // equivalent to the first-build semantics of C `init_hook`), while
            // LU/RESTART are honestly refused with `ENOSYS` and the loop
            // terminates (C panics on init failure; a single-threaded server
            // stops fail-closed and RS treats it as a crash). A third copy of
            // this logic lives in the sched transport layer: the three copies
            // exist because of layering (fs-rt depends on minix-fs's FsDriver
            // face, devman and sched each carry their own transport layer), and
            // unifying them is left to a later decision (fs-rt is out of bounds
            // here, so the three cannot be folded into one now).
            if !notify && msg.m_type == minix_types::RS_INIT && source == Endpoint::RS {
                // SAFETY: the active union arm of a birth request is
                // `m_rs_init` (m_type == RS_INIT and the source is RS).
                let kind = unsafe { msg.m_u.m_rs_init.type_ };
                // The birth protocol's orchestration belongs to the library:
                // its first section clears the IPC filter (C `process_init`'s
                // first section, the `assert(r == OK)` at sef_init.c:59-60,
                // really wired through `KernelIpc::statectl`), and the library
                // builds the report message. The dispatch semantics are
                // unchanged (devman's tree is already built by `Server::new`,
                // equivalent to the first-build semantics of C `init_hook` →
                // fresh OK; LU/RESTART are honestly refused with `ENOSYS`).
                let reply = minix_sef::process_init(
                    &mut |req, addr, len| {
                        let r = self.kernel.statectl(req, addr, len);
                        if r == minix_types::OK {
                            Ok(r)
                        } else {
                            Err(r)
                        }
                    },
                    kind,
                    &mut |t| if t == 0 { minix_types::OK } else { minix_types::ENOSYS },
                );
                let mut reply = match reply {
                    Ok(m) => m,
                    // C's section-one assert means a birth failure is
                    // fail-closed (the single-threaded server stops and RS
                    // treats it as a crash — the same as a refused init
                    // below).
                    Err(_) => return None,
                };
                // The birth report leg = C `sef_cb_init_response_rs_reply`
                // = `ipc_sendrec(RS, &m)` (sef_init.c:458-466), not a plain
                // send: after sending RS_INIT it parks in the receive(RS) half,
                // waiting for the OK wakeup from RS's catch exit.
                //[SD19-gate] sendrec wakes RS at birth; a failure does not
                // abort
                #[allow(clippy::let_underscore_must_use)]
                let _ = self.kernel.sendrec(Endpoint::RS, &mut reply);
                if reply.rs_init_result() == Some(minix_types::OK) {
                    continue; // birth answered: swallow this one, keep serving
                }
                return None; // init refused: stop fail-closed
            }

            // C `fsdriver_process:26-31`: a non-request (a notification or a
            // message from another service) takes the `other` path with no
            // reply. devman's other path is the DEVMAN message face.
        if notify || source != Endpoint::VFS {
            if notify {
                return Some(Incoming::Devman { source, msg: None });
            }
            let m4 = unsafe { &msg.m_u.m_m4 };
            let (word2, word3) = (m4.m4l2 as i32, m4.m4l3 as i32);
            // ADD's payload sits in the caller's grant, so read it in through
            // the grant first.
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

        // FS face: strip the transid first, then classify by request number
        // (an empty `GetNode` slot → the Unserved semantics that C's
        // `fsdriver_callvec` empty slot provides).
        // On the wire the high 16 bits of m_type are the call number
        // (vfsif.h:79-81).
        if !is_fs_rq(trns_del_id(msg.m_type)) {
            // The number is not in the FS range: C would index out of bounds
            // with `call_nr - FS_BASE` → ENOSYS; here it goes back to the DEVMAN
            // face (`DevmanMsg::classify` yields None = no reply).
            return Some(Incoming::Devman { source, msg: None });
        }
        let call = trns_del_id(msg.m_type);
        self.current_transaction = trns_get_id(msg.m_type);
        self.current_source = source;

        // The mount gate (`fsdriver.c:40-46`): while unmounted everything but
        // ReadSuper gets EINVAL. `reply()` would answer it as `mount_refused`,
        // but a dedicated Incoming variant cannot express that, so the decision
        // is made in this function and a "refused request" is returned for run
        // to answer directly.
        let request = match call {
            REQ_READSUPER => {
                // Payload: device@0, flags@8 (C `mess_vfs_fs_readsuper` —
                // ipc.h:2112-2119; the `REQ_ISROOT` bit is in flags).
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
                // Unmounted (and not ReadSuper), or a request number devman
                // does not serve: C splits these into EINVAL / ENOSYS
                // (`fsdriver.c:40-56`).
                let status = if self.mounted { ENOSYS } else { EINVAL };
                //[SD19-gate] the error reply's send is best effort
                #[allow(clippy::let_underscore_must_use)]
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
        // Data face: read data and directory entries are written back to the
        // caller's buffer through the grant (C `fsdriver_copyout`).
        if let Some(PayloadKind::Data(bytes)) = &payload
            && self
                .kernel
                .safecopy_to(self.current_granter, self.current_grant, 0, bytes)
                .is_err()
        {
            // Copy-out failed: reply EFAULT (in C, a `fsdriver_copyout`
            // failure is that errno).
            //[SD19-gate] the EFAULT reply's send is dropped on failure
            #[allow(clippy::let_underscore_must_use)]
            let _ = self.kernel.send(
                self.current_source,
                &reply_msg(minix_types::EFAULT, self.current_transaction),
            );
            return;
        }
        let mut m = reply_msg(status, self.current_transaction);
        if let Some(PayloadKind::Node(ino)) = payload {
            // The reply fields of `struct fsdriver_node` (C call.c:57-62: the
            // node number and attributes share one message) — devman's node
            // attributes are stat'ed separately by VFS, so only the node number
            // is returned (the first field of C's `fsdriver_node`).
            // SAFETY: the byte area of the reply payload.
            unsafe {
                m.m_u.raw[0..8].copy_from_slice(&(ino.0 as u64).to_le_bytes());
            }
        }
        //[SD19-gate] the terminal reply's send
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.kernel.send(self.current_source, &m);
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) {
        //[SD19-gate] an abstract send layer; nothing above consumes a
        // failure
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.kernel.send(dest, msg);
    }

    fn sendrec(&mut self, owner: Endpoint, msg: &mut Message) -> Result<(), minix_types::Errno> {
        self.kernel
            .sendrec(owner, msg)
            .map_err(|_| minix_types::Errno::from_i32(minix_types::EIO))
    }
}

/// What a reply carries besides the status (data face / node number).
enum PayloadKind {
    Data(alloc::vec::Vec<u8>),
    Node(crate::vtreefs::Ino),
    Dents,
}

/// Shape one fsdriver reply: `m_type = TRNS_ADD_ID(status, transid)`.
fn reply_msg(status: i32, transaction: u32) -> Message {
    Message {
        m_type: trns_add_id(status, transaction),
        ..Message::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vtreefs::{Ino, Transport};
    use alloc::collections::VecDeque;

    /// A scripted kernel face: an inbox queue, send recording and injectable
    /// safecopy failures.
    #[derive(Default)]
    struct ScriptedKernel {
        inbox: VecDeque<(Message, i32)>,
        sent: alloc::vec::Vec<(Endpoint, Message)>,
        sendrecs: alloc::vec::Vec<(Endpoint, Message)>,
        /// The bytes the next `safecopy_from` hands to the caller (names and
        /// the like).
        from_bytes: alloc::vec::Vec<u8>,
        /// The bytes `safecopy_to` received (read data and directory entry
        /// write-backs).
        to_bytes: alloc::vec::Vec<u8>,
        to_grant: Option<(Endpoint, i32, u64)>,
        fail_from: bool,
    }

    impl KernelIpc for ScriptedKernel {
        fn statectl(&mut self, _request: i32, _address: u64, _length: i32) -> i32 {
            minix_types::OK
        }
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

    /// Build a request as sent by VFS: `m_type = TRNS_ADD_ID(call, transid)`.
    fn vfs_request(call: i32, transid: u32, payload: &[(usize, i64)]) -> Message {
        let mut m = Message {
            m_source: Endpoint::VFS,
            m_type: trns_add_id(call, transid),
            ..Message::default()
        };
        // SAFETY(test): fill the payload per the offset table.
        unsafe {
            for (at, v) in payload {
                m.m_u.raw[*at..*at + 8].copy_from_slice(&v.to_le_bytes());
            }
        }
        m
    }

    fn ready(kernel: ScriptedKernel) -> MinixTransport<ScriptedKernel> {
        let mut t = MinixTransport::new(kernel);
        // Mount first (C's `fsdriver_mounted`): the framework requires every
        // request but ReadSuper to be served only after mounting.
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

    /// While unmounted everything but ReadSuper gets EINVAL
    /// (`fsdriver.c:40-46`), and the reply has already been sent by the
    /// transport (the loop gets `Refused` and does nothing more).
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

    /// ReadSuper: the mount request is classified and the reply carries the
    /// transid; the `REQ_ISROOT` bit lands in `is_root`.
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

    /// Lookup: the name is read in through the grant (`fsdriver_getname`) and
    /// truncated at the NUL.
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
        // `ready()` already sent the mount reply, so look at the last one.
        assert_eq!(
            sent.last().unwrap().1.m_type,
            trns_add_id(minix_types::ENOENT, 0xB03)
        );
    }

    /// Read: the data face writes back to the caller through the grant
    /// (`fsdriver_copyout`), and the status is the byte count.
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

    /// A message from a non-VFS source takes the DEVMAN face
    /// (`fsdriver.c:26-31`) and gets no reply.
    #[test]
    fn test_non_vfs_source_goes_to_devman_face() {
        let mut k = ScriptedKernel::default();
        // DEVMAN_DEL: m4l2 = the device id.
        let mut m = Message {
            m_source: Endpoint(5),
            m_type: minix_types::DEVMAN_DEL_DEV,
            ..Message::default()
        };
        // Writing a union field is safe (only reads are unsafe); DEVMAN's
        // BIND/UNBIND/DEL use the m4 words.
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

    /// A receive failure means transport corruption: `next()` gives None so the
    /// loop exits (the caller decides on a panic).
    /// A manager-shaped signal frame (C: the signal manager translates the
    /// number into `SIGS_SIGNAL_RECEIVED` carrying `num` — pm/signal.c:470-473,
    /// rs/main.c:699-701). Writing a union field is safe (only reads are
    /// unsafe); the `m_type` tag selects the payload arm.
    fn manager_signal_msg(signo: i32) -> Message {
        let mut m = Message {
            m_source: Endpoint::PM,
            m_type: minix_types::SIGS_SIGNAL_RECEIVED,
            ..Message::default()
        };
        m.m_u.m_pm_lsys_sigs_signal.num = signo;
        m
    }

    #[test]
    fn test_manager_sigterm_latches_terminate_and_stops_loop() {
        // Ground truth for devman's signal face: C goes through run_vtreefs
        // (main.c:88), and libvtreefs registers `got_signal`
        // (vtreefs.c:59/:39-46) — SIGTERM turns into `fsdriver_terminate()`
        // (running=FALSE plus sef_cancel, fsdriver.c:68-74). The counterpart
        // here: a manager-shaped SIGTERM latches `terminate` in the callback and
        // cancels the pending receive, so `next()` returns None at the
        // termination check — the loop stops and no further frames are served
        // (the escape hatch makes the receive return EINTR on the spot).
        let mut k = ScriptedKernel::default();
        k.inbox
            .push_back((manager_signal_msg(minix_types::SIGNAL_TERMINATE), 0));
        // If the library kept receiving after the cancel, the empty inbox would
        // make the fixture return EIO; with the escape hatch gone that EIO would
        // fold to None through the same `Err(_)` arm, so the `terminate` latch
        // distinguishes a clean stop from a corrupt one.
        let mut transport = MinixTransport::new(k);
        assert_eq!(transport.next(), None, "SIGTERM 后循环停止");
        assert!(transport.terminate, "锁存置位（干净停止而非损坏）");
    }

    #[test]
    fn test_receive_failure_ends_loop() {
        let mut t = MinixTransport::new(ScriptedKernel::default());
        assert_eq!(t.next(), None);
    }

    /// Birth face: a (fresh) RS_INIT is answered in place with `RS_INIT+OK` and
    /// swallowed, and the next message is classified normally (C
    /// `sef_startup`'s tail plus `do_sef_init_request`).
    #[test]
    fn test_rs_birth_fresh_replies_ok_and_continues() {
        let mut birth = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint::RS,
            ..Message::default()
        };
        // Writing a union field is safe; the active arm is `m_rs_init`
        // (type_=0 means SEF_INIT_FRESH).
        birth.m_u.m_rs_init.type_ = 0;
        let follow = Message {
            m_type: 0x1000 + 3, // a notification → surfaces on the Devman face
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
        assert_eq!(kernel.sendrecs.len(), 1, "出生回信恰好一条（sendrec）");
        let (dest, reply) = &kernel.sendrecs[0];
        assert_eq!(*dest, Endpoint::RS);
        assert_eq!(reply.m_type, minix_types::RS_INIT);
        // SAFETY(test): the reply arm `m_rs_init.result`.
        unsafe {
            assert_eq!(reply.m_u.m_rs_init.result, minix_types::OK);
        }
    }

    /// Birth face: LU/RESTART are honestly refused with `ENOSYS` and the loop
    /// terminates (fail-closed).
    #[test]
    fn test_rs_birth_stateful_refused_ends_loop() {
        let mut birth = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint::RS,
            ..Message::default()
        };
        // type_=1 means SEF_INIT_LU.
        birth.m_u.m_rs_init.type_ = 1;
        let kernel = ScriptedKernel {
            inbox: VecDeque::from(vec![(birth, 0)]),
            ..ScriptedKernel::default()
        };
        let mut t = MinixTransport::new(kernel);
        assert!(t.next().is_none(), "init 被拒 → 循环停机");
        // The reply is still RS_INIT + ENOSYS (RS treats it as a crash rather
        // than pretending the server is ready).
        // The birth leg goes through sendrec, recorded in sendrecs.
        let kernel = t.kernel;
        let (dest, reply) = &kernel.sendrecs[0];
        assert_eq!(*dest, Endpoint::RS);
        // SAFETY(test): as above.
        unsafe {
            assert_eq!(reply.m_u.m_rs_init.result, minix_types::ENOSYS);
        }
    }

    /// An RS_INIT from a non-RS source is not intercepted (it takes the
    /// existing DEVMAN face) — the birth test looks at both type and source (C
    /// `IS_SEF_INIT_REQUEST`, sef.h:33-34).
    #[test]
    fn test_rs_init_from_other_source_not_intercepted() {
        let fake = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint(5), // not RS
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

//! VTreeFS framework subset for devman (doc 02-vtreefs-framework).
//!
//! C: `minix3/minix/lib/libvtreefs/` — `vtreefs.c` (run + SEF + `fs_other`),
//! `table.c` (17-slot `fsdriver` table), `mount.c` (`fs_mount`/`fs_unmount`),
//! `file.c` (`fs_read` loop), `inode.c` (tree).
//!
//! This is the **devman usage subset** (plan §3.4, decision A-1: implemented
//! inside the devman crate; the shared `minix-vtreefs` crate stays a stub,
//! the same split the RS server uses with `minix-sef`). Only what devman
//! wires up exists here:
//!
//! | C entry | Rust | Note |
//! |---|---|---|
//! | `run_vtreefs` init half | [`VTreeFs::new`] | init sequence |
//! | `fsdriver_task` dispatch half | [`crate::server::Server::run`] | the loop lives on `Server` so DEVMAN messages and FS requests share one dispatch truth (DM-P1-2); C's loop (`fsdriver_task`) is likewise the single entry |
//! | `fs_mount`/`fs_unmount` | [`VTreeFs::mount`]/[`VTreeFs::unmount`] | REQ_ISROOT→EINVAL; cleanup_hook absent (devman never registers it, main.c:78-80) |
//! | `fs_read` | [`VTreeFs::read`] | full chunk loop incl. partial-result rule |
//! | `fs_getdents` traversal | [`VTreeFs::readdir`] | entry order only; dirent wire encoding belongs to the VFS transport |
//! | `fs_lookup`/`fs_stat` shape | [`VTreeFs::lookup`]/stat getters | traversal + attrs; `struct stat` encoding deferred like dirents |
//! | `fs_other` | `Server::run` Devman arm | C forwards non-FS messages to the registered `message_hook`; devman-rs has no hook — the unified loop dispatches `DevmanMsg`s directly (one switch, like C's single `fsdriver` table) |
//! | write/trunc/mknod/… | `Err(ENOSYS)` | devman leaves the slots NULL; C `fs_write` without hook is `EACCES` (file.c:122) — the framework default for unwired mutating ops is explicit `ENOSYS` |
//!
//! # Single-threaded model
//!
//! One `VTreeFs` per process, driven by the devman event loop. No atomics,
//! no locks (see `crate` docs).

use alloc::string::String;
use alloc::vec::Vec;
use minix_types::{Endpoint, Errno, Message};

use crate::hooks::{FsHooks, ReadHookFn, ServerConfig, S_IFDIR};
pub use inode::{InodeStat, InodeTree, Ino, NAME_MAX_LEN, PNAME_MAX_LEN, S_IFMT, S_IFREG};

pub mod inode;

/// One directory entry in framework order.
///
/// C: `fs_getdents` (file.c:195-295) emits `.` (pos 0), `..` (pos 1, self
/// for root), then non-indexed children in tree order, skipping deleted
/// ones (file.c:236-262; the indexed branch is vacuous — every devman node
/// is NO_INDEX). Wire encoding (`fsdriver_dentry_*`) is transport business;
/// this struct carries the framework's traversal contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirent {
    pub name: String,
    pub ino: Ino,
    pub is_dir: bool,
}

/// A classified incoming request.
///
/// The production classifier (raw IPC → `Request`) needs the VFS-side wire
/// layout (`fsdriver_data`, REQ_* field macros) and lands with the IPC
/// transport (`minix-sys` is still `todo!()`); until then `run` is driven
/// by test transports. The classification itself is transport-independent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Mount { is_root: bool },
    Unmount,
    Lookup { dir: Ino, name: String },
    Read { ino: Ino, len: usize, pos: u64 },
    Readdir { dir: Ino, start: u64 },
}

/// One item off the wire, already classified: a VFS-side FS request, or a
/// decoded DEVMAN message from `source` (`None` = Ignored type, 05 §2.6 —
/// the loop answers with a single no-op). One enum, one loop, one dispatch
/// truth (DM-P1-2; C's `fsdriver_task` table is the same single entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Fs(Request),
    Devman {
        source: Endpoint,
        msg: Option<crate::ipc::DevmanMsg>,
    },
}

/// The matching reply for each [`Request`] variant, in order. Errors ride
/// in the payload — the transport maps them onto the fsdriver reply's
/// status word (C replies carry `RES_*` the same way); there is no
/// sentinel encoding and no silent success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Mounted(Result<Ino, Errno>),
    Unmounted,
    Found(Result<Ino, Errno>),
    Data(Result<Vec<u8>, Errno>),
    Entries(Result<Vec<Dirent>, Errno>),
}

/// Message source/sink for the server loop (`Server::run`).
///
/// Dependency injection (no globals): production uses the kernel IPC
/// transport once `minix-sys` implements it; tests use [`VecTransport`].
/// The trait is a dumb pipe — the *server* stamps replies
/// (`apply_reply_with_id`) and builds forward messages; the transport
/// only moves bytes.
pub trait Transport {
    /// Next classified request, or `None` when the loop should stop.
    fn next(&mut self) -> Option<Incoming>;
    /// Deliver the reply for the last FS request.
    fn reply(&mut self, reply: Reply);
    /// Async send of a stamped DEVMAN reply (C `ipc_send` — never sendrec).
    fn send(&mut self, dest: Endpoint, msg: &Message);
    /// Synchronous round trip with a device owner (C `ipc_sendrec`,
    /// bind.c:32/80); the answered message carries the driver's RESULT.
    fn sendrec(&mut self, owner: Endpoint, msg: &mut Message) -> Result<(), Errno>;
}

/// In-memory transport for tests: replays a script, records replies,
/// sends, and sendrecs (with an optional scripted sendrec answer).
#[cfg(test)]
pub struct VecTransport {
    pub script: alloc::collections::VecDeque<Incoming>,
    pub replies: Vec<Reply>,
    pub sent: Vec<(Endpoint, Message)>,
    /// Per-`sendrec` outcomes, popped front-first; `Err` models transport
    /// failure, `Ok(answer)` replays a driver reply with the given RESULT.
    pub sendrec_script: alloc::collections::VecDeque<Result<i32, Errno>>,
    pub sendrecs: Vec<(Endpoint, Message)>,
}

#[cfg(test)]
impl VecTransport {
    pub fn new(script: Vec<Incoming>) -> Self {
        VecTransport {
            script: script.into_iter().collect(),
            replies: Vec::new(),
            sent: Vec::new(),
            sendrec_script: alloc::collections::VecDeque::new(),
            sendrecs: Vec::new(),
        }
    }
}

#[cfg(test)]
impl Transport for VecTransport {
    fn next(&mut self) -> Option<Incoming> {
        self.script.pop_front()
    }

    fn reply(&mut self, reply: Reply) {
        self.replies.push(reply);
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) {
        self.sent.push((dest, *msg));
    }

    fn sendrec(&mut self, owner: Endpoint, msg: &mut Message) -> Result<(), Errno> {
        let answer = self.sendrec_script.pop_front().unwrap_or(Ok(0));
        self.sendrecs.push((owner, *msg));
        match answer {
            Ok(result) => {
                // Union field assignment through a place expression is
                // safe Rust (only reads are unsafe) — and it must go
                // through the place: `unsafe { msg.m_u.m_m4 }` would copy
                // the union out and write to the copy.
                msg.m_u.m_m4.m4l1 = result as i64;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

/// The VTreeFS server: inode tree + devman hooks + I/O buffer budget.
///
/// C: the six `run_vtreefs` globals (`vtreefs_hooks`, pool size, root stat,
/// buffer size — vtreefs.c:97-102) as one owned value; the two zero args
/// (`inode_extra`, `nr_indexed_entries`) stay omitted (01 §3.4).
pub struct VTreeFs {
    tree: InodeTree,
    hooks: FsHooks,
    buf_size: usize,
    io_buf: Vec<u8>,
}

impl VTreeFs {
    /// C: `run_vtreefs` init sequence — `init_inodes` + `init_extra` +
    /// `init_buf`, failures `panic` in C (vtreefs.c:16-33) → `Err(ENOMEM)`
    /// here ([ARCH:A-7], same evolution as 01's `SefHooks`).
    /// `init_extra` with size 0 is a no-op (devman passes 0, main.c:89).
    pub fn new(config: &ServerConfig, hooks: FsHooks) -> Result<Self, Errno> {
        let tree = InodeTree::new(config.nr_inodes, config.root_stat.into())?;
        let mut io_buf = Vec::new();
        io_buf
            .try_reserve(config.buf_size)
            .map_err(|_| Errno::ENOMEM)?;
        Ok(VTreeFs {
            tree,
            hooks,
            buf_size: config.buf_size,
            io_buf,
        })
    }

    /// Tree access for device management (04 owns the semantics; the
    /// framework owns the storage).
    pub fn tree(&self) -> &InodeTree {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut InodeTree {
        &mut self.tree
    }

    /// C: `fs_mount` (mount.c:10-38) — refuse root mounts (`EINVAL`),
    /// ref the root, then call `init_hook` if registered.
    /// (The per-mount call is C-faithful; devman's once-guard lives in
    /// 01's `FirstGuard`, not here.)
    pub fn mount(&mut self, is_root: bool) -> Result<Ino, Errno> {
        if is_root {
            return Err(Errno::EINVAL);
        }
        let root = self.tree.root();
        // C: `ref_inode(root)` (mount.c:21) — the mount holds one ref.
        // The root starts at 0 refs; a failed mount must not leak it,
        // so refcount juggling stays inside `reference` (infallible here:
        // root always exists).
        let _ = self.tree.reference(root);
        // C: `if (vtreefs_hooks->init_hook != NULL) vtreefs_hooks->init_hook()`
        // (mount.c:24-25) — the library calls back into the service to let
        // it build its tree. devman-rs has no callback: vtreefs is inlined
        // into the devman crate (A-1), so the service-side init is a
        // direct guarded call by `Server` on mount success (DM-P1-3;
        // `Server::ensure_devices`, whose `Option<DeviceTree>` is C's
        // `static int first` value-typed).
        Ok(root)
    }

    /// C: `fs_unmount` (mount.c:44-58) — drop the mount's root ref.
    /// `cleanup_hook` is intentionally absent: devman never registers one
    /// (main.c:78-80 sets exactly 3 hooks), so C's `if != NULL` is dead
    /// for devman — no `Option` slot is modeled for it.
    pub fn unmount(&mut self) {
        let root = self.tree.root();
        let _ = self.tree.release(root);
    }

    /// C: `fs_lookup` shape (path.c) — resolve one name level.
    /// (`ENOENT` for absent; deeper path walking is VFS business.)
    pub fn lookup(&self, dir: Ino, name: &str) -> Result<Ino, Errno> {
        self.tree.lookup(dir, name)
    }

    /// C: `fs_stat` shape (stadir.c:9) — node attributes by number.
    /// (`struct stat` wire encoding is transport business, like dirents.)
    pub fn stat(&self, ino: Ino) -> Result<InodeStat, Errno> {
        self.tree.stat(ino).ok_or(Errno::EINVAL)
    }

    /// C: `fs_read` (file.c:46-103), faithfully:
    /// unknown inode → `EINVAL`; non-regular → `EINVAL`; deleted node or
    /// no read hook → empty (EOF, file.c:62-64); then the chunk loop —
    /// `chunk = min(remaining, bufsize)`, hook fills, `len > 0` appends,
    /// error-after-partial returns the partial result (file.c:88-94),
    /// short chunk ends the loop (file.c:97-98).
    ///
    /// One memory-safety hardening vs C: a hook returning `len > chunk`
    /// would over-read C's buffer; here it is `EIO` (documented, 02 §4.3).
    /// `pos` is `u64` (`off_t` is 64-bit); the hook takes `i64` like C.
    pub fn read(&mut self, ino: Ino, len: usize, pos: u64) -> Result<Vec<u8>, Errno> {
        let node = self.tree.find(ino).ok_or(Errno::EINVAL)?;
        if !is_reg(node.mode()) {
            return Err(Errno::EINVAL);
        }
        if node.deleted() {
            return Ok(Vec::new());
        }
        let hook: ReadHookFn = match self.hooks.read_hook {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        let cbdata = node.cbdata();
        // C reuses one static `buf`; reuse `io_buf` the same way.
        self.io_buf.clear();
        self.io_buf.resize(self.buf_size, 0);
        let mut out = Vec::new();
        let mut off = 0usize;
        let mut cur_pos = pos;
        while off < len {
            let chunk = (len - off).min(self.buf_size);
            let got = hook(
                &mut self.io_buf[..chunk],
                chunk,
                cur_pos as i64,
                cbdata,
            );
            if got < 0 {
                // C file.c:88-94: error after partial output returns
                // the partial result; error first returns the error.
                // Negative hook returns are raw errnos in C's negative
                // kernel convention; minix-rs `Errno` uses the positive
                // user-space convention (`Errno::to_i32`), so negate.
                if out.is_empty() {
                    return Err(Errno::from_i32(-got as i32));
                }
                return Ok(out);
            }
            let got = got as usize;
            if got > chunk {
                return Err(Errno::EIO);
            }
            out.extend_from_slice(&self.io_buf[..got]);
            off += got;
            cur_pos += got as u64;
            if got < self.buf_size {
                break;
            }
        }
        Ok(out)
    }

    /// C: `fs_getdents` traversal order (file.c:195-295) without the
    /// `fsdriver_dentry_*` wire encoding: `.` (self), `..` (parent, self
    /// for root), then live children in tree order. `start` is the C
    /// `*posp` cursor. The getdents-hook refresh call is devman-absent
    /// (hook never registered — main.c:78-80) so no refresh slot exists.
    pub fn readdir(&self, dir: Ino, start: u64) -> Result<Vec<Dirent>, Errno> {
        // C checks only resolvability (file.c:202-204); a file lists just
        // "." and ".." (no S_ISDIR gate in fs_getdents), and even a
        // deleted dir is traversed — only unknown numbers are EINVAL.
        if self.tree.find(dir).is_none() {
            return Err(Errno::EINVAL);
        }
        let mut out = Vec::new();
        let mut pos = 0u64;
        let mut push = |name: String, ino: Ino, is_dir: bool| {
            if pos >= start {
                out.push(Dirent { name, ino, is_dir });
            }
            pos += 1;
        };
        push(String::from("."), dir, true);
        let parent = self.tree.parent_of(dir).unwrap_or(dir);
        push(String::from(".."), parent, true);
        let mut child = self.tree.first_child(dir);
        while let Some(c) = child {
            if let Some(n) = self.tree.find(c) {
                let nm = String::from(n.name());
                let is_d = is_dir(n.mode());
                push(nm, c, is_d);
            }
            child = self.tree.next_child(c);
        }
        let _ = pos;
        Ok(out)
    }

    /// Mutating ops devman never wires (write/trunc/mknod/…) — explicit
    /// `ENOSYS`, mirroring the Redox `Scheme` default-method idea from
    /// 01 §3.1. (C's own asymmetric defaults — read→EOF, write→EACCES —
    /// only apply where devman actually registered hooks.)
    pub fn unsupported(&self) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

// C's dispatch half: `fsdriver_task(&vtreefs_table)` (table.c:6-24, 17
// slots) is one loop over one table — the single entry for every message.
// [DM-P1-2: devman-rs keeps that shape — the loop lives on `Server::run`
// so FS requests and DEVMAN messages share one dispatch truth; two loops
// would be the dual-dispatch problem again. `VTreeFs` keeps the per-op
// semantics (mount/lookup/read/readdir) that the loop dispatches over;
// the production transport is still the only missing piece for `main`
// (P1-6, narrowed to transport wiring).]

// `inode::Inode` exposes its own accessors; these two predicates keep the
// mode checks at the call sites expressive.
fn is_reg(mode: u32) -> bool {
    mode & S_IFMT == S_IFREG
}

fn is_dir(mode: u32) -> bool {
    mode & S_IFMT == S_IFDIR
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{S_IFDIR, S_IRALL};
    use crate::RootStat;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn cfg(buf: usize) -> ServerConfig {
        ServerConfig {
            nr_inodes: 16,
            root_stat: RootStat::devman_root(),
            buf_size: buf,
        }
    }

    fn dir_stat() -> InodeStat {
        InodeStat {
            mode: S_IFDIR | S_IRALL,
            uid: 0,
            gid: 0,
            size: 0,
            dev: 0,
        }
    }

    fn file_stat() -> InodeStat {
        InodeStat {
            mode: S_IFREG | S_IRALL,
            uid: 0,
            gid: 0,
            size: 0,
            dev: 0,
        }
    }

    fn plain_hooks() -> FsHooks {
        FsHooks::empty()
    }

    /// Fill the whole chunk (models a well-behaved `read_fn`, 06).
    fn hook_full(buf: &mut [u8], len: usize, _off: i64, _cb: usize) -> i64 {
        for b in &mut buf[..len] {
            *b = b'v';
        }
        len as i64
    }

    /// Short chunk (forces the `len < bufsize → break` path, file.c:97).
    fn hook_short2(buf: &mut [u8], len: usize, _off: i64, _cb: usize) -> i64 {
        let take = len.min(2);
        for b in &mut buf[..take] {
            *b = b's';
        }
        take as i64
    }

    /// Immediate error (C: negative errno return, file.c:88-94).
    fn hook_err5(_buf: &mut [u8], _len: usize, _off: i64, _cb: usize) -> i64 {
        -5 // EIO
    }

    /// Over-long return (would over-read C's static buf; Rust: EIO).
    fn hook_over(_buf: &mut [u8], _len: usize, _off: i64, _cb: usize) -> i64 {
        99
    }

    static PARTIAL_N: AtomicUsize = AtomicUsize::new(0);

    /// First call yields 3 bytes, then errors (partial-result rule).
    fn hook_partial(buf: &mut [u8], len: usize, _off: i64, _cb: usize) -> i64 {
        if PARTIAL_N.fetch_add(1, Ordering::SeqCst) == 0 {
            let take = len.min(3);
            for b in &mut buf[..take] {
                *b = b'p';
            }
            take as i64
        } else {
            -5
        }
    }

    fn hooks_with_read(f: ReadHookFn) -> FsHooks {
        FsHooks {
            read_hook: Some(f),
        }
    }

    #[test]
    fn mount_rejects_root() {
        // C: REQ_ISROOT → EINVAL (mount.c:16-17).
        let mut fs = VTreeFs::new(&cfg(64), plain_hooks()).unwrap();
        assert_eq!(fs.mount(true), Err(Errno::EINVAL));
    }

    #[test]
    fn mount_succeeds_twice_and_unmounts() {
        // C: fs_mount refs the root per mount (mount.c:21) and runs
        // unconditionally (both mounts Ok). devman-rs fires no init hook
        // here — service-side init is `Server::ensure_devices`'s guarded
        // direct call (DM-P1-3 collapse note on `mount`).
        let hooks = FsHooks {
            read_hook: None,
        };
        let mut fs = VTreeFs::new(&cfg(64), hooks).unwrap();
        assert_eq!(fs.mount(false).unwrap(), Ino(1));
        assert_eq!(fs.mount(false).unwrap(), Ino(1));
        fs.unmount();
    }

    #[test]
    fn read_bad_ino_and_dir_are_einval() {
        // C: find_inode NULL → EINVAL; !S_ISREG → EINVAL (file.c:55-60).
        let mut fs = VTreeFs::new(&cfg(64), hooks_with_read(hook_full)).unwrap();
        assert_eq!(fs.read(Ino(99), 8, 0), Err(Errno::EINVAL));
        assert_eq!(fs.read(Ino(1), 8, 0), Err(Errno::EINVAL));
    }

    #[test]
    fn read_no_hook_and_deleted_are_eof() {
        // C: deleted node or NULL hook → 0 / EOF (file.c:62-64).
        let mut fs = VTreeFs::new(&cfg(64), plain_hooks()).unwrap();
        let f = fs.tree_mut().add(Ino(1), "e", file_stat(), 0).unwrap();
        assert_eq!(fs.read(f, 8, 0).unwrap(), Vec::<u8>::new());
        let mut fs2 = VTreeFs::new(&cfg(64), hooks_with_read(hook_full)).unwrap();
        let g = fs2.tree_mut().add(Ino(1), "g", file_stat(), 0).unwrap();
        fs2.tree_mut().reference(g).unwrap();
        fs2.tree_mut().delete(g).unwrap();
        assert_eq!(fs2.read(g, 8, 0).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn read_full_single_chunk() {
        let mut fs = VTreeFs::new(&cfg(64), hooks_with_read(hook_full)).unwrap();
        let f = fs.tree_mut().add(Ino(1), "f", file_stat(), 0).unwrap();
        assert_eq!(fs.read(f, 8, 0).unwrap(), alloc::vec![b'v'; 8]);
    }

    #[test]
    fn read_multichunk_until_short() {
        // buf 4, request 10: hook_short2 yields 2 per call → the
        // `len < bufsize` break fires after the first chunk (file.c:97).
        let mut fs = VTreeFs::new(&cfg(4), hooks_with_read(hook_short2)).unwrap();
        let f = fs.tree_mut().add(Ino(1), "f", file_stat(), 0).unwrap();
        assert_eq!(fs.read(f, 10, 0).unwrap(), alloc::vec![b's'; 2]);
    }

    #[test]
    fn read_error_rules() {
        // Immediate error → Err (C file.c:91-94, off == 0 branch).
        let mut fs = VTreeFs::new(&cfg(64), hooks_with_read(hook_err5)).unwrap();
        let f = fs.tree_mut().add(Ino(1), "f", file_stat(), 0).unwrap();
        assert_eq!(fs.read(f, 8, 0), Err(Errno::EIO));
        // Error after partial output → partial result (off > 0 branch).
        PARTIAL_N.store(0, Ordering::SeqCst);
        let mut fs2 = VTreeFs::new(&cfg(64), hooks_with_read(hook_partial)).unwrap();
        let g = fs2.tree_mut().add(Ino(1), "g", file_stat(), 0).unwrap();
        assert_eq!(fs2.read(g, 8, 0).unwrap(), alloc::vec![b'p'; 3]);
    }

    #[test]
    fn read_overlong_hook_result_is_eio() {
        // Hardening vs C (which would over-read its static buf).
        let mut fs = VTreeFs::new(&cfg(4), hooks_with_read(hook_over)).unwrap();
        let f = fs.tree_mut().add(Ino(1), "f", file_stat(), 0).unwrap();
        assert_eq!(fs.read(f, 4, 0), Err(Errno::EIO));
    }

    #[test]
    fn readdir_dot_dot_children() {
        // C order: ".", ".." (self for root), then live children
        // in tree order, deleted skipped (file.c:226-280).
        let mut fs = VTreeFs::new(&cfg(64), plain_hooks()).unwrap();
        let d = fs.tree_mut().add(Ino(1), "devices", dir_stat(), 0).unwrap();
        let e = fs.tree_mut().add(Ino(1), "events", file_stat(), 0).unwrap();
        fs.tree_mut().add(d, "dev_type", file_stat(), 0).unwrap();
        let names: Vec<String> = fs
            .readdir(Ino(1), 0)
            .unwrap()
            .into_iter()
            .map(|de| de.name)
            .collect();
        assert_eq!(names, alloc::vec![".", "..", "devices", "events"]);
        // C has no S_ISDIR gate in fs_getdents: a live file lists "."
        // and "..".
        let only = fs.readdir(e, 0).unwrap();
        assert_eq!(only.len(), 2);
        // Cursor + deleted-skip.
        fs.tree_mut().delete(e).unwrap();
        let names2: Vec<String> = fs
            .readdir(Ino(1), 2)
            .unwrap()
            .into_iter()
            .map(|de| de.name)
            .collect();
        assert_eq!(names2, alloc::vec!["devices"]);
        // ".." of a child is the parent.
        let up = fs.readdir(d, 1).unwrap();
        assert_eq!(up[0].name, "..");
        assert_eq!(up[0].ino, Ino(1));
        // A reaped slot no longer resolves (hardening vs C: `find_inode`
        // returns even freed slots with stale content; Rust refuses).
        assert_eq!(fs.readdir(e, 0), Err(Errno::EINVAL));
        // stat shape mirrors the stored attrs.
        assert_eq!(fs.stat(Ino(1)).unwrap().mode, dir_stat().mode);
        assert_eq!(fs.stat(Ino(99)), Err(Errno::EINVAL));
    }

    // [DM-P1-2: the former `other_forwards_to_message_hook` and
    // `run_dispatches_script` tests moved with the loop itself —
    // `Server::run` in server.rs now owns loop-level coverage, FS and
    // DEVMAN alike.]

    #[test]
    fn unsupported_is_enosys() {
        // Unwired mutating slots fail closed.
        let fs = VTreeFs::new(&cfg(64), plain_hooks()).unwrap();
        assert_eq!(fs.unsupported(), Err(Errno::ENOSYS));
    }
}

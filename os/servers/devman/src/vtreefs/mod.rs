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

use crate::buf::Buf;
use crate::event_queue::EventQueue;
use crate::hooks::{ServerConfig, S_IFDIR};
pub use inode::{InodeContent, InodeStat, InodeTree, Ino, NAME_MAX_LEN, PNAME_MAX_LEN, S_IFMT, S_IFREG};
use inode::is_dir;

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

/// The VTreeFS server: inode tree + reused I/O buffer.
///
/// C: the six `run_vtreefs` globals (pool size, root stat, buffer size,
/// hooks table — vtreefs.c:97-102) as one owned value; the two zero args
/// (`inode_extra`, `nr_indexed_entries`) stay omitted (01 §3.4). The
/// hooks table dissolved in DM-P1-2/P1-5: dispatch lives on `Server`,
/// file content lives on the inodes — the last hook (`read_hook`) had
/// nothing left to select that the node's own [`InodeContent`] doesn't
/// already know.
pub struct VTreeFs {
    tree: InodeTree,
    buf: Buf,
    buf_size: usize,
}

impl VTreeFs {
    /// C: `run_vtreefs` init sequence — `init_inodes` + `init_extra` +
    /// `init_buf`, failures `panic` in C (vtreefs.c:16-33) → `Err(ENOMEM)`
    /// here ([ARCH:A-7], same evolution as 01's `SefHooks`).
    /// `init_extra` with size 0 is a no-op (devman passes 0, main.c:89).
    pub fn new(config: &ServerConfig) -> Result<Self, Errno> {
        let tree = InodeTree::new(config.nr_inodes, config.root_stat.into())?;
        let buf = Buf::new()?;
        Ok(VTreeFs {
            tree,
            buf,
            buf_size: config.buf_size,
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

    /// C: `fs_read` (file.c:46-103): unknown inode → `EINVAL`; non-regular
    /// → `EINVAL`; deleted node → empty (EOF, file.c:62-64); then the
    /// chunk loop — `chunk = min(remaining, bufsize)`, short chunk ends
    /// the loop (file.c:97-98). The "hook" being chunk-read is the node's
    /// own [`InodeContent`] (DM-P1-5): C's `read_fn` cookie jump
    /// (main.c:64-66 → `files::dispatch_read`) collapsed into a match.
    /// Content readers are infallible and bounded (`Buf` caps at
    /// `BUF_SIZE - 1`), so the C error paths (negative hook return /
    /// over-long return → EIO hardening) have no residue here. `pos` is
    /// `u64` (`off_t` is 64-bit).
    pub fn read(&mut self, ino: Ino, len: usize, pos: u64) -> Result<Vec<u8>, Errno> {
        let node = self.tree.find(ino).ok_or(Errno::EINVAL)?;
        if !is_reg(node.mode()) {
            return Err(Errno::EINVAL);
        }
        if node.deleted() {
            return Ok(Vec::new());
        }
        match self.tree.content(ino) {
            Some(InodeContent::Static(_)) | Some(InodeContent::Events(_)) => {}
            // Directories are rejected by the is_reg gate above; a freed
            // slot fails the `content` lookup. Either way: EOF.
            _ => return Ok(Vec::new()),
        }
        let mut out = Vec::new();
        let mut off = 0usize;
        let mut cur_pos = pos;
        while off < len {
            let chunk = (len - off).min(self.buf_size);
            let got = self.read_chunk(ino, chunk, cur_pos);
            out.extend_from_slice(&got);
            off += got.len();
            cur_pos += got.len() as u64;
            if got.len() < self.buf_size {
                break;
            }
        }
        Ok(out)
    }

    /// One chunk out of the node's own content, through the reused `buf`
    /// (C reuses its statics; DM-P1-5/P3-3 — no per-read allocation).
    fn read_chunk(&mut self, ino: Ino, chunk: usize, cur_pos: u64) -> Vec<u8> {
        match self.tree.content_mut(ino) {
            Some(InodeContent::Static(text)) => {
                EventQueue::read_static(&mut self.buf, text, chunk, cur_pos as usize)
            }
            Some(InodeContent::Events(q)) => q.read_oldest(&mut self.buf, chunk, cur_pos as usize),
            _ => Vec::new(),
        }
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
}

// C's dispatch half: `fsdriver_task(&vtreefs_table)` (table.c:6-24, 17
// slots) is one loop over one table — the single entry for every message.
// [DM-P1-2: devman-rs keeps that shape — the loop lives on `Server::run`
// so FS requests and DEVMAN messages share one dispatch truth; two loops
// would be the dual-dispatch problem again. `VTreeFs` keeps the per-op
// semantics (mount/lookup/read/readdir) that the loop dispatches over;
// the production transport is still the only missing piece for `main`
// (P1-6, narrowed to transport wiring).]

// `Inode` exposes its own accessors; this predicate keeps the mode check
// at the call site expressive (the dir twin lives in `inode`, shared).
fn is_reg(mode: u32) -> bool {
    mode & S_IFMT == S_IFREG
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{S_IFDIR, S_IRALL};
    use crate::RootStat;
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

    /// A file node with static text content (the readable-file shape).
    fn content_file(t: &mut InodeTree, name: &str, text: &str) -> Ino {
        t.add(Ino(1), name, file_stat(), InodeContent::Static(String::from(text)))
            .unwrap()
    }

    #[test]
    fn mount_rejects_root() {
        // C: REQ_ISROOT → EINVAL (mount.c:16-17).
        let mut fs = VTreeFs::new(&cfg(64)).unwrap();
        assert_eq!(fs.mount(true), Err(Errno::EINVAL));
    }

    #[test]
    fn mount_succeeds_twice_and_unmounts() {
        // C: fs_mount refs the root per mount (mount.c:21) and runs
        // unconditionally (both mounts Ok). devman-rs fires no init hook
        // here — service-side init is `Server::ensure_devices`'s guarded
        // direct call (DM-P1-3 collapse note on `mount`).
        let mut fs = VTreeFs::new(&cfg(64)).unwrap();
        assert_eq!(fs.mount(false).unwrap(), Ino(1));
        assert_eq!(fs.mount(false).unwrap(), Ino(1));
        fs.unmount();
    }

    #[test]
    fn read_bad_ino_and_dir_are_einval() {
        // C: find_inode NULL → EINVAL; !S_ISREG → EINVAL (file.c:55-60).
        let mut fs = VTreeFs::new(&cfg(64)).unwrap();
        assert_eq!(fs.read(Ino(99), 8, 0), Err(Errno::EINVAL));
        assert_eq!(fs.read(Ino(1), 8, 0), Err(Errno::EINVAL));
    }

    #[test]
    fn read_deleted_is_eof() {
        // C: deleted node → 0 / EOF (file.c:62-64).
        let mut fs = VTreeFs::new(&cfg(64)).unwrap();
        let g = content_file(&mut fs.tree_mut(), "g", "data");
        fs.tree_mut().reference(g).unwrap();
        fs.tree_mut().delete(g).unwrap();
        assert_eq!(fs.read(g, 8, 0).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn read_static_content_renders_text_plus_newline() {
        // Static content: `text + '\n'` through the skip/cap funnel (06).
        let mut fs = VTreeFs::new(&cfg(64)).unwrap();
        let f = content_file(&mut fs.tree_mut(), "f", "hi");
        assert_eq!(fs.read(f, 8, 0).unwrap(), b"hi\n".to_vec());
        // Offset skip: the first produced byte is eaten.
        assert_eq!(fs.read(f, 8, 1).unwrap(), b"i\n".to_vec());
    }

    #[test]
    fn read_events_content_drains_two_reads() {
        // Events content: data read, then EOF read consumes (06 §2.4).
        let mut fs = VTreeFs::new(&cfg(64)).unwrap();
        let f = fs
            .tree_mut()
            .add(
                Ino(1),
                "events",
                file_stat(),
                InodeContent::Events(crate::event_queue::EventQueue::new()),
            )
            .unwrap();
        if let Some(InodeContent::Events(q)) = fs.tree_mut().content_mut(f) {
            q.push(crate::structs::Event::new("ADD ./devices/a/ 0x00000001").unwrap())
                .unwrap();
        }
        assert_eq!(
            fs.read(f, 128, 0).unwrap(),
            b"ADD ./devices/a/ 0x00000001".to_vec()
        );
        // Non-consuming read (r > 0): still there.
        assert_eq!(fs.read(f, 128, 0).unwrap().len(), 27);
        // EOF read consumes.
        assert_eq!(fs.read(f, 128, 27).unwrap(), Vec::<u8>::new());
        assert_eq!(fs.read(f, 128, 0).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn read_multichunk_until_short() {
        // buf 4, Static longer than one chunk: the `len < bufsize` break
        // fires only after a short chunk (file.c:97) — here after the
        // second chunk carries the tail.
        let mut fs = VTreeFs::new(&cfg(4)).unwrap();
        let f = content_file(&mut fs.tree_mut(), "f", "ssssss");
        assert_eq!(fs.read(f, 10, 0).unwrap(), b"ssssss\n".to_vec());
    }

    #[test]
    fn readdir_dot_dot_children() {
        // C order: ".", ".." (self for root), then live children
        // in tree order, deleted skipped (file.c:226-280).
        let mut fs = VTreeFs::new(&cfg(64)).unwrap();
        let d = fs.tree_mut().add(Ino(1), "devices", dir_stat(), InodeContent::Dir).unwrap();
        let e = fs.tree_mut().add(Ino(1), "events", file_stat(), InodeContent::Static(String::new())).unwrap();
        fs.tree_mut().add(d, "dev_type", file_stat(), InodeContent::Static(String::new())).unwrap();
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
}

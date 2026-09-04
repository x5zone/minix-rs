//! Startup hooks and server configuration (doc 01-devm-init-main).
//!
//! C: `minix3/minix/servers/devman/main.c` (93 lines) +
//! `minix3/minix/lib/libvtreefs/vtreefs.c:sef_local_startup,run_vtreefs`.
//!
//! The devman `main` does three things: register hooks, describe the root
//! inode, and hand both to `run_vtreefs`. This module models exactly those
//! three things with Rust types; the VTreeFS internals live in 02's module.

use minix_types::Errno;

// ── Constants (C sources annotated; 99-devm-global-concepts is upstream) ──

/// C: `main.c:89` — inode pool size passed to `run_vtreefs`.
pub const DEVMAN_NR_INODES: u32 = 1024;
/// C: `devman.h:39` — I/O buffer size (`4096 + 1` for the NUL terminator).
pub const BUF_SIZE: usize = 4097;
/// POSIX `S_IFDIR` (directory file type bits).
pub const S_IFDIR: u32 = 0o040_000;
/// POSIX read bits `S_IRUSR|S_IRGRP|S_IROTH` (`0444`).
pub const S_IRALL: u32 = 0o444;
/// C: Minix `const.h:132` (`#define NO_DEV ((dev_t) 0)`) — virtual FS
/// backends no block device. NOTE: value is 0, not -1 (drivers use a
/// different `NO_DEVICE -1`; do not confuse the two).
pub const NO_DEV: i32 = 0;

// ── Hook function types ──

/// Context handed to the init hook (mount-triggered, see `FirstGuard`).
pub struct InitCtx {
    init_requested: bool,
}

impl InitCtx {
    pub fn new() -> Self {
        InitCtx {
            init_requested: false,
        }
    }

    /// Mark that device-tree init was requested (04 implements the tree).
    pub fn request_init(&mut self) {
        self.init_requested = true;
    }

    pub fn init_requested(&self) -> bool {
        self.init_requested
    }
}

impl Default for InitCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// C: `read_hook` signature (`vtreefs.h:29-30`), minus the unused inode ptr.
/// Returns bytes filled, 0 on EOF, negative errno on error.
pub type ReadHookFn =
    fn(buf: &mut [u8], len: usize, offset: i64, cbdata_addr: usize) -> i64;

/// C: `message_hook` signature (`vtreefs.h:43`).
pub type MessageHookFn = fn(m_type: i32, m_source: i32, _ipc_status: i32);

// ── FsHooks ──

/// C: `struct fs_hooks` (`vtreefs.h:24-44`, 13 slots).
/// Only the 3 slots devman uses are modeled here; the remaining 10 slots
/// (lookup/getdents/write/...) belong to 02's framework module.
pub struct FsHooks {
    /// C: `main.c:78` `hooks.init_hook = init_hook`.
    pub init_hook: Option<fn(&mut InitCtx)>,
    /// C: `main.c:79` `hooks.read_hook = read_hook`.
    pub read_hook: Option<ReadHookFn>,
    /// C: `main.c:80` `hooks.message_hook = message_hook`.
    pub message_hook: Option<MessageHookFn>,
}

impl FsHooks {
    /// C: `main.c:70-80` — zeroed table with the three devman hooks filled.
    pub fn devman_default() -> Self {
        FsHooks {
            init_hook: Some(devman_init_hook),
            read_hook: Some(devman_read_hook),
            message_hook: Some(devman_message_hook),
        }
    }

    /// All-`None`: missing hook = safe default (no-op / EOF / ignore).
    /// Mirrors the Redox `Scheme` "unimplemented = ENOSYS" idea.
    pub fn empty() -> Self {
        FsHooks {
            init_hook: None,
            read_hook: None,
            message_hook: None,
        }
    }

    /// C: `mount.c:24-25` `if (vtreefs_hooks->init_hook != NULL)`.
    pub fn fire_init(&self, ctx: &mut InitCtx) {
        if let Some(f) = self.init_hook {
            f(ctx);
        }
    }

    /// Default when no read hook: EOF (C: `file.c` returns 0).
    pub fn fire_read(
        &self,
        buf: &mut [u8],
        len: usize,
        offset: i64,
        cbdata_addr: usize,
    ) -> i64 {
        match self.read_hook {
            Some(f) => f(buf, len, offset, cbdata_addr),
            None => 0,
        }
    }

    /// Default when no message hook: ignore (fail-closed; 05 refines this).
    pub fn fire_message(&self, m_type: i32, m_source: i32, ipc_status: i32) {
        if let Some(f) = self.message_hook {
            f(m_type, m_source, ipc_status);
        }
    }
}

// ── Default hook bodies (wiring only; business logic in 04/05/06) ──

/// C: `main.c:36-43` — guarded init entry; tree building itself is 04.
pub fn devman_init_hook(ctx: &mut InitCtx) {
    ctx.request_init();
}

/// C: `main.c:60-67` — dispatch via per-inode `read_fn`.
/// 06 wires the real dispatch: the cookie selects the file in the
/// process table (`files::dispatch_read` — per-file strategy, same shape
/// as C's `read_fn` indirection).
pub fn devman_read_hook(
    buf: &mut [u8],
    len: usize,
    offset: i64,
    cbdata_addr: usize,
) -> i64 {
    crate::files::dispatch_read(buf, len, offset, cbdata_addr)
}

/// C: `main.c:46-58` — message entry.
/// [ARCH:A-3] single-handler dispatch is live: `dispatch(m_type)` routes
/// to exactly one arm (05). Handler bodies land in 07–09; until then each
/// arm is a fail-closed placeholder pointing at its owner doc.
pub fn devman_message_hook(m_type: i32, _m_source: i32, _ipc_status: i32) {
    match crate::ipc::dispatch(m_type) {
        // 07-devm-add-device.md owns this arm.
        crate::ipc::Handler::Add => {}
        // 08-devm-del-device.md owns this arm.
        crate::ipc::Handler::Del => {}
        // 09-devm-bind-unbind.md owns this arm (RS-only gate inside).
        crate::ipc::Handler::Bind => {}
        crate::ipc::Handler::Unbind => {}
        // 05 §2.6: unknown → run nothing, reply nothing.
        crate::ipc::Handler::Ignored => {}
    }
}

// ── RootStat ──

/// C: `struct inode_stat` (`vtreefs.h:16-22`) as an immutable value.
/// C: `main.c:82-86` fills a mutable global; Rust constructs it whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootStat {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: i64,
    pub dev: i32,
}

impl RootStat {
    /// C: `main.c:82-86` — `S_IFDIR|0444, uid/gid 0, size 0, NO_DEV`.
    pub const fn devman_root() -> Self {
        RootStat {
            mode: S_IFDIR | S_IRALL,
            uid: 0,
            gid: 0,
            size: 0,
            dev: NO_DEV,
        }
    }
}

// ── ServerConfig ──

/// Explicit parameter object replacing the six globals C needs at
/// `vtreefs.c:97-102`.
// [ARCH:A-1-相关] C must stage args in globals ("inability to pass
// parameters through SEF", vtreefs.c:93-96); Rust has no SEF signature
// constraint, so the same values travel as an explicit argument.
// Externally equivalent: identical values enter identical init order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerConfig {
    /// C: `main.c:89` `nr_inodes = 1024`.
    pub nr_inodes: u32,
    pub root_stat: RootStat,
    /// C: `devman.h:39` `BUF_SIZE 4097`.
    pub buf_size: usize,
}

impl ServerConfig {
    /// C: `main.c:89` `run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE)`.
    /// The two zero args (`inode_extra`, `nr_indexed_entries`) are omitted
    /// at the type level: devman never uses indexed slots (02 proves it).
    pub const fn devman_default(root: RootStat) -> Self {
        ServerConfig {
            nr_inodes: DEVMAN_NR_INODES,
            root_stat: root,
            buf_size: BUF_SIZE,
        }
    }
}

// ── FirstGuard ──

/// C: `main.c:37` `static int first` — run-once guard for `init_hook`.
/// Single-threaded event loop: plain `bool` suffices (no `AtomicBool`;
/// same argument as VM `global.rs` BSS state).
pub struct FirstGuard(bool);

impl FirstGuard {
    /// Fresh guard: the next `enter()` returns `true`.
    pub const fn new() -> Self {
        FirstGuard(true)
    }

    /// Returns `true` at most once; caller runs `devman_init_devices` (04).
    pub fn enter(&mut self) -> bool {
        if self.0 {
            self.0 = false;
            true
        } else {
            false
        }
    }
}

impl Default for FirstGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ── SEF lifecycle ──

/// C: `vtreefs.c:54-59` — the three SEF registrations + startup, in order.
/// `minix-sef` is currently a stub: this enum documents devman's required
/// shape (forward reference; do not invent `minix-sef` APIs here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SefLifecycle {
    InitFresh,
    InitRestartStateful,
    SignalTerm,
    Startup,
}

/// Minimal SEF contract devman needs (fresh init + signal).
/// [ARCH:A-7] C `panic("init_inodes failed")` becomes explicit
/// `Err(ENOMEM)`; the caller decides fail-fast.
pub trait SefHooks {
    fn init_server(&mut self) -> Result<(), Errno>;
    fn on_signal(&mut self, sig: i32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_guard_fires_once() {
        let mut g = FirstGuard::new();
        assert!(g.enter());
        assert!(!g.enter());
        assert!(!g.enter());
    }

    #[test]
    fn root_stat_matches_c_main() {
        // C: main.c:82-86. Literal values (not `NO_DEV` self-reference):
        // C: const.h:132 `#define NO_DEV ((dev_t) 0)` — value is 0.
        let r = RootStat::devman_root();
        assert_eq!(r.mode, S_IFDIR | 0o444);
        assert_eq!(r.uid, 0);
        assert_eq!(r.gid, 0);
        assert_eq!(r.size, 0);
        assert_eq!(r.dev, 0);
        assert_eq!(r.dev, NO_DEV);
    }

    #[test]
    fn server_config_defaults_match_c() {
        // C: main.c:89 (1024) + devman.h:39 (4097).
        let c = ServerConfig::devman_default(RootStat::devman_root());
        assert_eq!(c.nr_inodes, 1024);
        assert_eq!(c.buf_size, 4097);
    }

    #[test]
    fn hooks_none_is_safe_default() {
        let h = FsHooks::empty();
        let mut ctx = InitCtx::new();
        h.fire_init(&mut ctx);
        assert!(!ctx.init_requested());
        let mut buf = [0u8; 8];
        assert_eq!(h.fire_read(&mut buf, 8, 0, 0), 0);
        h.fire_message(1, 2, 3); // must not panic
    }

    #[test]
    fn init_hook_wires_to_first_guard() {
        // Guarded init: hook fires, but the tree build runs only once.
        let h = FsHooks::devman_default();
        let mut guard = FirstGuard::new();
        let mut builds = 0;
        for _ in 0..3 {
            let mut ctx = InitCtx::new();
            h.fire_init(&mut ctx);
            assert!(ctx.init_requested());
            if guard.enter() {
                builds += 1;
            }
        }
        assert_eq!(builds, 1);
    }

    /// Test double #1 (behavior: records lifecycle, always succeeds).
    /// C: `sef_local_startup` order Fresh → Startup (vtreefs.c:52-60).
    struct OkSef {
        sequence: alloc::vec::Vec<SefLifecycle>,
    }

    impl SefHooks for OkSef {
        fn init_server(&mut self) -> Result<(), Errno> {
            self.sequence.push(SefLifecycle::InitFresh);
            Ok(())
        }

        fn on_signal(&mut self, sig: i32) {
            // C: `got_signal` ignores everything but SIGTERM (vtreefs.c:38-46).
            if sig == 15 {
                self.sequence.push(SefLifecycle::SignalTerm);
            }
        }
    }

    /// Test double #2 (behavior: fresh init always fails).
    /// [ARCH:A-7] C `panic("init_inodes failed")` becomes explicit
    /// `Err(ENOMEM)`; the caller (not this trait) decides fail-fast.
    struct FailingSef;

    impl SefHooks for FailingSef {
        fn init_server(&mut self) -> Result<(), Errno> {
            Err(Errno::ENOMEM)
        }

        fn on_signal(&mut self, _sig: i32) {}
    }

    #[test]
    fn sef_ok_records_lifecycle_sequence() {
        let mut sef = OkSef {
            sequence: alloc::vec::Vec::new(),
        };
        assert_eq!(sef.init_server(), Ok(()));
        sef.on_signal(15);
        sef.on_signal(1); // non-TERM ignored, like C `got_signal`
        assert_eq!(
            sef.sequence,
            alloc::vec![SefLifecycle::InitFresh, SefLifecycle::SignalTerm]
        );
    }

    #[test]
    fn sef_failing_returns_enomem() {
        let mut sef = FailingSef;
        // [ARCH:A-7] allocation failure surfaces as ENOMEM, never panics.
        assert_eq!(sef.init_server(), Err(Errno::ENOMEM));
    }
}

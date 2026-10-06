//! Startup hooks and server configuration (doc 01-devm-init-main).
//!
//! C: `minix3/minix/servers/devman/main.c` (93 lines) +
//! `minix3/minix/lib/libvtreefs/vtreefs.c:sef_local_startup,run_vtreefs`.
//!
//! The devman `main` does three things: register hooks, describe the root
//! inode, and hand both to `run_vtreefs`. This module models exactly those
//! three things with Rust types; the VTreeFS internals live in 02's module.

use minix_types::Errno;
use crate::server::Server;

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

/// Minimal SEF contract devman needs: fresh init only. The signal half of
/// C's registration set (`vtreefs.c:59` registers `got_signal`) lives on
/// the transport, whose stop flag is what `fsdriver_terminate()` actually
/// clears (`fsdriver.c:68-74`) — the trait method this once carried was a
/// dead link (nothing called it) and was removed (PD-26 的删一半；C 语义
/// 由传输层忠实承接，见 `ipc::minix::MinixTransport`）。
/// [ARCH:A-7] C `panic("init_inodes failed")` becomes explicit
/// `Err(ENOMEM)`; the caller decides fail-fast.
pub trait SefHooks {
    fn init_server(&mut self) -> Result<(), Errno>;
}

/// Production SEF hooks (E-ISWIRE) — the callbacks C main.c:77-80 fills
/// and libvtreefs's `sef_local_startup` (vtreefs.c:52-62) registers:
/// `init_server` = `init_hook` (fresh init: rebuild the framework + tree
/// from the stored config via [`Server::new`]); restart is registered as
/// `SEF_CB_INIT_RESTART_STATEFUL` (vtreefs.c:58) — state survives the RS
/// image restore, so the restart callback body is empty and this hook is
/// only ever invoked for the fresh case. The signal registration
/// (`got_signal`, vtreefs.c:39-46/59) lands on the transport's stop latch
/// (`ipc::minix::MinixTransport::terminate`), not on this trait.
pub struct DevmanSef {
    config: ServerConfig,
    /// The framework + device-tree state (rebuilt by [`Self::init_server`]).
    pub server: Server,
}

impl DevmanSef {
    /// Fresh-boot assembly: config + first `Server` (C main.c:82-89).
    pub fn new(config: ServerConfig) -> Result<Self, Errno> {
        Ok(Self { server: Server::new(&config)?, config })
    }
}

impl SefHooks for DevmanSef {
    fn init_server(&mut self) -> Result<(), Errno> {
        self.server = Server::new(&self.config)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    /// Production impl (E-ISWIRE): fresh init rebuilds the Server from
    /// the config. The SIGTERM half lives on the transport latch
    /// (`ipc::minix::MinixTransport`), not here.
    #[test]
    fn test_devman_sef_production_hooks() {
        let root = RootStat::devman_root();
        let mut sef = DevmanSef::new(ServerConfig::devman_default(root)).unwrap();
        // Fresh init: rebuild is transparent (the framework re-inits).
        assert!(sef.init_server().is_ok());
    }

    /// Test double #2 (behavior: fresh init always fails).
    /// [ARCH:A-7] C `panic("init_inodes failed")` becomes explicit
    /// `Err(ENOMEM)`; the caller (not this trait) decides fail-fast.
    struct FailingSef;

    impl SefHooks for FailingSef {
        fn init_server(&mut self) -> Result<(), Errno> {
            Err(Errno::ENOMEM)
        }
    }

    #[test]
    fn sef_ok_records_lifecycle_sequence() {
        let mut sef = OkSef {
            sequence: alloc::vec::Vec::new(),
        };
        assert_eq!(sef.init_server(), Ok(()));
        assert_eq!(sef.sequence, alloc::vec![SefLifecycle::InitFresh]);
    }

    #[test]
    fn sef_failing_returns_enomem() {
        let mut sef = FailingSef;
        // [ARCH:A-7] allocation failure surfaces as ENOMEM, never panics.
        assert_eq!(sef.init_server(), Err(Errno::ENOMEM));
    }
}

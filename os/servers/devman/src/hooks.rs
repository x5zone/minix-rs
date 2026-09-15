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

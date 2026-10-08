//! File server runtime: the production half of the `minix-fs` task loop.
//!
//! C correspondence: the IPC-facing half of `minix3/minix/lib/libfsdriver`
//! (`fsdriver.c`'s receive/reply verbs plus the grant copies the adapters
//! issue) and the common shape of every file server's `main` —
//! `env_setargs`, `sef_local_startup`, `fsdriver_task` (for example
//! `minix3/minix/fs/mfs/main.c:29-45`). The `minix-fs` library deliberately
//! knows nothing about the kernel: its `FsTransport` documentation hands
//! receiving, replying, and grant copies to "the server runtime". This
//! crate is that runtime, shared by every file server binary on the
//! `os/fs/` side.
//!
//! Three pieces, none of them server-specific:
//! - [`transport::FsRt`] implements `minix_fs::task::FsTransport` over an
//!   injected [`ipc::RtIpc`]: it receives through the SEF layer, classifies
//!   file-system requests against the wire offset tables, decodes request
//!   bodies (fetching names out of grants, like `fsdriver_getname`), encodes
//!   replies, and moves bytes through the safecopy calls.
//! - The receive path also owns the **birth face**: an `RS_INIT` request
//!   from the restart server never reaches the task loop — it runs the
//!   injected init callback and reports the result back, exactly like
//!   `sef_startup` (`minix3/minix/lib/libsys/sef.c` tail) waiting for the
//!   init message and `do_sef_init_request` answering `RS_INIT`.
//! - [`source::BootBlockSource`] is what a server binary starts on: the
//!   boot image RAM disk ([`source::ImgrdBlockSource`], the E-FSBDEV
//!   minimal half) when the packaging supplied one, or the fail-closed
//!   [`source::PendingBlockSource`] whose mounts answer "input/output
//!   error" instead of pretending to read a disk. The bdev channel to a
//!   real block driver is the separately tracked full half of E-FSBDEV.

#![no_std]
// SD-19 静默吞错家族机械化门（第六家 crate）：本 runtime 的 `let _ =` 均为
// 发后即忘的 send（出生报告/终回复/错误回复）与尽力而为的 lookup 重写 copy_to，
// 失败对端会自行重启或无从重投。新开无理由 `let _ =` 当场多报一条。
#![warn(clippy::let_underscore_must_use)]

extern crate alloc;

pub mod ipc;
pub mod source;
pub mod transport;
pub mod wire;

pub use transport::{serve, serve_with, ServerHooks};

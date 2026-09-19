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
//! - [`source::PendingBlockSource`] is the fail-closed block source for
//!   servers whose driver channel is not wired yet (the block seam is the
//!   separately tracked E-FSBDEV item); mounts answer "input/output error"
//!   instead of pretending to read a disk.

#![no_std]

extern crate alloc;

pub mod ipc;
pub mod source;
pub mod transport;
pub mod wire;

pub use transport::{serve, serve_with, ServerHooks};

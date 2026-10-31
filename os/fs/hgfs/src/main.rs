//! FS server (hgfs) entry point — registered stub (edge `E-FSRUNTIME` 余量).
//!
//! The server body is **not** implemented yet: `minix-fs-hgfs` today carries the
//! `minix-sffs` parameter set, the case-option parser and an empty `init()`. The
//! real entry is the pfs-shaped fsdriver loop (`os/fs/pfs/src/{lib,main}.rs`),
//! same as vbfs's registered first slice (`15-stage-fs/todo.md` 的四家入口未接).
//!
//! This binary used to park in `loop {}` after an empty `init()` — a silent hang
//! that looks like a running service to RS and to any operator (SD-19's
//! "pretend" genus). Until the loop exists, refuse loudly: RS never launches
//! hgfs today, so a panic here is inert and turns a hanging mount into a
//! self-describing failure.

fn main() {
    minix_fs_hgfs::init();
    panic!(
        "hgfs: the fsdriver loop is not implemented (registered edge E-FSRUNTIME; \
         the pfs server is the template)"
    );
}

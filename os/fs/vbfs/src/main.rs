//! FS server (vbfs) entry point — registered stub (A 板 A20 第一缺).
//!
//! The server body is **not** implemented yet: `minix-fs-vbfs` today carries only
//! the `minix-sffs` parameter set (`lib.rs::default_params`) and its two constant
//! tests. The real entry is the pfs-shaped fsdriver loop (block-cache + inode
//! operations + the FS driver protocol, `os/fs/pfs/src/{lib,main}.rs`, ~800 lines);
//! `15-stage-fs/todo.md:3` 的「四家入口未接」把 vbfs 列为第一家。
//!
//! This binary used to park in `loop {}` after an empty `init()` — a silent hang
//! that looks like a running service to RS and to any operator. SD-19's genus in
//! its purest form: the process *pretends*. Until the loop exists, refuse loudly:
//! RS never launches vbfs today, so a panic here is inert on the boot path and
//! turns "why is my mount hanging" into "this server is not built yet".
//!
//! Progress marker (`2026-11-06`, A 板 A20 后端先行切片): the guest-library
//! connect core is in (`vbox.rs` — the backdoor ABI model over the libsys
//! verb set; `vboxfs.rs` — `vboxfs_init`'s MAP_FOLDER flow and the path
//! object, `minix-fs-vbfs` 8/0). What keeps this main a panic: the sffs
//! framework port (C `minix3/minix/fs/sffs/`) and the vboxguest driver
//! (machine face) are the registered prerequisites before the ring can
//! replace this refusal.

fn main() {
    minix_fs_vbfs::init();
    panic!(
        "vbfs: the fsdriver loop is not implemented (registered on A 板 A20, \
         15-stage-fs/todo.md:3; the pfs server is the template)"
    );
}

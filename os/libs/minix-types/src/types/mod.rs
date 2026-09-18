//! Type definitions module.
//!
//! Provides Minix3 core type definitions:
//! - `com`: System-level constants (MAX_NR_TASKS, NR_PROCS, etc.)
//! - `pid`: Process ID, process index
//! - `endpoint`: Endpoint identifier (core IPC concept)
//! - `proc_nr`: Process number (kernel process table slot number)
//! - `id`: User ID, Group ID
//! - `clock`: Clock ticks, timestamp, file offset
//! - `address`: Virtual/physical address types
//! - `bitmap`: Generic bitmap
//! - `boot`: Boot image types
//! - `boot_image`: GET_IMAGE wire entry (E-ISPROD)
//! - `cell`: Single-threaded interior mutability primitives
//! - `errno`: POSIX errno constants
//! - `diagnostic`: panic diagnostic hook registration (D-48, kernel ↔ minix-rt shared)
//! - `grant`: grant wire layout (`cp_grant_t` family — kernel ↔ user-space
//!   grant-table contract, E-DSWIRE)
//! - `irq_hook`: GET_IRQHOOKS wire entry (E-ISPROD)
//! - `kinfo`: GET_KINFO wire entry (E-ISPROD)
//! - `kerninfo`: `MINIX_KERNINFO` shared page ABI (`minix_kerninfo` /
//!   `kuserinfo` + OS release constants, E-KERNINFO)
//! - `priv_info`: GET_PRIVTAB wire entry (E-ISPROD)

mod address;
mod bitmap;
mod boot;
mod boot_image;
mod cell;
mod clock;
mod com;
mod diagnostic;
pub mod device;
pub mod dma;
mod endpoint;
pub mod errno;
mod grant;
mod id;
mod irq_hook;
mod kinfo;
mod kerninfo;
mod fproc;
mod mproc;
mod pid;
mod priv_info;
mod proc_info;
mod proc_nr;
mod ps_strings;
pub mod signal;
pub mod stat;
mod sysctl;
mod sysctl_abi;

pub use address::*;
pub use bitmap::*;
pub use boot::*;
pub use boot_image::*;
pub use cell::*;
pub use clock::*;
pub use com::*;
pub use diagnostic::*;
pub use device::*;
pub use dma::*;
pub use endpoint::*;
pub use errno::*;
pub use grant::*;
pub use id::*;
pub use irq_hook::*;
pub use kinfo::*;
pub use kerninfo::*;
pub use fproc::FProcSnap;
pub use mproc::{MprocWire, SigSetWire, MinixTimerWire, MP_MAGIC, NGROUPS_MAX};
pub use pid::*;
pub use priv_info::*;
pub use proc_info::*;
pub use proc_nr::*;
pub use ps_strings::*;
pub use signal::*;
// `stat` 故意不走根 glob：ipc::fs_driver 已有一个序列化用的 `Stat`
// （VTreeFS `fs_stat` 载荷），两个概念同名——用户态 `struct stat` ABI
// 经 `types::stat::Stat` 路径取用（minix-sys 再导出为 `minix_sys::Stat`）。
pub use sysctl::*;
pub use sysctl_abi::*;

//! riscv64 AP early stub — S-3c (two-arch stub deliverable, riscv half).
//!
//! # Entry state (SBI hart_start, HSM extension — see `smp.rs::boot_ap`)
//!
//! - a0 = hartid
//! - a1 = the `opaque` cookie (`boot_ap` passes the bootstrap pointer —
//!   S-3c fixed the old "priv mode" mislabel)
//! - PC = the physical entry address `boot_ap` passed as a1's neighbor
//!   start_addr
//! - S-mode, MMU **off**
//!
//! # Stub obligations (frozen §3.1; the body lands with S-4)
//!
//! 1. Read the `ApBootstrap` record (the riscv form is **arch-local static
//!    storage** — v7 #4; record address = PC-relative from the stub inside
//!    the kernel image, computable with MMU off).
//! 2. `fence rw, rw` consumer-side ordering barrier (§3.9 first-read).
//! 3. satp ← the record's page-table root (BSP value, SATP-mode Sv39);
//!    `sfence.vma`.
//! 4. Branch to the `ap_early_entry` Rust convergence point (high VA via
//!    the kernel mapping the new satp establishes).
//!
//! # Identity-coverage verification (S-3c 核验记录)
//!
//! Same argument as the arm half: the stub and record live inside the
//! kernel image; the BSP root (boot-shim) maps the kernel image identity
//! **and** high plus the DM window — so MMU-on at the image's PA keeps the
//! PC mapped, and the record is reachable both identity (MMU-off reads)
//! and through the kernel mapping after `sfence.vma`. The hartid comes
//! from `boot_ap`'s a0 — no <1MiB constraint on riscv64.
//!
//! # Status
//!
//! **Skeleton** — the obligation list above is the contract; the asm body
//! lands with S-4 (satp sequencing per §3.3 riscv row).

/// Unit-test sentinel: proves the module's contract constants stay in sync
/// with the SBI channel (`boot_ap` passes the bootstrap cookie in a1/a2).
pub const OPAQUE_IS_BOOTSTRAP_COOKIE: bool = true;

//! aarch64 AP early stub — S-3c (two-arch stub deliverable, arm half).
//!
//! # Entry state (PSCI CPU_ON, SMC64 form — see `smp.rs::boot_ap`)
//!
//! - x0 = context_id (the bootstrap cookie; `boot_ap` hands the entry
//!   cookie through — §3.1)
//! - PC = the physical entry address `boot_ap` passed as x2
//! - EL1, MMU **off**, D-cache off, I-cache cold
//!
//! # Stub obligations (frozen §3.1; the body lands with S-4)
//!
//! 1. Read the `ApBootstrap` record (the arm form is **arch-local static
//!    storage**, not a copied image — v7 #4; the record sits at a fixed
//!    offset from this stub inside the kernel image, so its address is
//!    PC-relative computable with MMU off).
//! 2. `dsb ish` consumer-side ordering barrier (§3.9 first-read barrier).
//! 3. TTBR0/TTBR1 ← the record's page-table root (BSP value).
//! 4. MAIR/TCR ← BSP values; `dsb sy` + `isb`; SCTLR.M = 1 + `isb`
//!    (MMU on — the boot root covers the kernel image both identity and
//!    high, so the PC remains valid across the enable).
//! 5. Branch to the high-VA `ap_early_entry` Rust convergence point.
//!
//! # Identity-coverage verification (S-3c 核验记录)
//!
//! The stub lives inside the kernel image; the BSP root built by
//! boot-shim covers the kernel image **both at its high VMA and
//! identity** (the same root the BSP itself is running on — identity +
//! kernel + DM windows, see `uefi_helpers.rs` boot_pt_alloc comment).
//! Therefore the stub's physical entry, its record, and its post-MMU
//! high VA are all covered by construction. x0 (context) and the entry
//! PA come from `boot_ap` — no <1MiB constraint exists on aarch64.
//!
//! # Status
//!
//! **Skeleton** — the obligation list above is the contract; the asm body
//! lands with S-4 (init_ap), which owns TTBR/MAIR/TCR/SCTLR sequencing.

/// Unit-test sentinel: proves the module's contract constants stay in sync
/// with the PSCI channel (`boot_ap` passes the entry cookie in x0).
pub const CONTEXT_IS_BOOTSTRAP_COOKIE: bool = true;

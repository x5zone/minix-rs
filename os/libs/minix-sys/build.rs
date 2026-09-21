//! minix-sys build script — the single authority for the kernel-trap cfg.
//!
//! `cfg(kernel_trap)` answers one question: *is there a kernel this binary
//! can trap into?* That is a property of the **target**, not of each
//! consuming package:
//!
//! - freestanding targets (`*-none*`: `x86_64-unknown-none`,
//!   `riscv64gc-unknown-none-elf`, `aarch64-unknown-none`) ship inside the
//!   boot image, where a live kernel always answers — the real trap legs
//!   are the only sane behavior. Before this cfg existed, each package had
//!   to opt in with `features = ["real-trap"]` in its own manifest, and
//!   every package that forgot silently got the `-EIO` stub instead —
//!   NK4-A fix20 proved this boot-killing (VM's `sys_exec` returned EIO
//!   because exactly one declaration was missing; the remaining 11 image
//!   modules had the same latent hole).
//! - hosted targets (tests on linux/macos/windows) have no kernel behind
//!   the trap — the transports keep their documented fail-closed `-EIO`.
//!
//! The historical `real-trap` feature stays as a **host-side force
//! switch** (experiments that link against a live kernel via some
//! userspace channel), so `kernel_trap = freestanding ∨ feature`.
//!
//! This mirrors the semantics the transport docs already state
//! (`syscall.rs`: "In a hosted test environment no kernel answers, so the
//! call reports a generic input-output failure explicitly").

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let freestanding = target.contains("-none");
    let forced = std::env::var("CARGO_FEATURE_REAL_TRAP").is_ok();
    if freestanding || forced {
        println!("cargo:rustc-cfg=kernel_trap");
    }
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_REAL_TRAP");
}

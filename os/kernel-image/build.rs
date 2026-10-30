//! Link-script wiring for bare-metal kernel images.
//!
//! The script is chosen by `TARGET`: `x86_64-unknown-none` uses
//! `x86_64.ld`, `aarch64-unknown-none` uses `aarch64.ld`, and
//! `riscv64gc-unknown-none-elf` uses `riscv64.ld`. All three have the same
//! shape (high VMA + low physical LMA, with `vaddr - paddr` consistent
//! across segments) and differ only in base constants and the entry
//! instruction sequence; the contract is written at the head of each
//! script. Host targets get no extra link arguments — this package has no
//! buildable target on the host anyway (the bin is isolated by the
//! fw-none-image required-features gate).

use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    // One bin, several architectures: `--target` selects the link script.
    let script = match target.as_str() {
        "x86_64-unknown-none" => Some("x86_64.ld"),
        "aarch64-unknown-none" => Some("aarch64.ld"),
        "riscv64gc-unknown-none-elf" => Some("riscv64.ld"),
        _ => None,
    };
    if let Some(script) = script {
        println!("cargo:rustc-link-arg-bins=-T{manifest}/{script}");
    }
    println!("cargo:rerun-if-changed=build.rs");
    // All three scripts are declared unconditionally: changing any one of
    // them must rerun this build.rs (a change there can also affect the
    // script-selection logic).
    println!("cargo:rerun-if-changed=x86_64.ld");
    println!("cargo:rerun-if-changed=aarch64.ld");
    println!("cargo:rerun-if-changed=riscv64.ld");
}

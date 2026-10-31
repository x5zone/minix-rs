//! Link-script wiring for the D-73 U-Boot leg shim bin.
//!
//! The UEFI bin needs no script (PE/UEFI handles its own load address); the
//! riscv64 U-Boot bin is physically linked — VMA == LMA == `0x84500000` — by
//! `riscv64-uboot.ld` (layout evidence in the script header). The link arg
//! is emitted only when the `fw-riscv-uboot` feature is selected **and** the
//! target is riscv64: every other (target × feature) combination leaves the
//! default link behavior untouched, so the UEFI bin and host tests are
//! byte-identical to before.

use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let uboot = env::var("CARGO_FEATURE_FW_RISCV_UBOOT").is_ok();
    if uboot && target == "riscv64gc-unknown-none-elf" {
        println!("cargo:rustc-link-arg-bins=-T{manifest}/riscv64-uboot.ld");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=riscv64-uboot.ld");
}

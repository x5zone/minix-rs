//! Minix-RS Kernel binary stub.
//!
//! This is a placeholder for the UEFI-bootable kernel binary. In production,
//! this would be compiled as a separate `[[bin]]` with `test = false`.
//! During development, tests run through the `lib.rs` entry points with MockPaging.
//!
//! To restore the binary target, add this to kernel/Cargo.toml:
//! ```toml
//! [[bin]]
//! name = "minix-kernel"
//! path = "src/main.rs"
//! test = false
//! ```

fn main() {
    eprintln!("Kernel binary: build with --target x86_64-unknown-uefi");
}

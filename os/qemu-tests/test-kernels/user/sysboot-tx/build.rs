//! Per-target link script selection: the freestanding birth image links
//! differently per architecture (x86-64 keeps the original script; the
//! riscv64 variant adds the sdata/sbss small-data sections and declares
//! the riscv output arch). K12b riscv64 leg.
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let target = std::env::var("TARGET").unwrap_or_default();
    let script = if target.starts_with("riscv64") {
        "link-riscv64.ld"
    } else if target.starts_with("aarch64") {
        "link-aarch64.ld"
    } else {
        "link.ld"
    };
    println!("cargo:rustc-link-arg=-T{dir}/{script}");
}

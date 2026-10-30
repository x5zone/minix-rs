fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{dir}/mb.ld");
    println!("cargo:rerun-if-changed=mb.ld");
    // Identity low linking: non-PIE (the trampoline/ladder use absolute
    // addressing), static with no dynamic segment.
    println!("cargo:rustc-link-arg=-no-pie");
    println!("cargo:rustc-link-arg=-static");
}

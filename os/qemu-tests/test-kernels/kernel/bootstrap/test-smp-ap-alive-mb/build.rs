fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{dir}/mb.ld");
    println!("cargo:rerun-if-changed=mb.ld");
    // 恒等低链接：非 PIE（跳板/阶梯使用绝对寻址），静态无动态段。
    println!("cargo:rustc-link-arg=-no-pie");
    println!("cargo:rustc-link-arg=-static");
}

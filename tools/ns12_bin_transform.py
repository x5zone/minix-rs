#!/usr/bin/env python3
"""NS12 批二：标准形命令 bin 双 seam 切换（claim/NS12-zcode_glm_2）。

变换对象＝fileops 批一已钉死的模板形：含 mod support、无 print!、std 面
仅 {std::env::args, std::process::exit, std::str::from_utf8} 的 bin。
模板契约（echo.rs / init 判例）：
  1. //! 头后插 no_std/no_main 门 + extern crate alloc;
  2. argv 行走 support::args()（bin_support cfg 双形）
  3. std::process::exit → support::terminate
  4. std::str::from_utf8 → core::str::from_utf8（两侧同构）
  5. fn main → fn run() -> ! + 尾部双形 main
    （run 体若可落穿，编译期 E0308 暴露，手工补 terminate(0)）
"""
import re
import sys
from pathlib import Path

HEADER_ATTR = '#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]'
TRAILER = '''

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
'''


def transform(path: Path) -> str:
    src = path.read_text()
    if re.search(r'#!\[[^\]]*no_std', src):
        return "skip:no_std"
    if "mod support;" not in src:
        return "skip:no-support"
    if re.search(r"println!|eprintln!|print!", src):
        return "skip:print"
    if "fn main()" not in src:
        return "skip:no-main"

    lines = src.split("\n")
    # 1) insertion point: after the last leading //! line
    insert_at = 0
    for i, line in enumerate(lines):
        if line.startswith("//!"):
            insert_at = i + 1
        elif i == insert_at and line.strip() == "":
            insert_at = i + 1
        else:
            break
    lines.insert(insert_at, "")
    lines.insert(insert_at + 1, HEADER_ATTR)
    lines.insert(insert_at + 2, "")
    lines.insert(insert_at + 3, "extern crate alloc;")
    src = "\n".join(lines)

    # 2) argv seam
    n_args = src.count("std::env::args().collect()")
    src = src.replace(
        "let argv: Vec<String> = std::env::args().collect();",
        "let argv: Vec<String> = support::args();",
    )
    src = src.replace(
        "let args: Vec<String> = std::env::args().collect();",
        "let args: Vec<String> = support::args();",
    )
    # 3) exit seam
    src = src.replace("std::process::exit(", "support::terminate(")
    # 4) str reexport
    src = src.replace("std::str::from_utf8", "core::str::from_utf8")
    # 5) main → run
    src = src.replace("fn main() {", "fn run() -> ! {", 1)
    src = src.rstrip("\n") + "\n" + TRAILER

    # alloc uses for types named in the body
    body_needs_vec = re.search(r"\bVec\b", src) and "use alloc::vec::Vec;" not in src
    body_needs_string = (
        re.search(r"\bString\b", src) and "use alloc::string::String;" not in src
    )
    if body_needs_vec or body_needs_string:
        uses = []
        if body_needs_string:
            uses.append("use alloc::string::String;")
        if body_needs_vec:
            uses.append("use alloc::vec::Vec;")
        anchor = "extern crate alloc;\n"
        src = src.replace(anchor, anchor + "\n" + "\n".join(uses) + "\n", 1)

    path.write_text(src)
    leftover = n_args - src.count("support::args();")
    return f"ok:args{leftover}"


def main() -> None:
    for raw in sys.argv[1:]:
        path = Path(raw)
        print(f"{transform(path)}: {path}")


if __name__ == "__main__":
    main()

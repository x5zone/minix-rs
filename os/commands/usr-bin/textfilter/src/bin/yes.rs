//! Minix-RS yes — repeated affirmation.
//!
//! Ground truth: `minix3/usr.bin/yes/yes.c` (NetBSD), main at lines 42-56:
//! the operand defaults to "y" and the program prints it with a newline
//! forever; the loop only ends when the write fails, and the C then
//! leaves with `EXIT_FAILURE`. There is no deciding half — the doing
//! half is the whole command.
//!
//! 这一支的参数**不是**档名也不是旗标，它就是被重复打出的那段内容：`yes.c:52` 把
//! `argv[1]` 存成 `const char *`，`puts` 按字节发出再加换行。因此命令行上的一枚裸
//! `ff` 必须原样出现在每一行里；先折成 `ef bf bd` 再打，改的是输出内容本身。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // `yes.c:52` 的 `yes = (argc > 1) ? argv[1] : "y"` 取的是 argv 里那串**字节**，
    // `puts` 也只写字节：参数就是输出内容，折一次就改的是被重复打出的那些字节。
    let word: &[u8] = argv.get(1).map_or(b"y", Vec::as_slice);
    let mut line = word.to_vec();
    line.push(b'\n');
    // The write channel keeps refusing on the host until the transport
    // lands; a refusing write is the C's loop-exit (`puts < 0`) and
    // leaves status 1.
    while support::write_ok(&line) {
        continue;
    }
    support::terminate(1);
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}

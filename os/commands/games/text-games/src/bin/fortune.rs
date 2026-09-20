//! Minix-RS fortune — the doing half over `minix_text_games::fortune`.
//!
//! Ground truth: `minix3/games/fortune/fortune/fortune.c`——格言库按行
//! 切分，抽取下标对库长取模（`pick_quip` 的决定半语义）。本壳读数据
//! 文件（缺省 `/usr/share/games/fortune`，操作数可指），抽取值取时钟
//! 秒（真机再混进程号），命中行打印。


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::fortune::{pick_quip, SliceQuips};
use minix_sys::{open, read, Fd};

const DEFAULT_DB: &str = "/usr/share/games/fortune";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let path = args.get(1).copied().unwrap_or(DEFAULT_DB);

    let fd: Fd = match open(path, 0, 0) {
        Ok(fd) => fd,
        Err(_) => {
            support::warn(format!("fortune: {path}: cannot open\n").as_bytes());
            support::terminate(1);
        }
    };
    let mut image = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => image.extend_from_slice(&chunk[..n]),
            Err(_) => {
                support::warn(format!("fortune: {path}: read error\n").as_bytes());
                support::terminate(1);
            }
        }
    }
    let _ = minix_sys::close(fd);

    let text = String::from_utf8_lossy(&image).into_owned();
    let quips: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let draw = (support::epoch_micros().unwrap_or(0) / 1_000_000) as u32;
    let db = SliceQuips::new(&quips);
    match pick_quip(&db, draw) {
        Ok(quip) => support::emit(format!("{quip}\n").as_bytes()),
        Err(_) => {
            support::warn(format!("fortune: {path}: empty database\n").as_bytes());
            support::terminate(1);
        }
    }
    support::terminate(0)
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

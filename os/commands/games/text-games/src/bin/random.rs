//! Minix-RS random — the doing half over `minix_text_games::lottery`.
//!
//! Ground truth: `minix3/games/random/random.c`——`random N` 从标准输入
//! 读行，每行以 1/N 概率被选中（后中覆盖先中），最后打印选中行；
//! 播种混入时钟与进程号（`mix_seed` 的决定半语义）。抽取值由薄壳内的
//! LCG 生成（textfilter `jot.rs` 先例），播种料来自时钟与 `getpid`。


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::lottery::{is_selected, mix_seed, parse_denominator};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let Some(word) = argv.get(1) else {
        support::warn(b"usage: random [N]\n");
        support::terminate(1);
    };
    let denominator = match parse_denominator(word) {
        Ok(v) => v,
        Err(_) => {
            support::warn(format!("random: {word}: bad denominator\n").as_bytes());
            support::terminate(1);
        }
    };

    let micros = support::epoch_micros().unwrap_or(0);
    let pid = minix_sys::getpid().unwrap_or(0) as u32;
    let mut state = mix_seed(micros / 1_000_000, (micros % 1_000_000) as u32, pid);

    // 逐行读标准输入：每行掷一次 [0, N)，掷中 0 即替换当前选中行。
    let mut chosen: Option<String> = None;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        match read(0, &mut chunk) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(_) => {
                support::warn(b"random: read error\n");
                support::terminate(1);
            }
        }
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    for line in text.lines() {
        // LCG（Numerical Recipes 常数），取模前先掷高位。
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let draw = ((state >> 33) as u32) % denominator;
        buf.clear();
        buf.extend_from_slice(line.as_bytes());
        match is_selected(draw, denominator) {
            Ok(true) => chosen = Some(line.to_string()),
            Ok(false) => {}
            Err(_) => {}
        }
    }
    match chosen {
        Some(line) => support::emit(format!("{line}\n").as_bytes()),
        None => {
            support::warn(b"random: nothing selected\n");
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

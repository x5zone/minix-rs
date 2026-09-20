//! Minix-RS stty — the doing half over `minix_termctl::stty`.
//!
//! Ground truth: `minix3/bin/stty/stty.c` — no operands prints the speed
//! line; `-a` prints everything; `-g` prints the machine-readable grep
//! form (`gfmt.c:61-71`); operands parse then apply to the current
//! attributes (`tcgetattr` → apply → `tcsetattr`), including whole
//! `gfmt1:...` operands (`stty.c:143-146`). The C main loop prints and
//! then consumes operands even in `-a`/`-g` mode; this shell keeps the
//! mutually exclusive branch shape declared in 13-terminal-termios.md §5.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

#[path = "../bin_support.rs"]
mod support;
use minix_sys::{tcgetattr, tcsetattr};
use minix_termctl::stty::{apply_ops, display_a, grep_print, parse_args};

const STDIN: i32 = 0;
const STDOUT: i32 = 1;
const STDERR: i32 = 2;


fn fail(message: &str) -> ! {
    let _ = minix_sys::write(STDERR, message.as_bytes());
    let _ = minix_sys::write(STDERR, b"\n");
    support::terminate(1)
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let all: Vec<&str> = argv.iter().map(String::as_str).collect();
    let args: Vec<&str> = all[1..].to_vec();

    if args.first().map(|a| *a == "-g").unwrap_or(false) {
        // C 的 STTY_GFLAG 支路（stty.c:120-122）：tcgetattr 后 gprint 一行。
        let mut t = minix_sys::Termios::new();
        tcgetattr(STDIN, &mut t).unwrap_or_else(|_| fail("stdin is not a terminal"));
        let mut out = [0u8; 512];
        match grep_print(&t, &mut out) {
            Ok(used) => {
                let _ = minix_sys::write(STDOUT, &out[..used]);
            }
            Err(_) => fail("display buffer too small"),
        }
        support::terminate(0);
    }
    if args.first().map(|a| *a == "-a").unwrap_or(false) {
        let mut t = minix_sys::Termios::new();
        tcgetattr(STDIN, &mut t).unwrap_or_else(|_| fail("stdin is not a terminal"));
        let mut out = [0u8; 512];
        match display_a(&t, &mut out) {
            Ok(used) => {
                let _ = minix_sys::write(STDOUT, &out[..used]);
            }
            Err(_) => fail("display buffer too small"),
        }
        support::terminate(0);
    }

    if args.is_empty() {
        // 无操作数：只打速度行（C `stty` 的缺省面）。
        let mut t = minix_sys::Termios::new();
        tcgetattr(STDIN, &mut t).unwrap_or_else(|_| fail("stdin is not a terminal"));
        let line = format!("speed {} baud\n", t.c_ospeed);
        let _ = minix_sys::write(STDOUT, line.as_bytes());
        support::terminate(0);
    }

    let (ops, count) = match parse_args(&args) {
        Ok(pair) => pair,
        Err(_) => fail("unrecognized operand"),
    };
    let mut t = minix_sys::Termios::new();
    tcgetattr(STDIN, &mut t).unwrap_or_else(|_| fail("stdin is not a terminal"));
    apply_ops(&ops, count, &mut t);
    tcsetattr(STDIN, &t).unwrap_or_else(|_| fail("cannot set terminal attributes"));
    support::terminate(0);
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

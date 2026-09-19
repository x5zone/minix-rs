//! Minix-RS stty — the doing half over `minix_termctl::stty`.
//!
//! Ground truth: `minix3/bin/stty/stty.c` — no operands prints the speed
//! line; `-a` prints everything; operands parse then apply to the current
//! attributes (`tcgetattr` → apply → `tcsetattr`). `-g` (the readable
//! single-line form) is declared unwired — 13-terminal-termios.md §5.

use minix_sys::{tcgetattr, tcsetattr};
use minix_termctl::stty::{apply_ops, display_a, parse_args};

const STDIN: i32 = 0;
const STDOUT: i32 = 1;
const STDERR: i32 = 2;

fn terminate(code: i32) -> ! {
    std::process::exit(code)
}

fn fail(message: &str) -> ! {
    let _ = minix_sys::write(STDERR, message.as_bytes());
    let _ = minix_sys::write(STDERR, b"\n");
    terminate(1)
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let all: Vec<&str> = argv.iter().map(String::as_str).collect();
    let args: Vec<&str> = all[1..].to_vec();

    if args.first().map(|a| *a == "-g").unwrap_or(false) {
        fail("option not wired");
    }
    if args.first().map(|a| *a == "-a").unwrap_or(false) {
        let mut t = minix_types::Termios::new();
        tcgetattr(STDIN, &mut t).unwrap_or_else(|_| fail("stdin is not a terminal"));
        let mut out = [0u8; 512];
        match display_a(&t, &mut out) {
            Ok(used) => {
                let _ = minix_sys::write(STDOUT, &out[..used]);
            }
            Err(_) => fail("display buffer too small"),
        }
        terminate(0);
    }

    if args.is_empty() {
        // 无操作数：只打速度行（C `stty` 的缺省面）。
        let mut t = minix_types::Termios::new();
        tcgetattr(STDIN, &mut t).unwrap_or_else(|_| fail("stdin is not a terminal"));
        let line = format!("speed {} baud\n", t.c_ospeed);
        let _ = minix_sys::write(STDOUT, line.as_bytes());
        terminate(0);
    }

    let (ops, count) = match parse_args(&args) {
        Ok(pair) => pair,
        Err(_) => fail("unrecognized operand"),
    };
    let mut t = minix_types::Termios::new();
    tcgetattr(STDIN, &mut t).unwrap_or_else(|_| fail("stdin is not a terminal"));
    apply_ops(&ops, count, &mut t);
    tcsetattr(STDIN, &t).unwrap_or_else(|_| fail("cannot set terminal attributes"));
    terminate(0);
}

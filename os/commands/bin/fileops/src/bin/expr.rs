//! Minix-RS expr — POSIX expression evaluation.
//!
//! Ground truth: `minix3/bin/expr/expr.y` (NetBSD), main at lines 441-457
//! and the grammar at lines 71-244. The deciding half (precedence,
//! arithmetic with the C's overflow checks, comparisons, `:` matching,
//! `length`) is the library's `expr` module; this program prints the
//! result and leaves status 0 or 1 by the null-or-zero rule
//! (expr.y:73-76), and status 2 for every evaluation error
//! (expr.y:429-439), whose diagnostics go to standard error.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::string::String;
use alloc::vec::Vec;
use minix_fileops::expr::{evaluate, is_zero_or_null, ExprError};

/// The whole program body; both `main` forms call it and it never
/// returns (every path ends in [`support::terminate`]). The two-form
/// entry contract is documented once in `echo.rs`, this crate's
/// template binary.
fn run() -> ! {
    let argv: Vec<String> = support::args();
    if argv.len() < 2 {
        support::warn(b"usage: expr expression\n");
        support::terminate(2);
    }
    let tokens: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let result = match evaluate(&tokens) {
        Ok(result) => result,
        Err(error) => {
            support::warn(&diagnose(&error));
            support::terminate(2);
        }
    };
    let mut line = result.as_bytes().to_vec();
    line.push(b'\n');
    support::emit(&line);
    if is_zero_or_null(&result) {
        support::terminate(1);
    }
    support::terminate(0);
}

/// Renders one evaluation error in the C `verrx` shapes
/// (expr.y:281-306, 319-333, 336-343, 429-439); the "not reached"
/// comments in the C are these exits.
fn diagnose(error: &ExprError) -> Vec<u8> {
    let mut line = Vec::new();
    match error {
        ExprError::Syntax => line.extend_from_slice(b"expr: syntax error\n"),
        ExprError::NonInteger(word) => {
            line.extend_from_slice(b"expr: non-integer argument '");
            line.extend_from_slice(word.as_bytes());
            line.extend_from_slice(b"'\n");
        }
        ExprError::OutOfRange(word) => {
            line.extend_from_slice(b"expr: value '");
            line.extend_from_slice(word.as_bytes());
            line.extend_from_slice(b"' is out of range\n");
        }
        ExprError::DivideByZero(op) => {
            line.extend_from_slice(b"expr: second argument to '");
            line.extend_from_slice(op.as_bytes());
            line.extend_from_slice(b"' must not be zero\n");
        }
        ExprError::Overflow { left, op, right } => {
            line.extend_from_slice(b"expr: integer overflow or underflow occurred for operation '");
            line.extend_from_slice(left.as_bytes());
            line.push(b' ');
            line.extend_from_slice(op.as_bytes());
            line.push(b' ');
            line.extend_from_slice(right.as_bytes());
            line.extend_from_slice(b"'\n");
        }
        ExprError::BadRegex => {
            line.extend_from_slice(b"expr: bad regular expression\n");
        }
    }
    line
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

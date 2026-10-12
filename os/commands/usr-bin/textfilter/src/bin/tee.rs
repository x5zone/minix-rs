//! Minix-RS tee — fan stdin out to stdout and file operands.
//!
//! Ground truth: `minix3/usr.bin/tee/tee.c` (NetBSD): `-a` appends
//! instead of truncating, stdin copies to stdout and every operand file,
//! and open/write failures warn while the exit status leaves 1 (a failed
//! file is not dropped — later chunks retry it, matching the C list).
//! Operands are created with `open(O_WRONLY|O_CREAT|(O_APPEND|O_TRUNC),
//! 0666)` (`tee.c:102-103`, flags per `minix3/sys/sys/fcntl.h`); `-i`'s
//! signal face (ignore SIGINT) is not wired in this no_std userland and
//! stays deferred. Real-machine file creation is gated at C-57/E5.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read, write, Fd};

// `open` flags from `minix3/sys/sys/fcntl.h` (the Minix values, not the
// Linux ones): O_WRONLY 0x1 (:65), O_APPEND 0x8 (:82), O_CREAT 0x200 (:99),
// O_TRUNC 0x400 (:100).
const O_WRONLY: i32 = 0x1;
const O_APPEND: i32 = 0x8;
const O_CREAT: i32 = 0x200;
const O_TRUNC: i32 = 0x400;

/// Argument-splitting errors for the tee front end.
#[derive(PartialEq, Debug)]
enum TeeArgError {
    /// `-i` (ignore SIGINT) — needs the signal face, not wired yet.
    SignalPending,
    /// Any option other than `-a`/`-i`.
    BadFlag,
}

/// Splits tee argv into `(append, output operands)`, faithfully folding BSD
/// `getopt("ai")` (`tee.c:80`). Option letters **cluster within one token**
/// (`-ai` is `-a -i`); `-a` selects append, `-i` is rejected (its signal face is
/// pending), any other letter is a usage error. `--` ends option scanning; every
/// remaining word is an output file. tee's input is always stdin, so a bare `-`
/// operand is just a file named `-` (`tee.c:102` `open`s it literally).
fn parse_tee_args<'a>(argv: &'a [&'a str]) -> Result<(bool, Vec<&'a str>), TeeArgError> {
    let mut append = false;
    let mut operands: Vec<&'a str> = Vec::new();
    let mut index = 0usize;
    while index < argv.len() {
        let arg = argv[index];
        if arg == "--" {
            operands.extend_from_slice(&argv[index + 1..]);
            break;
        }
        let bytes = arg.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            // Scan the token's option letters (getopt clusters them).
            for &letter in &bytes[1..] {
                match letter {
                    b'a' => append = true,
                    b'i' => return Err(TeeArgError::SignalPending),
                    _ => return Err(TeeArgError::BadFlag),
                }
            }
        } else {
            operands.push(arg);
        }
        index += 1;
    }
    Ok((append, operands))
}

/// The create-mode open flags: append (`O_APPEND`) versus truncate
/// (`O_TRUNC`), both `O_WRONLY|O_CREAT` — `tee.c:102-103`.
fn open_flags(append: bool) -> i32 {
    if append {
        O_WRONLY | O_CREAT | O_APPEND
    } else {
        O_WRONLY | O_CREAT | O_TRUNC
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let words: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (append, operands) = match parse_tee_args(&words[1..]) {
        Ok(parsed) => parsed,
        Err(TeeArgError::SignalPending) => {
            support::warn(b"tee: -i waits for the signal face\n");
            support::terminate(1);
        }
        Err(TeeArgError::BadFlag) => {
            support::warn(b"usage: tee [-ai] [file ...]\n");
            support::terminate(1);
        }
    };

    // `tee.c:101-107`: open each output operand; an unopenable operand warns,
    // flags the exit status and is skipped (stdout is always a target).
    let mut files: Vec<(Fd, String)> = Vec::new();
    let mut rval = 0;
    for operand in &operands {
        match open(operand, open_flags(append), 0o666) {
            Ok(fd) => files.push((fd, (*operand).to_string())),
            Err(_) => {
                support::warn(format!("tee: {operand}\n").as_bytes());
                rval = 1;
            }
        }
    }

    // `tee.c:109-123`: read stdin in chunks and write each whole chunk to
    // stdout and every open file; a failed target warns and flags the status
    // (the file stays in the list, retried on the next chunk).
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                let slice = &chunk[..count];
                if write_partial(support::STDOUT, slice).is_err() {
                    support::warn(b"tee: stdout\n");
                    rval = 1;
                }
                for (fd, name) in &files {
                    if write_partial(*fd, slice).is_err() {
                        support::warn(format!("tee: {name}\n").as_bytes());
                        rval = 1;
                    }
                }
            }
            Err(_) => {
                support::warn(b"tee: read\n");
                rval = 1;
                break;
            }
        }
    }

    // `tee.c:129-134`: close every operand file; a close error flags the status.
    for (fd, name) in &files {
        if close(*fd).is_err() {
            support::warn(format!("tee: {name}\n").as_bytes());
            rval = 1;
        }
    }
    support::terminate(rval);
}

/// Writes the whole `bytes` to `fd`, looping over short writes; a zero-length
/// or failed write returns `Err` (the C's `do { write } while (n -= wval)`).
fn write_partial(fd: Fd, bytes: &[u8]) -> Result<(), ()> {
    let mut written = 0usize;
    while written < bytes.len() {
        match write(fd, &bytes[written..]) {
            Ok(0) => return Err(()),
            Ok(n) => written += n,
            Err(_) => return Err(()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operands_collected_in_order() {
        let (append, operands) = parse_tee_args(&["a.txt", "b.txt"]).unwrap();
        assert!(!append);
        assert_eq!(operands, ["a.txt", "b.txt"]);
    }

    #[test]
    fn dash_a_selects_append() {
        let (append, _) = parse_tee_args(&["-a", "f"]).unwrap();
        assert!(append);
    }

    #[test]
    fn dash_i_is_deferred() {
        assert_eq!(parse_tee_args(&["-i", "f"]), Err(TeeArgError::SignalPending));
    }

    #[test]
    fn unknown_flag_is_a_usage_error() {
        assert_eq!(parse_tee_args(&["-z", "f"]), Err(TeeArgError::BadFlag));
    }

    #[test]
    fn option_letters_cluster_within_a_token() {
        // 关键回归：C getopt("ai") 接受聚簇，旧整 token 匹配把 `-ai` 误拒。
        // `-a` 后接 `-i`：聚簇命中 -i → 仍按信号未接报 SignalPending。
        assert_eq!(
            parse_tee_args(&["-ai", "f"]),
            Err(TeeArgError::SignalPending)
        );
        assert_eq!(
            parse_tee_args(&["-ia", "f"]),
            Err(TeeArgError::SignalPending)
        );
        // 纯聚簇 -a 两次幂等仍为 append。
        let (append, operands) = parse_tee_args(&["-aa", "f"]).unwrap();
        assert!(append);
        assert_eq!(operands, ["f"]);
        // 聚簇里混入集外字母 → BadFlag（C `default:`）。
        assert_eq!(parse_tee_args(&["-az", "f"]), Err(TeeArgError::BadFlag));
    }

    #[test]
    fn double_dash_ends_option_scanning() {
        let (append, operands) = parse_tee_args(&["-a", "--", "-i", "f"]).unwrap();
        assert!(append);
        assert_eq!(operands, ["-i", "f"]);
    }

    #[test]
    fn flags_and_files_mix_in_any_order() {
        let (append, operands) = parse_tee_args(&["f1", "-a", "f2"]).unwrap();
        assert!(append);
        assert_eq!(operands, ["f1", "f2"]);
    }

    #[test]
    fn bare_dash_is_a_literal_output_file() {
        let (_, operands) = parse_tee_args(&["-"]).unwrap();
        assert_eq!(operands, ["-"]);
    }

    #[test]
    fn open_flags_truncate_versus_append() {
        assert_eq!(open_flags(false), O_WRONLY | O_CREAT | O_TRUNC);
        assert_eq!(open_flags(true), O_WRONLY | O_CREAT | O_APPEND);
    }
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

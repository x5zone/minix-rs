//! Minix-RS cksum — checksums and byte counts.
//!
//! Ground truth: `minix3/usr.bin/cksum/` (NetBSD). The deciding half —
//! the POSIX CRC, the two historic sums, the block-rounding of the printed
//! length, and the `%lu %lld` output format (`print.c` `pcrc`/`psum1`/`psum2`)
//! — is the library's `cksum` module. This program reads each file operand (or
//! standard input when none) and prints `{crc} {length}` plus ` {name}` for a
//! file; the C's `-a` crypto algorithms (md5/sha1/sha256/sha384/sha512) need
//! crypto primitives and are declared unsupported.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::GetoptReport;
use minix_textfilter::cksum::{
    CheckEntry, CksumError, SumKind, cksum_crc, csum1, csum2, parse_check_line, parse_cksum_args,
    sum_display_length,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    // `argv[0]` is the program name; C's getopt loop starts at `argv[1]`.
    let request = match parse_cksum_args(&refs[1..]) {
        Ok(request) => request,
        Err(CksumError::IllegalOptionValue(flag)) => {
            // C：`warnx("illegal argument to -o option")` + `usage()`（`cksum.c:219`、`:243`）。
            support::warn(format!("{prog}: illegal argument to -{flag} option\n").as_bytes());
            usage();
        }
        Err(CksumError::IllegalUseOfA) => {
            // C 的 `case 'a'` 第一行（`cksum.c:195-199`）：`warnx` 两段形，不缀 strerror，
            // 随后 `usage()`。这一形要 hash 已被选中才到得了，所以它跟着状态走。
            support::warn(format!("{prog}: illegal use of -a option\n").as_bytes());
            usage();
        }
        Err(CksumError::MutuallyExclusiveWithSum(name)) => {
            // C 的 `warnx("%s mutually exclusive with sum", hash->hashname)`
            //（`cksum.c:228-234`）：句子里是表里那一份**大写**名字（`MD5` 而不是 `md5`）。
            support::warn(format!("{prog}: {name} mutually exclusive with sum\n").as_bytes());
            usage();
        }
        Err(CksumError::RequiresHash(flag)) => {
            // C 的 `requirehash`（`cksum.c:521-525`）：`<旗标> flag requires `-a algorithm'`
            // 再接 usage。旧写法把句子自创成「requires a hash algorithm」且不接 usage。
            support::warn(format!("{prog}: -{flag} flag requires `-a algorithm'\n").as_bytes());
            usage();
        }
        Err(CksumError::UnknownOption(flag)) => {
            // libc 的那一句（`getopt.c:101-104`）是 `unknown option`，不是 `illegal option`——
            // 后者属于 `pr` 链的 `egetopt`，同一族里不能混用。
            support::warn(GetoptReport::UnknownOption(flag).line(prog).as_bytes());
            usage();
        }
        Err(CksumError::MissingValue(flag)) => {
            // 带值选项缺值：libc 的另一句（`getopt.c:132-136`），同样接 usage。
            support::warn(GetoptReport::MissingValue(flag).line(prog).as_bytes());
            usage();
        }
    };

    if request.hash.is_some() {
        // 选中了算法，接下来要的是一行真摘要，而本树还没有这些原语。这一句是本壳的
        // 占位而**不是** C 的形状——C 在这里打的是 `MD5 (file) = <digest>` 并退 0——
        // 退码沿用批次三百零七定下的 2，等原语那一片接上再换（登记 `CKSUM-HASH-STATE`）。
        // 放在解析之后：`-a` 的两条互斥诊断（C 在 getopt 环里就报完）必须先看状态，
        // 不能被这一句抢在头里。
        support::warn(b"cksum: -a crypto algorithms are not modelled yet\n");
        support::terminate(2);
    }

    if request.do_check {
        // `-c` consumes only the first operand as the check file (extra
        // operands are ignored), mirroring C's `argv[0]`-or-stdin choice.
        support::terminate(check_files(&request.operands, request.check_warn));
    }

    let mut status = 0;
    if request.operands.is_empty() {
        // C computes over stdin and prints the line with no filename.
        let input = read_all(support::STDIN);
        emit_line(request.kind, &input, None);
        support::terminate(status);
    }
    for operand in request.operands.iter().copied() {
        let fd = match open(operand, 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                // C warns and skips the file, no output line for it.
                support::warn(format!("cksum: {operand}\n").as_bytes());
                status = 1;
                continue;
            }
        };
        let input = read_all(fd);
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
        emit_line(request.kind, &input, Some(operand));
    }
    support::terminate(status)
}

/// `cksum -c`: verify the checksums listed in a check file (the first operand,
/// or standard input when there is none). Faithful to C's `do_check`
/// (`cksum.c:283-460`): a line is silent on success, prints `name: FAILED` on a
/// mismatch or an unreadable file, and the whole run exits 1 if any line failed.
/// The `name (file) = digest` hash grammar and a spaceless "bogus format" line
/// are counted as failures (the hash needs a crypto primitive we do not carry),
/// warned on only under `-w`. `-c` on an unreadable check file is C's `err(1,
/// "Cannot read %s")` — a hard exit 1.
fn check_files(operands: &[&str], check_warn: bool) -> i32 {
    let from_operand = !operands.is_empty();
    let fd = if from_operand {
        match open(operands[0], 0, 0) {
            Ok(fd) => fd,
            Err(_) => {
                support::warn(format!("cksum: Cannot read {}\n", operands[0]).as_bytes());
                return 1;
            }
        }
    } else {
        support::STDIN
    };
    let bytes = read_all(fd);
    if from_operand {
        //[SD19-gate] SD-19: close 后 fd 不再使用
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(fd);
    }
    let Ok(text) = core::str::from_utf8(&bytes) else {
        return 1;
    };
    // C reads line-by-line with `fgets`, so a trailing newline terminates the
    // last real line and yields no phantom empty line; peel exactly one.
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut failed = false;
    if !body.is_empty() {
        for line in body.split('\n') {
            match parse_check_line(line) {
                CheckEntry::Hash => {
                    if check_warn {
                        support::warn(format!("cksum: unknown hash: {line}\n").as_bytes());
                    }
                    failed = true;
                }
                CheckEntry::Malformed => {
                    if check_warn {
                        support::warn(
                            format!("cksum: bogus format: {line}. Skipping...\n").as_bytes(),
                        );
                    }
                    failed = true;
                }
                CheckEntry::Checksum { crc, name } => {
                    let ok = match open(name, 0, 0) {
                        Err(_) => {
                            if check_warn {
                                support::warn(format!("cksum: {name}\n").as_bytes());
                            }
                            false
                        }
                        Ok(data_fd) => {
                            let data = read_all(data_fd);
                            //[SD19-gate] SD-19: close 后 fd 不再使用
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = close(data_fd);
                            cksum_crc(&data).0 == crc
                        }
                    };
                    if !ok {
                        support::emit(format!("{name}: FAILED\n").as_bytes());
                        failed = true;
                    }
                }
            }
        }
    }
    i32::from(failed)
}

/// Read a descriptor to the end (the C `crc`/`csum` read the whole input).
fn read_all(fd: i32) -> Vec<u8> {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(fd, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    input
}

/// Compute the selected sum over `data` and print C's `"%lu %lld"` line (with a
/// trailing ` name` for a file operand), the length folded per `print.c`.
fn emit_line(kind: SumKind, data: &[u8], name: Option<&str>) {
    let (value, bytes) = match kind {
        SumKind::Crc => cksum_crc(data),
        SumKind::Old1 => csum1(data),
        SumKind::Old2 => csum2(data),
    };
    let shown = sum_display_length(kind, bytes);
    match name {
        Some(n) => support::emit(format!("{value} {shown} {n}\n").as_bytes()),
        None => support::emit(format!("{value} {shown}\n").as_bytes()),
    }
}

/// C 的 `usage()`（`cksum.c:535-550`）：九行——`cksum`/`sum` 各占两行（参数串与
/// 文件串分列），`md2`/`md4`/`md5`/`rmd160`/`sha1` 各一行。名字在 C 里是**字面量**
/// （递的是 `sumargs`/`hashargs`/`fileargs` 三个串，不用 `getprogname()`），所以这一
/// 块的缩进与行项都得逐字抄。`exit(1)`。
fn usage() -> ! {
    support::warn(
        concat!(
            "usage: cksum [-n] [-a algorithm [-ptx] [-s string]] [-o 1|2]\n",
            "             [file ... | -c [-w] [sumfile]]\n",
            "       sum [-n] [-a algorithm [-ptx] [-s string]] [-o 1|2]\n",
            "           [file ... | -c [-w] [sumfile]]\n",
            "       md2 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
            "       md4 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
            "       md5 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
            "       rmd160 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
            "       sha1 [-nptx] [-s string] [file ... | -c [-w] [sumfile]]\n",
        )
        .as_bytes(),
    );
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

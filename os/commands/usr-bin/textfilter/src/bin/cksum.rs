//! Minix-RS cksum — checksums and byte counts.
//!
//! Ground truth: `minix3/usr.bin/cksum/` (NetBSD). The deciding half —
//! the POSIX CRC, the two historic sums, the block-rounding of the printed
//! length, and the `%lu %lld` output format (`print.c` `pcrc`/`psum1`/`psum2`)
//! — is the library's `cksum` module. This program reads each file operand (or
//! standard input when none) and prints `{crc} {length}` plus ` {name}` for a
//! file. `-a` 的摘要那条腿现在走 `minix_textfilter::digest` 的真本体（MD5/SHA1/
//! SHA256 三族），报文形是 C 的 `MD5 (file) = <digest>` 与它的 `-n`/`-q` 两个变体；
//! 其余五枚算法、`-s`/`-t`/`-x` 三条腿与 hash 在里的 `-c` 校验腿仍在占位句上。

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
    CheckEntry, CksumError, CksumRequest, SumKind, cksum_crc, csum1, csum2, parse_check_line,
    parse_cksum_args, sum_display_length,
};
use minix_textfilter::digest;

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

    if let Some(name) = request.hash {
        // 选中了算法：摘要本体已经接了三族（`src/digest.rs`），剩下五枚与 `-s`/`-t`/`-x`
        // 三条腿、以及 hash 在里的 `-c` 校验腿仍在 `CKSUM-HASH-*` 的后一片。
        support::terminate(hash_run(&request, name));
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

/// C 的 `hash` 那条路：`cksum.c:474-491` 的循环体加上 `hash_digest_file`（`:506-524`）。
/// 有操作数就逐档印一行摘要，没有操作数就把标准输入读尽——`-p` 时先把字节原样转抄到
/// 标准输出——末尾印那一行摘要（`md5.c` 的 `MD5Filter`，摘要那行恒在转抄之后）。
///
/// 三条形状在 C 里由 `print_flags` 决定，`-q` 赢 `-n`（`cksum.c:515-520` 是
/// `if QUIET … else if NORMAL … else …`）：
///
/// * 缺省 `MD5 (file) = <digest>`；
/// * `-n` `<digest> file`；
/// * `-q` 只有 `<digest>`。
///
/// 退路三条，都落在批次三百零七定下的占位句上（退 2）：算法本体未接的那五枚
///（`MD2`/`MD4`/`RMD160`/`SHA384`/`SHA512`）、`-s`/`-t`/`-x` 三条腿、以及 hash 在里的
/// `-c` 校验腿——C 在那条腿里用 `hash->filefunc` 重算比对（`cksum.c:421-441`），并在
/// 失败行前缀 `(MD5) `（`:451-456`），那是另一片报文形。
///
/// 读档走的是 `support::read_file`/`support::read_stdin` 这一对 cfg 双生面，不是本支
/// 另一条腿上那个直接 `minix_sys` 的 `read_all`：后者在宿主上开不了也读不到（现跑
/// `printf 'abc' | cksum` → `4294967295 0`，即零字节；`cksum <真实档>` → `cksum: <档名>`
/// 退 1），摘要若走那条缝，宿主门就只能钉住空输入的摘要。已登记 `CKSUM-HOSTED-READ-SEAM`。
fn hash_run(request: &CksumRequest<'_>, name: &'static str) -> i32 {
    let Some(algo) = digest::Algo::from_hashname(name) else {
        not_modelled();
    };
    if request.do_check || request.nohash_stdin {
        not_modelled();
    }

    if request.operands.is_empty() {
        // 读失手在 C 那边就是「读到尽头」（`MD5Filter` 的 `while ((len = fread(…)) > 0)`
        // 不看 `ferror`），本壳的 `read_stdin` 同一口径，所以这里没有错腿可报。
        let input = support::read_stdin();
        if request.pipe {
            support::emit(&input);
        }
        support::emit(format!("{}\n", algo.hex_digest(&input)).as_bytes());
        return 0;
    }

    let mut status = 0;
    for operand in request.operands.iter().copied() {
        let Ok(input) = support::read_file(operand) else {
            // C 的 `filefunc` 交回 `NULL` 时 `hash_digest_file` 退 1，`main` 于是
            // `warn("%s", fn)` 并把整体 `rval` 置 1（`cksum.c:477-483`），不印摘要行。
            // 那句 `warn` 的第三段（`: strerror`）在 `CKSUM-FILE-LEGS` 那一行里对账。
            support::warn(format!("cksum: {operand}\n").as_bytes());
            status = 1;
            continue;
        };
        let digest = algo.hex_digest(&input);
        let line = if request.print_quiet {
            format!("{digest}\n")
        } else if request.print_normal {
            format!("{digest} {operand}\n")
        } else {
            format!("{name} ({operand}) = {digest}\n")
        };
        support::emit(line.as_bytes());
    }
    status
}

/// 本壳的占位报文：`CKSUM-HASH-*` 后一片的三条退路共用它，退码沿用批次三百零七定下的 2。
fn not_modelled() -> ! {
    support::warn(b"cksum: -a crypto algorithms are not modelled yet\n");
    support::terminate(2);
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

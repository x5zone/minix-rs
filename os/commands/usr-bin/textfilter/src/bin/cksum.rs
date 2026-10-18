//! Minix-RS cksum — checksums and byte counts.
//!
//! Ground truth: `minix3/usr.bin/cksum/` (NetBSD). The deciding half —
//! the POSIX CRC, the two historic sums, the block-rounding of the printed
//! length, and the `%lu %lld` output format (`print.c` `pcrc`/`psum1`/`psum2`)
//! — is the library's `cksum` module. This program reads each file operand (or
//! standard input when none) and prints `{crc} {length}` plus ` {name}` for a
//! file. `-a` 的摘要那条腿走 `minix_textfilter::digest` 的真本体（八枚算法全接），报文形是
//! C 的 `MD5 (file) = <digest>` 与它的 `-n`/`-q` 两个变体；`-s`/`-t`/`-x` 三条自印腿按 argv
//! 次序在诊断之前排出去（判据与读数见 `emit_action`）；hash 在里的 `-c` 校验腿按 C 的
//! `do_check` 逐行重算比对（见 `check_files`）。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_sys::{close, open, read};
use minix_textfilter::cksum::{
    cksum_crc, classify_check_line, csum1, csum2, leading_u32, parse_cksum_args_into,
    sum_display_length, CheckLine, CksumAction, CksumError, CksumRequest, SumKind,
};
use minix_textfilter::digest::{self, Algo};
use minix_textfilter::GetoptReport;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    // `argv[0]` is the program name; C's getopt loop starts at `argv[1]`.
    let mut actions: Vec<CksumAction> = Vec::new();
    let parsed = parse_cksum_args_into(&refs[1..], &mut actions);
    // C 的 `-s`/`-t`/`-x` 三条腿在 getopt 环**里**就把话印完（`cksum.c:255-274`），后面
    // 选项报错追不回来：参照件 e-1/e-2/e-3 三形的 stdout 都有那一行，退码仍是 1。
    // 所以这一趟在诊断之前，不在 `hash_run` 里。
    for action in &actions {
        emit_action(action);
    }
    let request = match parsed {
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

    if request.do_check {
        // C 的 `if (do_check)` 整块在计算那支**之前**（`cksum.c:283`），所以 `-a md5 -c 档`
        // 走的是校验腿；`-s`/`-t`/`-x` 已经在选项环那趟把各自的行印出去了。
        // `-c` 只吃第一个操作数当校验档（多余的忽略），对位 C 的 `argv[0]`-或-标准输入。
        support::terminate(check_files(
            &request.operands,
            request.check_warn,
            request.hash,
        ));
    }

    if let Some(algo) = request.hash {
        // 选中了算法：八枚的本体与 `-s`/`-t`/`-x` 三条腿都已接成真读数（`src/digest.rs` 与
        // 上面的 `emit_action`），这一腿只剩摘要本体。
        support::terminate(hash_run(&request, algo));
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
/// 只剩一条退路落在批次三百零七定下的占位句上（退 2）：hash 在里的 `-c` 校验腿——C 在
/// 那条腿里用 `hash->filefunc` 重算比对（`cksum.c:421-441`），并在失败行前缀 `(MD5) `
///（`:451-456`），那是另一片报文形（`CKSUM-HASH-STATE` 的收尾片）。`-s`/`-t`/`-x` 三条
/// 自印腿已经接成真读数，它们的行在 `emit_action` 里。
///
/// 读档走的是 `support::read_file`/`support::read_stdin` 这一对 cfg 双生面，不是本支
/// 另一条腿上那个直接 `minix_sys` 的 `read_all`：后者在宿主上开不了也读不到（现跑
/// `printf 'abc' | cksum` → `4294967295 0`，即零字节；`cksum <真实档>` → `cksum: <档名>`
/// 退 1），摘要若走那条缝，宿主门就只能钉住空输入的摘要。已登记 `CKSUM-HOSTED-READ-SEAM`。
fn hash_run(request: &CksumRequest<'_>, algo: Algo) -> i32 {
    if request.operands.is_empty() {
        // C 的 `else if (hash && !nohashstdin)`（`cksum.c:489`）：`-s`/`-t`/`-x` 任一出现过，
        // 这一腿就整个跳过——它们自己的行已经在诊断之前由 `emit_action` 排出去了
        //（参照件 s-10 形：`-a md5 -p -s abc` 只有那一行串摘要，既不转抄也不摘要标准输入）。
        if request.nohash_stdin {
            return 0;
        }
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
            format!("{} ({operand}) = {digest}\n", algo.hashname())
        };
        support::emit(line.as_bytes());
    }
    status
}

/// C 的 `MD5String` 那一行（`minix3/usr.bin/cksum/md5.c:60-67`）：`NAME ("<串>") = <摘要>`。
/// 串是**原样回显**的，不转义也不加别的标点（参照件 s-2 带空格、s-3 空串两形同此形）；
/// 这一行不受 `-n`/`-q`/`-p` 影响，因为 `printf` 的格式串在 `md5.c` 里写死，`print_flags`
/// 只在 `hash_digest_file`（`cksum.c:506-524`）那条腿里被读。
fn string_line(algo: Algo, text: &str) -> String {
    format!(
        "{} (\"{text}\") = {}\n",
        algo.hashname(),
        algo.hex_digest(text.as_bytes())
    )
}

/// 排出一条自印腿的字节。三条腿各印几行、行首是什么，判据都在 `md5.c` 的那三个函数里，
/// 八枚算法共用同一份（`digest::TEST_SUITE` 的注释记了为什么共用）。
fn emit_action(action: &CksumAction<'_>) {
    match action {
        CksumAction::String(algo, text) => support::emit(string_line(*algo, text).as_bytes()),
        CksumAction::TestSuite(algo) => {
            support::emit(format!("{} test suite:\n", algo.hashname()).as_bytes());
            for text in digest::TEST_SUITE {
                support::emit(string_line(*algo, text).as_bytes());
            }
        }
        CksumAction::TimeTrial(algo) => {
            // C 分两次写：先印这一句并 `fflush`（`md5.c:81-83`），再开始折叠，折完才补
            // ` done` 与那三行。这里也分两次，不拼成一整串——`Time`/`Speed` 两行的值
            // 本来就只能在折叠之后才知道。
            support::emit(
                format!(
                    "{} time trial.  Digesting {} {}-byte blocks ...",
                    algo.hashname(),
                    digest::TIME_TRIAL_BLOCK_LEN,
                    digest::TIME_TRIAL_BLOCK_COUNT
                )
                .as_bytes(),
            );
            let start = support::wall_seconds();
            let hex = algo.hex_digest(&digest::time_trial_data());
            let elapsed = support::wall_seconds() - start;
            let total = (digest::TIME_TRIAL_BLOCK_LEN * digest::TIME_TRIAL_BLOCK_COUNT) as i64;
            // C 那句 `(endTime - startTime) != 0 ? (endTime - startTime) : 1`：一秒内跑完
            // 就按 1 秒算，所以 1000×1000 字节的读数恒为 `Speed = 1000000 bytes/second`。
            let divisor = if elapsed != 0 { elapsed } else { 1 };
            support::emit(
                format!(
                    " done\nDigest = {hex}\nTime = {elapsed} seconds\nSpeed = {} bytes/second\n",
                    total / divisor
                )
                .as_bytes(),
            );
        }
    }
}

/// `cksum -c`：C 的 `do_check`（`cksum.c:283-465`）逐行验档。行为三条：
/// 一行全对就**一声不出**；判失败就打 `<档>: FAILED`（`hash` 在里时前缀 `(<NAME>) `，`:454`），
/// 整趟里只要有一行失败（或因格式失败）就退 1。
///
/// `hash` 状态是**跨行留存**的：C 在遇到 `NAME (档) = 摘要` 这一形时把 `hash` 指过去
///（`hash = nhash`，`:363`），此后同一档里那些没有 `(` 的普通行也走摘要那一支——
/// 参照件 h-1/h-3/h-4 三形实测都因此判失败，其中 h-4 根本没给 `-a`，算法是档里第一行
/// 的行首前缀自己认的。所以本函数带着 `hash` 走一遍循环，而不是一行一判。
///
/// 摘要那一支读不到档时 C 走的是 `access(filename, R_OK) == 0` 那半条短路（`:424`），
/// 失败也只打 `(MD5) <档>: FAILED`——**不** warn 那一句，与 crc 那支的 `warn("%s", filename)`
///（`:432`）分叉（参照件 c-5/h-13 两形实测）。crc 那支读失手时的 warn 照 C 只在 `-w` 下打。
///
/// 三条格式腿的句子各是各的：`unknown hash: <整行>`（`:358`，只在行首前缀认不出时）、
/// `bogus format: <整行>. Skipping...`（`:325` 与 `:398` 两处同句）、
/// `no filename in <整行>? Skipping...`（`:381` 与 `:409`）。三句都只在 `-w` 下打
///（C 的 `if (check_warn)` 包着每一句），但**都记一次失败**（`rval = 1`）。
fn check_files(operands: &[&str], check_warn: bool, selected: Option<Algo>) -> i32 {
    let from_operand = !operands.is_empty();
    let path = operands.first().copied().unwrap_or("");
    let bytes = if from_operand {
        match support::read_file(path) {
            Ok(bytes) => bytes,
            Err(_) => {
                // 校验档本身打不开在 C 是 `err(1, "Cannot read %s", …)`（`:302-303`）——
                // 那句的第三段（`: <strerror>`）归 A5b `CKSUM-FILE-LEGS`。
                support::warn(format!("cksum: Cannot read {path}\n").as_bytes());
                return 1;
            }
        }
    } else {
        support::read_stdin()
    };
    let Ok(text) = core::str::from_utf8(&bytes) else {
        return 1;
    };
    // C 用 `fgets` 逐行读，所以末尾那个换行 terminates 掉最后一行、不产出一枚幻影空行；
    // 只剥这一个。
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut hash = selected;
    let mut failed = false;
    if !body.is_empty() {
        for line in body.split('\n') {
            match classify_check_line(line, hash) {
                CheckLine::Hash {
                    algo: None,
                    name,
                    digest,
                } => {
                    // C 认不出行首前缀就整行不作数，报一句 `unknown hash`（`-w` 下）并记失败；
                    // 它**不**打 FAILED 行（参照件 c-3/c-4 两形：stdout 空）。
                    let _ = (name, digest);
                    if check_warn {
                        support::warn(format!("cksum: unknown hash: {line}\n").as_bytes());
                    }
                    failed = true;
                }
                CheckLine::Hash {
                    algo: Some(algo),
                    name,
                    digest,
                } => {
                    hash = Some(algo);
                    if !verify_digest(algo, name, digest) {
                        support::emit(format!("({}) {name}: FAILED\n", algo.hashname()).as_bytes());
                        failed = true;
                    }
                }
                CheckLine::Malformed => {
                    if check_warn {
                        support::warn(
                            format!("cksum: bogus format: {line}. Skipping...\n").as_bytes(),
                        );
                    }
                    failed = true;
                }
                CheckLine::NoFilename => {
                    if check_warn {
                        support::warn(
                            format!("cksum: no filename in {line}? Skipping...\n").as_bytes(),
                        );
                    }
                    failed = true;
                }
                CheckLine::Token { token, name } => {
                    let ok = match hash {
                        // `hash` 已选中：拿整串 token 与重算的摘要逐字比（`:424-426` 的 `strcmp`），
                        // 读不到档也只算失败、不 warn（参照件 h-13 形）。
                        Some(algo) => verify_digest(algo, name, token),
                        // crc 那支：第一个词按 `strtoul` 取前导十进制（`:442`），档读不到时
                        // 在 `-w` 下 warn 那一句（`:432`）。
                        None => match support::read_file(name) {
                            Err(_) => {
                                if check_warn {
                                    support::warn(format!("cksum: {name}\n").as_bytes());
                                }
                                false
                            }
                            Ok(data) => cksum_crc(&data).0 == leading_u32(token),
                        },
                    };
                    if !ok {
                        match hash {
                            Some(algo) => support::emit(
                                format!("({}) {name}: FAILED\n", algo.hashname()).as_bytes(),
                            ),
                            None => support::emit(format!("{name}: FAILED\n").as_bytes()),
                        }
                        failed = true;
                    }
                }
            }
        }
    }
    i32::from(failed)
}

/// 把 `name` 这档读尽、按 `algo` 重算摘要，与档里记的 `expected` 逐字比。读不到就是失败：
/// C 那一侧要么被 `access` 挡在门外（`:424`），要么 `filefunc` 交回 `NULL`——它把 `NULL`
/// 直接喂给 `strcmp`，于是那一条会当场崩（`:425`），本壳按「失败」收，不跟着崩。
fn verify_digest(algo: Algo, name: &str, expected: &str) -> bool {
    match support::read_file(name) {
        Ok(data) => algo.hex_digest(&data) == expected,
        Err(_) => false,
    }
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

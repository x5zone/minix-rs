//! Minix-RS primes — prime generation over an interval.
//!
//! Ground truth: `minix3/games/primes/primes.c`, `primes [-dh] [start [stop]]`.
//! The library caps the interval at `MAX_BOUND` (1,000,000 — the trial
//! division engine is built for short lived runs, not the C streaming
//! sieve), so with no arguments the interval is 2 to the cap instead of the
//! C default near 2^32; one argument starts there, two bound it.
//!
//! `stop` is an **exclusive** bound — C says so in its own header
//! (`primes.c:49`, "Print primes >= start and < stop") and the reference
//! process agrees: `primes 5 13` stops at 11, `primes 2 2` prints nothing.
//! The shell expresses that by lowering the sieve's inclusive upper bound by
//! one; `sieve` itself stays the generic inclusive primitive.
//!
//! The diagnostic face is wired to C leg by leg (`primes.c:133-172`): the
//! negative check and the two conversion kinds are different lines in C, and
//! which family each belongs to is not interchangeable —
//! `err(1, "%s", argv[i])` when `strtoumax` set `errno` (`_strtol.h:69` `EINVAL`,
//! `:157`/`:172` `ERANGE`) prints `primes: <串>: <句子>`, while
//! `errx(1, "%s: illegal numeric format.")` for trailing junk
//! (`primes.c:140`/`:147`/`:160`) and the size and ordering refusals
//! (`:149`, `:172`) print a phrase and no sentence. `usage()` here is a
//! hardcoded literal (`:339-343`) — unlike `rm`/`cp`/`mv`, this one does not
//! carry `getprogname()`.
//!
//! One leg of C's own remains unwired and is declared rather than hidden: option
//! *permutation* — C's getopt moves an option letter found after an operand back into
//! the option scan, which this shell's single-pass parser does not (the separate
//! registered item `7g`). The zero-operand path (`read_num_buf`, `:176-205`) and the
//! base-0 spellings (`0x`/octal/leading blanks/leading `+`) are wired since this batch.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::vec;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::primes::{
    BoundError, MAX_BOUND, NumVerdict, TABLE_LAST_PRIME, judge_number_line, over_cap, parse_bound,
    previous_prime, sieve,
};
use minix_sys::Errno;
use support::terminate;

/// C 的 `errx(1, …)` 那一族：程序名、短语、不缀句子，写完退 1。
fn errx(prog: &str, phrase: &str) -> ! {
    support::warn(format!("{prog}: {phrase}\n").as_bytes());
    terminate(1)
}

/// C 的 `err(1, "%s", argv[i])`：同一个形状，但缀上 `strtoumax` 留下的那个号。
/// 本命令能置起 errno 的只有一种——`ERANGE`（`_strtol.h:157`/`:172`）。「一个数字也转不出」
/// 不置号（`_strtol.h:69` 的 `EINVAL` 是 base 非法分支，base 恒 0 到不了），所以那形落
/// `errx` 的「illegal numeric format」而不是这里。批次二百五十二把两者混为一支，本批改正。
fn err_erange(prog: &str, word: &str) -> ! {
    support::warn_line(prog, word, Errno::from_i32(ERANGE));
    terminate(1)
}

/// C 的 `err(1, "stdin")`：同一形状，但号不是解析留下的，而是 `ferror(stdin)` 那个
/// （`primes.c:187-190`）。与上面那个助手的区别只在号的来路，报文形状与 C 吻合。
fn err_errno_code(prog: &str, label: &str, code: Errno) -> ! {
    support::warn_line(prog, label, code);
    terminate(1)
}

/// `minix3/sys/sys/errno.h` 的 `ERANGE`（值 34，「数值结果过大」）。`Errno` 未带这个
/// 名的关联常量，所以沿用 `rm.rs` 对 `ETXTBSY`、本文件对 `MAX_BOUND` 的先例：壳侧定值再换算。
const ERANGE: i32 = 34;

/// `primes.c:339-343`：一行硬写 usage，不用 `getprogname()`。
fn usage() -> ! {
    support::warn(b"usage: primes [-dh] [start [stop]]\n");
    terminate(1)
}

/// 把一支边界按 C 的三条腿拆开：负号、转换失手、尾字符。
fn bound_of(prog: &str, word: &str) -> u64 {
    if word.starts_with('-') {
        // `primes.c:133`/`:155`：`argv[i][0] == '-'` 先拒，轮不到 strtoumax。
        // 注意 C 的选项扫描在前，所以只有落在操数里的连字符才走这一支。
        errx(prog, "negative numbers aren't permitted.");
    }
    match parse_bound(word) {
        Ok(value) => value,
        // `primes.c:140`/`:147`/`:160`：`*p != '\0'`——尾字符与「一个位也没转出」同属这支
        // （真源把 `endptr` 留在原处）。句末的句点是 C 自己的。
        Err(BoundError::TrailingJunk) => errx(prog, &format!("{word}: illegal numeric format.")),
        // `primes.c:138`/`:145`：只有 `errno` 真被置起（ERANGE）才缀句子。
        Err(BoundError::OutOfRange) => err_erange(prog, word),
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C 也不叫 `setprogname`；`getprogname()` 由 `crt0-common.c:156-165` 从 `argv[0]`
    // 起好那一段，`err`/`errx` 都以它开头。
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // `primes.c:105-117` 的 `getopt(argc, argv, "dh")`：`-d` 与 `-h` 都是计数器
    // （`dflag++`/`hflag++`），所以 `-d`、`-h`、`-dh`、`-hd`、`-dd`、`-d -h` 全都算数；
    // 不认得的字母落 `case '?': default: usage()`。裸 `-` 在 getopt 里是操数不是选项，
    // 所以这里只吃带字母的串。选项 permute（C 的 getopt 会把操数后面的选项也抽走）
    // 是台账里的另一件（`7g`），本轮不顺手扩。
    let mut dflag = false;
    let mut hflag = false;
    let mut cursor = 1usize;
    while let Some(word) = args.get(cursor) {
        let Some(letters) = word.strip_prefix('-').filter(|letters| !letters.is_empty()) else {
            break;
        };
        for letter in letters.bytes() {
            match letter {
                b'd' => dflag = true,
                b'h' => hflag = true,
                _ => usage(),
            }
        }
        cursor += 1;
    }
    let operands = &args[cursor..];

    if operands.len() > 2 {
        // `primes.c:167-168`：`default: usage()`。
        usage();
    }

    let (lo, hi) = match operands {
        [] => {
            // C 的 `case 0: start = read_num_buf();`（`primes.c:165-166`）：从标准输入一行行
            // 取起始值。四条腿的判据都在决定半的 `judge_number_line`（可纯测），本支只做读与写。
            // 缓冲尺寸：C 用 `buf[LINE_MAX]`，本树取 256（与同包其他壳的定长行同口径）；
            // 单行超过它时本层会拆成两次读（与 C 的一次读完不同，已登记）。
            let mut input = [0u8; 256];
            let mut reader = support::LineReader::new(support::stdin_chunk, &mut input);
            let start = loop {
                // 两种收尾在 C 里不是一回事：`fgets` 返 NULL 且 `ferror(stdin)` 才算错，走
                // `err(1, "stdin")`（`:187-190`）；只是读到尽头则 `exit(0)`，一声不出。
                let has_line = match reader.next_line() {
                    Ok(has_line) => has_line,
                    Err(code) => err_errno_code(prog, "stdin", code),
                };
                if !has_line {
                    terminate(0);
                }
                // 下面的报文字段都用整块 `buf`（含结尾换行），所以 C 的那两行会断成两行。
                let buf = core::str::from_utf8(reader.line()).unwrap_or("");
                match judge_number_line(buf) {
                    NumVerdict::SkipLine => continue,
                    NumVerdict::Negative => errx(prog, "negative numbers aren't permitted."),
                    NumVerdict::RangeError => err_erange(prog, buf),
                    NumVerdict::TrailingJunk(whole) => {
                        errx(prog, &format!("{whole}: illegal numeric format."))
                    }
                    NumVerdict::Start(value) => break value,
                }
            };
            if over_cap(start) {
                // C 不查起始值（只查 `stop > SPSPMAX`，`:149`）；本树的筛子接不了超限值，
                // 只能在这里拦，报文形与操数那两支同形——本树上限与 C 默认上界的差异已声明。
                errx(
                    prog,
                    &format!("{start}: stop value too large (>{}).", MAX_BOUND),
                );
            }
            (start as u32, MAX_BOUND)
        }
        [start] => {
            let lo = bound_of(prog, start);
            if over_cap(lo) {
                // C 只对 `stop` 查上限（`:149`）；`start` 超本树上限时只能在这里拦，
                // 因为下面的筛子接不了它——报文同形，差别已在 22 篇登记。
                errx(
                    prog,
                    &format!("{start}: stop value too large (>{}).", MAX_BOUND),
                );
            }
            (lo as u32, MAX_BOUND)
        }
        [start, stop] => {
            let lo = bound_of(prog, start);
            let hi = bound_of(prog, stop);
            if over_cap(hi) {
                // `primes.c:149`：只有 stop 有这一支，括号里是那个上限值。
                errx(
                    prog,
                    &format!("{stop}: stop value too large (>{}).", MAX_BOUND),
                );
            }
            if over_cap(lo) {
                errx(
                    prog,
                    &format!("{start}: stop value too large (>{}).", MAX_BOUND),
                );
            }
            if lo > hi {
                // `primes.c:171-172`：C 比的是**原始**两个值，所以 `lo == hi` 是合法的
                // （它不报错，只是什么都不印——下面那一降就把它交给了空区间）。
                errx(prog, "start value must be less than stop value.");
            }
            // C 的上界是开区间（`:49` 头注 + 参照进程实测），`sieve` 的上界是闭区间，
            // 所以在这里降 1；`stop == 0` 时降到 0，与 `lo > hi` 一同落在空输出上。
            (lo as u32, (hi as u32).saturating_sub(1))
        }
        // 不可达：三支之外的操数已经在上面 `usage()` 掉了（编译器需要这一支才认完备）。
        _ => usage(),
    };
    // Largest prime count below MAX_BOUND is pi(1_000_000) = 78,498.
    let mut found = vec![0u32; 78_498];
    let count = match sieve(lo, hi, &mut found) {
        Ok(count) => count,
        Err(_) => terminate(1),
    };
    // C 的两条打印路对「第一个素数有没有前驱」答案不同（形状与实测见 `TABLE_LAST_PRIME`）：
    // 起始值落在存表范围内时差值列无条件印（`primes.c:256-258`），表外走筛子路时
    // 守卫只剩 `prev != 0`，即第一个素数不带列（`:328`）。
    let uses_table = (lo as u64) <= TABLE_LAST_PRIME as u64;
    let mut printed: Option<u32> = None;
    let mut out = [0u8; 12];
    for &prime in &found[..count] {
        // `-h` 只改**素数本身**的进制；差值列在 C 里永远是十进制的 `PRIu64`
        // （`:257`/`:329`）。参照进程给得出这一分：`primes -dh 113 130` 打
        // `71 (4)` 与 `7f (14)`——素数走十六进制（113→`71`、127→`7f`），而那个差值印 `14` 而不是 `e`。
        let len = if hflag {
            support::utox(prime, &mut out)
        } else {
            support::utoa(prime, &mut out)
        };
        support::emit(&out[..len]);
        if dflag {
            let difference = match printed {
                Some(before) => Some(prime - before),
                None if uses_table => Some(prime - previous_prime(prime)),
                None => None,
            };
            if let Some(difference) = difference {
                support::emit(b" (");
                let dlen = support::utoa(difference, &mut out);
                support::emit(&out[..dlen]);
                support::emit(b")");
            }
        }
        support::emit(b"\n");
        printed = Some(prime);
    }
    terminate(0);
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

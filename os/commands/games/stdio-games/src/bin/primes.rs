//! Minix-RS primes — prime generation over an interval.
//!
//! Ground truth: `minix3/games/primes/primes.c`, `primes [start [stop]]`.
//! The library caps the interval at `MAX_BOUND` (1,000,000 — the trial
//! division engine is built for short lived runs, not the C streaming
//! sieve), so with no arguments the interval is 2 to the cap instead of the
//! C default near 2^32; one argument starts there, two bound it. Output is
//! one prime per line.
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
//! Three legs of C's own remain unwired, each declared rather than hidden:
//! the `-d` (difference) and `-h` (hexadecimal) output flags
//! (`:107-112`, used at `:255-256` and `:327-328`) — an option letter we do
//! not implement is reported as not wired, mirroring the other shells; C's
//! zero-operand path, which reads the start value from standard input
//! (`read_num_buf`, `:176-205`, with its own four legs); and the base-0
//! spellings (`0x`/octal prefixes, leading blanks, explicit `+`) that
//! `strtoumax` would take.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::vec;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_stdio_games::primes::{BoundError, MAX_BOUND, over_cap, parse_bound, sieve};
use minix_sys::Errno;
use support::terminate;

/// C 的 `errx(1, …)` 那一族：程序名、短语、不缀句子，写完退 1。
fn errx(prog: &str, phrase: &str) -> ! {
    support::warn(format!("{prog}: {phrase}\n").as_bytes());
    terminate(1)
}

/// C 的 `err(1, "%s", argv[i])`：同一个形状，但缀上 `strtoumax` 留下的那个号
/// （`EINVAL` 或 `ERANGE`，`minix3/sys/sys/errno.h`）。
fn err_errno(prog: &str, word: &str, kind: BoundError) -> ! {
    let code = match kind {
        BoundError::NoConversion => Errno::EINVAL,
        BoundError::OutOfRange => Errno::from_i32(ERANGE),
        // 不可达：尾字符那一支在 C 里 `errno` 未置，走的是 `errx`。
        BoundError::TrailingJunk => Errno::EINVAL,
    };
    support::warn_line(prog, word, code);
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
        // `primes.c:140`/`:147`/`:160`：转出了数字而后面还有字符（此时 `errno` 未置）。
        // 句末的句点是 C 自己的。
        Err(BoundError::TrailingJunk) => errx(prog, &format!("{word}: illegal numeric format.")),
        Err(kind) => err_errno(prog, word, kind),
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C 也不叫 `setprogname`；`getprogname()` 由 `crt0-common.c:156-165` 从 `argv[0]`
    // 起好那一段，`err`/`errx` 都以它开头。
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // `primes.c:105-117` 的 `getopt("dh")`：本树没有那两个旗标的输出面，所以开头的
    // 选项串一支就得出结果——认得的字母报拒收，不认得的按 C 的 `?`→`default: usage()`。
    // 一次就止，也意味着后面的项全是操数（与 C 的差别仅在「多个选项串」这一形，已登记）。
    if let Some(letters) = args
        .get(1)
        .filter(|word| word.len() > 1)
        .and_then(|word| word.strip_prefix('-'))
    {
        if letters
            .bytes()
            .any(|letter| letter != b'd' && letter != b'h')
        {
            usage();
        }
        errx(
            prog,
            "-d/-h not wired (difference column, hexadecimal output)",
        );
    }
    let operands = &args[1..];

    if operands.len() > 2 {
        // `primes.c:167-168`：`default: usage()`。
        usage();
    }

    let (lo, hi) = match operands {
        // C 在这一支从标准输入读数（`read_num_buf`，`:176-205`）；本树未接那条腿，
        // 区间退到 2..上限，已列入头注的声明差。
        [] => (2, MAX_BOUND),
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
                // `primes.c:171-172`。
                errx(prog, "start value must be less than stop value.");
            }
            (lo as u32, hi as u32)
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
    let mut out = [0u8; 12];
    for &prime in &found[..count] {
        let len = support::utoa(prime, &mut out);
        support::emit(&out[..len]);
        support::emit(b"\n");
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

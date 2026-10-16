//! Minix-RS `nl` — the doing half over `minix_textfilter::nl`.
//!
//! Ground truth: `minix3/usr.bin/nl/nl.c`. Numbering, formats and the page
//! delimiter are the library (`nl_lines`); this program parses the flags,
//! reads the file operands (standard input when none), strips newlines into
//! lines, and emits the numbered result.
//!
//! Wired: the three logical-page sections with their numbering types via
//! `-b`/`-h`/`-f` (a/t/n), `-v`/`-i` (start, step), `-w` (number field width),
//! `-l` (blank-run grouping for `-b a`, C `nblank`), `-n ln|rn|rz` (format),
//! `-p` (do not restart numbering at a page delimiter), `-d` (set the 2-byte
//! page delimiter, `\:` by default) and `-s` (set the separator bytes between
//! the number field and the text). Declared residual: `number_regex` (`-b p…`) —
//! requesting it reports "option not wired" and exits 1.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minix_textfilter::GetoptReport;
use minix_textfilter::nl::{NlOptions, NumFormat, NumberType, nl_lines};

// 标准输入的读缝走 `bin_support::read_stdin()`（宿主走 `std::io::stdin`、真机走
// `minix_sys::read`）。本文件此前自带一个只调裸 `read(0)` 的本地版本，宿主上读不到任何东西
// ——这正是新增端到端门当场暴露的那件事（同批次二百二十四给 games 收过的坑）。

/// C 的 `errx(EXIT_FAILURE, …)`：一行 `<程名>: <句子>`、**不接 usage**、退 1。
fn errx(prog: &str, sentence: &str) -> ! {
    support::warn(format!("{prog}: {sentence}\n").as_bytes());
    support::terminate(1)
}

/// C 的 `usage()`（`nl.c:400-405`）：两行，第一行末尾是 `\n\t`；`exit(EXIT_FAILURE)`。
fn usage(prog: &str) -> ! {
    support::warn(
        format!(
            "Usage: {prog} [-p] [-b type] [-d delim] [-f type] [-h type] [-i incr] [-l num]\n\t[-n format] [-s sep] [-v startnum] [-w width] [file]\n"
        )
        .as_bytes(),
    );
    support::terminate(1)
}

/// libc `getopt` 自己那一行之后**再接** usage（`getopt.c:101-104` 集外字母、
/// `:132-136` 带值选项缺值；两句都递 `getprogname()`）。这是本族共同的形状：
/// 一句话属于 libc，命令自己的 usage 跟在后面。
fn getopt_leg(report: GetoptReport, prog: &str) -> ! {
    support::warn(report.line(prog).as_bytes());
    usage(prog)
}

/// C 的 `strtol(optarg, &ep, 10)` + `if (*ep != '\0')`：跳前导空白、吃一个符号、
/// 只收十进制数字，**末尾剩任何东西（含空白）都算错**。溢出饱和到 `i64` 端点，
/// 与 C 的 `LONG_MIN/LONG_MAX && errno` 那一项同效（都是拒收）。
fn strtol10(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        pos += 1;
    }
    let mut negative = false;
    if pos < bytes.len() && matches!(bytes[pos], b'+' | b'-') {
        negative = bytes[pos] == b'-';
        pos += 1;
    }
    let first = pos;
    let mut acc: i64 = 0;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        let digit = i64::from(bytes[pos] - b'0');
        // 溢出即拒（C 那里是 `LONG_MIN/LONG_MAX && errno != 0` 那一项，同样报错）。
        acc = acc.checked_mul(10)?.checked_add(digit)?;
        pos += 1;
    }
    if pos == first || pos != bytes.len() {
        return None;
    }
    Some(if negative { -acc } else { acc })
}

/// C 的 `strtoul(optarg, &ep, 10)`（`-l` 那一支）：不接受符号，其余同上。
fn strtoul10(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        pos += 1;
    }
    let first = pos;
    let mut acc: u64 = 0;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        acc = acc
            .checked_mul(10)?
            .checked_add(u64::from(bytes[pos] - b'0'))?;
        pos += 1;
    }
    if pos == first || pos != bytes.len() {
        return None;
    }
    Some(acc)
}

/// 整个程序体；两种 `main` 都调它，它不返回。
fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut opts = NlOptions::default();
    let mut files: Vec<String> = Vec::new();
    let mut index = 1usize;
    // C 的选项串是 `"pb:d:f:h:i:l:n:s:v:w:"`（`nl.c:149`）：只有 `p` 不带值。
    // 逐字母扫，允许聚簇（`-pb`）、值附在本 token（`-ba`）或取下一词（`-b a`），
    // 与 libc `getopt` 同形——旧写法按「整个 token 当一个字母」扫，`-ba` 与 `-pb t`
    // 都会掉进集外字母那支。
    let mut scanning = true;
    while index < args.len() {
        let token = args[index];
        if !scanning || token.len() < 2 || !token.starts_with('-') {
            files.push(token.to_string());
            index += 1;
            continue;
        }
        if token == "--" {
            scanning = false;
            index += 1;
            continue;
        }
        let bytes = token.as_bytes();
        let mut pos = 1usize;
        while pos < bytes.len() {
            let letter = bytes[pos] as char;
            // 取一个选项的值：本 token 剩余部分优先（去掉 getopt 允许的前导空白），
            // 否则取下一词；两处都没有就是 libc 的「option requires an argument」。
            macro_rules! value {
                ($c:literal) => {{
                    let mut rest = &token[pos + 1..];
                    if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => rest = word,
                            // `getopt_leg` 不返回，这里不能写 `return`（否则是 unreachable 表达式）。
                            None => getopt_leg(GetoptReport::MissingValue($c), prog),
                        }
                    } else {
                        rest = rest.trim_start_matches([' ', '\t']);
                    }
                    pos = bytes.len();
                    rest
                }};
            }
            match letter {
                'p' => opts.restart = false,
                'b' | 'f' | 'h' => {
                    // C 的 `parse_numbering`（`:340-…`）只看首字符，其余按整串报出去；
                    // 三段的句子各带自己的名字（`numbering_properties[]` 的 name：
                    // footer / body / header，`nl.c:73-75`）。
                    let value = value!('b');
                    let name = match letter {
                        'b' => "body",
                        'f' => "footer",
                        _ => "header",
                    };
                    let kind = match value.as_bytes().first() {
                        Some(b'a') => NumberType::All,
                        Some(b'n') => NumberType::None,
                        Some(b't') => NumberType::Nonempty,
                        // `p<正则>` 是 C 的另一条腿（`regcomp` + 按正则决定编号），本树尚未接；
                        // 这一形与「非法类型」不同句，故单独拒而不冒充 C 的句子（已登记）。
                        Some(b'p') => errx(prog, "number_regex (-b/-h/-f p…) not wired"),
                        _ => errx(
                            prog,
                            &format!("illegal {name} line numbering type -- {value}"),
                        ),
                    };
                    match letter {
                        'b' => opts.body = kind,
                        'f' => opts.footer = kind,
                        _ => opts.header = kind,
                    }
                }
                'd' => {
                    // C 取前两字符作分隔符，**第三个字符**才报错（`nl.c:157-169`）；
                    // 空值不动默认（两个 `if (optarg[n] != '\0')` 都不成立）。
                    let value = value!('d');
                    let chars = value.as_bytes();
                    match chars.len() {
                        0 => {}
                        1 => opts.delim[0] = chars[0],
                        2 => {
                            opts.delim[0] = chars[0];
                            opts.delim[1] = chars[1];
                        }
                        _ => errx(prog, &format!("invalid delim argument -- {value}")),
                    }
                }
                'i' => {
                    let value = value!('i');
                    match strtol10(value) {
                        // C 这里**没有**非零检查：`incr` 为 0 也照收（后续按 0 递增）。
                        Some(n) if n >= 0 => opts.incr = n as u64,
                        // 决定半的计数器是 `u64`（C 的是 `int`，能倒着数），负增量另成一未接腿，
                        // 不拿 C 的句子冒充——已登记为 `NL-SIGNED-COUNTERS`。
                        Some(_) => errx(prog, "signed numbering counters (-i/-v) not wired"),
                        None => errx(prog, &format!("invalid incr argument -- {value}")),
                    }
                }
                'l' => {
                    let value = value!('l');
                    match strtoul10(value) {
                        Some(n) => opts.nblank = n as usize,
                        None => errx(prog, &format!("invalid num argument -- {value}")),
                    }
                }
                'n' => {
                    let value = value!('n');
                    opts.format = match value {
                        "ln" => NumFormat::Left,
                        "rn" => NumFormat::Right,
                        "rz" => NumFormat::RightZero,
                        _ => errx(prog, &format!("illegal format -- {value}")),
                    };
                }
                's' => {
                    opts.sep = value!('s').as_bytes().to_vec();
                }
                'v' => {
                    let value = value!('v');
                    match strtol10(value) {
                        Some(n) if n >= 0 => opts.startnum = n as u64,
                        Some(_) => errx(prog, "signed numbering counters (-i/-v) not wired"),
                        None => errx(prog, &format!("invalid startnum value -- {value}")),
                    }
                }
                'w' => {
                    let value = value!('w');
                    match strtol10(value) {
                        Some(n) if n > 0 => opts.width = n as usize,
                        // C 的判据是 `if (!(width > 0))`，句子把那个值也打出去（`nl.c:224-228`）；
                        // 而“转不出数字”与“转出来了但 ≤ 0”是两句不同的话，不能合并。
                        Some(n) => errx(prog, &format!("width argument must be > 0 -- {n}")),
                        None => errx(prog, &format!("invalid width value -- {value}")),
                    }
                }
                _ => getopt_leg(GetoptReport::UnknownOption(letter), prog),
            }
            pos += 1;
        }
        index += 1;
    }
    // C 在选项环之后只接受 0 或 1 个操作数，多一个就是 `usage()`（`nl.c:239-249`）；
    // 这一支**不带** libc 那一行（不是 getopt 报的）。
    if files.len() > 1 {
        usage(prog);
    }

    // Read every operand (a `-` names standard input); concatenate in order.
    let mut input: Vec<u8> = Vec::new();
    if files.is_empty() {
        input = support::read_stdin();
    } else {
        for f in &files {
            if f == "-" {
                input.extend(support::read_stdin());
            } else {
                // C 是 `err(EXIT_FAILURE, "Cannot open `%s'", argv[0])`：程序名、**带反引号的
                // 操作数**、`strerror` 句子三件齐备（`nl.c:244-246`），退 1、不接 usage。
                if let Err(code) = support::read_file(f) {
                    support::warn_line(prog, &format!("Cannot open `{f}'"), code);
                    support::terminate(1);
                }
            }
        }
    }

    let mut lines: Vec<&[u8]> = input.split(|&b| b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }

    let out = nl_lines(&lines, &opts);
    let mut rendered = Vec::new();
    for line in &out {
        rendered.extend_from_slice(line);
        rendered.push(b'\n');
    }
    support::emit(&rendered);
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

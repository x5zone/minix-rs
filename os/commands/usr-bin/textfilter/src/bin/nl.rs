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
//! the number field and the text). The option loop runs on the raw argv bytes:
//! the delimiter, the separator and the seven `-- %s` echoes carry those bytes
//! unchanged, because two of them are printed into every output line. Declared
//! residual: `number_regex` (`-b p…`) — requesting it reports "option not
//! wired" and exits 1.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use minix_textfilter::GetoptReport;
use minix_textfilter::nl::{NlOptions, NumFormat, NumberType, nl_lines};

// 标准输入的读缝走 `bin_support::read_stdin()`（宿主走 `std::io::stdin`、真机走
// `minix_sys::read`）。本文件此前自带一个只调裸 `read(0)` 的本地版本，宿主上读不到任何东西
// ——这正是新增端到端门当场暴露的那件事（同批次二百二十四给 games 收过的坑）。

/// C 的 `errx(EXIT_FAILURE, …)`：一行 `<程名>: <句子>`、**不接 usage**、退 1。句子交回的
/// 是**字节**——这一支有七句把 `optarg`（或整个类型串）拓在后面，那一段是 argv 的原字节，
/// 经任何字符解码都会改长度。
fn errx(prog: &str, sentence: &[u8]) -> ! {
    let mut line = Vec::with_capacity(prog.len() + sentence.len() + 3);
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(sentence);
    line.push(b'\n');
    support::warn(&line);
    support::terminate(1)
}

/// 「前缀 + argv 原字节 + 后缀」拼一句。C 那七句 `-- %s` 共用这一个形状，句子各在自己的
/// `errx` 里：`invalid delim argument`（判据 `nl.c:163`、句子 `:165`）、
/// `invalid incr argument`（`:182`）、`invalid num argument`（`:191`）、
/// `illegal format`（`:203`）、`invalid startnum value`（`:214`）、
/// `invalid width value`（`:223`）、`illegal %s line numbering type -- %s`（`:393`，
/// 前缀里还夹着段名，名字来自 `numbering_properties[]` 的 `name`，`:73-75`）。
fn quoted(prefix: &[u8], value: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(prefix.len() + value.len() + suffix.len());
    message.extend_from_slice(prefix);
    message.extend_from_slice(value);
    message.extend_from_slice(suffix);
    message
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
    // 字母位走 `line_bytes`：`getopt.c:79` 的 `optopt = *place++` 一次进一个字节、
    // `:103` 的 `%c` 原样写出，所以 `-é`（`c3 a9`）报的是首字节 `c3`。本壳那一环的取法
    // 也是按字节（`char::from(bytes[pos])`，环整个住在薄壳里），两边同形才成立。
    support::warn(&report.line_bytes(prog));
    usage(prog)
}

/// C 的 `strtol(optarg, &ep, 10)` + `if (*ep != '\0')`：跳前导空白、吃一个符号、
/// 只收十进制数字，**末尾剩任何东西（含空白）都算错**。溢出饱和到 `i64` 端点，
/// 与 C 的 `LONG_MIN/LONG_MAX && errno` 那一项同效（都是拒收）。
fn strtol10(value: &[u8]) -> Option<i64> {
    let bytes = value;
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
fn strtoul10(value: &[u8]) -> Option<u64> {
    let bytes = value;
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
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // `getprogname()` 打的是 `argv[0]` 那一段原字节；只有 `warn_line`（开档失败那句要借
    // `err_line` 的 `&str` 口）与 usage 那一行的程序名需要文本形，各折一次
    //（在册 `ARGV-BYTE-PATH-IN-LIBS`）。
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv));
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();

    let mut opts = NlOptions::default();
    let mut files: Vec<Vec<u8>> = Vec::new();
    let mut index = 1usize;
    // C 的选项串是 `"pb:d:f:h:i:l:n:s:v:w:"`（`nl.c:149`）：只有 `p` 不带值。
    // 逐字母扫，允许聚簇（`-pb`）、值附在本 token（`-ba`）或取下一词（`-b a`），
    // 与 libc `getopt` 同形——旧写法按「整个 token 当一个字母」扫，`-ba` 与 `-pb t`
    // 都会掉进集外字母那支。
    let mut scanning = true;
    while index < args.len() {
        let token = args[index];
        if !scanning || token.len() < 2 || token[0] != b'-' {
            files.push(token.to_vec());
            index += 1;
            continue;
        }
        if token == b"--" {
            scanning = false;
            index += 1;
            continue;
        }
        let bytes = token;
        let mut pos = 1usize;
        while pos < bytes.len() {
            let letter = char::from(bytes[pos]);
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
                            None => getopt_leg(GetoptReport::MissingValue($c), &prog),
                        }
                    } else {
                        // 附值形跳掉前导空白/制表符。**libc 不跳**：`getopt.c:118` 是
                        // `optarg = __UNCONST(place)`，整段剩余直接交回。参照件对
                        // `nl -s' x'` 现量交回 `     1 xa`，本壳交回 `     1xa`（A/B 的
                        // KD4 那两形）。这一处不随本轮的字节口径改动，它是一条独立判据，
                        // 登记 `NL-ATTACHED-VALUE-LEADING-BLANK-STRIPPED`。
                        while matches!(rest.first(), Some(b' ' | b'\t')) {
                            rest = &rest[1..];
                        }
                    }
                    pos = bytes.len();
                    rest
                }};
            }
            match letter {
                'p' => opts.restart = false,
                'b' | 'f' | 'h' => {
                    // C 的 `parse_numbering`（函数在 `nl.c:357`，`switch (argstr[0])` 在 `:362`）
                    // 只看首字符，其余按整串报出去；
                    // 三段的句子各带自己的名字（`numbering_properties[]` 的 name：
                    // footer / body / header，`nl.c:73-75`）。
                    let value = value!('b');
                    let name = match letter {
                        'b' => "body",
                        'f' => "footer",
                        _ => "header",
                    };
                    let kind = match value.first() {
                        Some(b'a') => NumberType::All,
                        Some(b'n') => NumberType::None,
                        Some(b't') => NumberType::Nonempty,
                        // `p<正则>` 是 C 的另一条腿（`regcomp` + 按正则决定编号），本树尚未接；
                        // 这一形与「非法类型」不同句，故单独拒而不冒充 C 的句子（已登记）。
                        Some(b'p') => {
                            errx(&prog, "number_regex (-b/-h/-f p…) not wired".as_bytes())
                        }
                        _ => errx(
                            &prog,
                            quoted(
                                format!("illegal {name} line numbering type -- ").as_bytes(),
                                value,
                                b"",
                            )
                            .as_slice(),
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
                    // 不足两字符时 C 那三条 `if` 读的是 `optarg[1]`/`optarg[2]`，越出终止符
                    // 之外——参照件据此对 `-d\xff` 报错、对 `-d ""` 把**内存里排在后面的
                    // argv 首字节**当成第二个分隔符（取证 `tmp/nl-465/kd2-empty-value-oob.log`）。
                    // 本层按那三条 `if` 的意图办，不照抄越界读；在册 `NL-DELIM-LENGTH-RULE`。
                    let value = value!('d');
                    let chars = value;
                    match chars.len() {
                        0 => {}
                        1 => opts.delim[0] = chars[0],
                        2 => {
                            opts.delim[0] = chars[0];
                            opts.delim[1] = chars[1];
                        }
                        _ => errx(
                            &prog,
                            quoted(b"invalid delim argument -- ", value, b"").as_slice(),
                        ),
                    }
                }
                'i' => {
                    let value = value!('i');
                    match strtol10(value) {
                        // C 这里**没有**非零检查：`incr` 为 0 也照收（后续按 0 递增）。
                        Some(n) if n >= 0 => opts.incr = n as u64,
                        // 决定半的计数器是 `u64`（C 的是 `int`，能倒着数），负增量另成一未接腿，
                        // 不拿 C 的句子冒充——已登记为 `NL-SIGNED-COUNTERS`。
                        Some(_) => errx(&prog, b"signed numbering counters (-i/-v) not wired"),
                        None => errx(
                            &prog,
                            quoted(b"invalid incr argument -- ", value, b"").as_slice(),
                        ),
                    }
                }
                'l' => {
                    let value = value!('l');
                    match strtoul10(value) {
                        Some(n) => opts.nblank = n as usize,
                        None => errx(
                            &prog,
                            quoted(b"invalid num argument -- ", value, b"").as_slice(),
                        ),
                    }
                }
                'n' => {
                    let value = value!('n');
                    // 三个格式名比的是**字节串**（`strcmp(optarg, "ln")` 等三支在 `nl.c:195-199`）。
                    opts.format = match value {
                        b"ln" => NumFormat::Left,
                        b"rn" => NumFormat::Right,
                        b"rz" => NumFormat::RightZero,
                        _ => errx(&prog, quoted(b"illegal format -- ", value, b"").as_slice()),
                    };
                }
                's' => {
                    // `-s` 的值就是输出内容（`nl.c:340` 的 `printf("%s%s", sep, buffer)`），
                    // 所以这一路不能有第二个形状：交回 `Vec<u8>` 就是打出去的那几枚字节。
                    opts.sep = value!('s').to_vec();
                }
                'v' => {
                    let value = value!('v');
                    match strtol10(value) {
                        Some(n) if n >= 0 => opts.startnum = n as u64,
                        Some(_) => errx(&prog, b"signed numbering counters (-i/-v) not wired"),
                        None => errx(
                            &prog,
                            quoted(b"invalid startnum value -- ", value, b"").as_slice(),
                        ),
                    }
                }
                'w' => {
                    let value = value!('w');
                    match strtol10(value) {
                        Some(n) if n > 0 => opts.width = n as usize,
                        // C 的判据是 `if (!(width > 0))`（`nl.c:225-228`），句子把那个值也打出去；
                        // 而“转不出数字”与“转出来了但 ≤ 0”是两句不同的话，不能合并。
                        Some(n) => errx(
                            &prog,
                            format!("width argument must be > 0 -- {n}").as_bytes(),
                        ),
                        None => errx(
                            &prog,
                            quoted(b"invalid width value -- ", value, b"").as_slice(),
                        ),
                    }
                }
                _ => getopt_leg(GetoptReport::UnknownOption(letter), &prog),
            }
            pos += 1;
        }
        index += 1;
    }
    // C 在选项环之后只接受 0 或 1 个操作数，多一个就是 `usage()`（`nl.c:239-249`）；
    // 这一支**不带** libc 那一行（不是 getopt 报的）。
    if files.len() > 1 {
        usage(&prog);
    }

    // Read every operand (a `-` names standard input); concatenate in order.
    let mut input: Vec<u8> = Vec::new();
    if files.is_empty() {
        input = support::read_stdin();
    } else {
        for f in &files {
            if f == b"-" {
                input.extend(support::read_stdin());
            } else {
                // 两条 `&str` 口（开档那一条与 `err` 那一句）之前折一次；档名的**内容**不受
                // 影响，只有名字位是折叠形（在册 `ARGV-BYTE-PATH-IN-LIBS`）。
                let path = String::from_utf8_lossy(f);
                // C 那一支是 `if (strcmp(argv[0], "-") != 0 && freopen(argv[0], "r", stdin)
                // == NULL) err(EXIT_FAILURE, "Cannot open `%s'", argv[0])`（`nl.c:243-245`）：
                // 程序名、**带反引号的操作数**、`strerror` 句子三件齐备，退 1、不接 usage；
                // `-` 那形跳过 `freopen`，于是读的是真标准输入。成功那一侧必须把字节接进
                // `input`：C 把档名重定向到 stdin 上，整条流水线只有 `fgets(..., stdin)`
                // （`:288`）这一个读缝，档名内容与标准输入走同一条编号腿。这里以前只写
                // `if let Err(...)`，`Ok` 的 `Vec` 被当场丢弃，于是 `nl file` 什么都不输出
                // 还退 0（批次四百一十八按参照件 `tmp/nl-417/nl_ref` 逐字节对出来的一形，
                // 登记 `NL-OPERAND-CONTENT-DROPPED`）。
                match support::read_file(&path) {
                    Ok(bytes) => input.extend(bytes),
                    Err(code) => {
                        support::warn_line(&prog, &format!("Cannot open `{path}'"), code);
                        support::terminate(1);
                    }
                }
            }
        }
    }

    let mut lines: Vec<&[u8]> = input.split(|&b| b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }

    let out = nl_lines(&lines, &opts);
    // C 的编号腿是 `fgets` 连着串尾换行一起读、`printf("%s%s", sep, buffer)` 原样打
    // （`nl.c:288`、`:340`），末行没有换行就没有。这一支按字节自己切行（要容忍非
    // UTF-8），所以用与 `lines_of` 同源的判据，而不是在这里另表一遍。
    let keep_final_newline = minix_textfilter::window::final_line_terminated(&input);
    let mut rendered = Vec::new();
    for (index, line) in out.iter().enumerate() {
        rendered.extend_from_slice(line);
        if index + 1 < out.len() || keep_final_newline {
            rendered.push(b'\n');
        }
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

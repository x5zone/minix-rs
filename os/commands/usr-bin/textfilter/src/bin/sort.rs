//! Minix-RS sort — key-based line sorting.
//!
//! Ground truth: `minix3/usr.bin/sort/` (NetBSD). The deciding half —
//! `-k` key specifications, the `b d f i M n r` modifiers, `-n` numeric
//! comparison, `-t` delimiters, `-r`, `-u`, and `-c` — is the library's
//! `sort` module; the doing half collects the operands, orders the lines
//! in memory (the C's external merge over temporary files is the
//! execution layer for inputs beyond memory), and emits lines with a
//! trailing newline. Each input operand is read in file order as its own
//! run (a `-` names standard input; no operand reads standard input); the
//! default concatenates and sorts them, and `-m` merges the runs assuming
//! each is already ordered (`merge_sorted`). `-o` writes to a file instead of
//! standard output, replacing an existing one through a temporary exactly as
//! `sort.c:320-379` does; `-` and the file operands keep their existing shapes.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::sort::{ParseError, compare, merge_sorted, parse_options};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    // C 的每一句诊断都从 `getprogname()` 开头（`sort.c:401-412` 的 usage、
    // `:261-285` 的 errx/err），所以不能写死「sort」。
    let prog = support::prog_name(&argv);
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let (options, operands) = match parse_options(&words) {
        Ok((options, operands)) => (options, operands),
        Err(error) => parse_fail(prog, error),
    };
    // C reads each input operand in file order (`-` / an unrecognised leading
    // dash names standard input) and concatenates their lines; no operand reads
    // standard input. `-c` accepts at most one input file (sort.c too-many guard).
    if options.check && operands.len() > 1 {
        // `errx(2, "too many input files for -c option")`（`sort.c:261`）：只这一句，
        // 没 usage、也【不】缀 `errno` 句子。
        support::warn(format!("{prog}: too many input files for -c option\n").as_bytes());
        support::terminate(2);
    }
    let mut runs: Vec<Vec<String>> = Vec::new();
    if operands.is_empty() {
        runs.push(split_records(&support::read_stdin(), options.record_delim));
    } else {
        for operand in operands {
            let input = if operand == "-" {
                support::read_stdin()
            } else {
                read_operand(prog, operand)
            };
            runs.push(split_records(&input, options.record_delim));
        }
    }
    let mut lines: Vec<String> = runs.iter().flatten().cloned().collect();

    if options.check {
        check(prog, &lines, &options);
        support::terminate(0);
    }

    if options.merge {
        let refs: Vec<&[String]> = runs.iter().map(|r| r.as_slice()).collect();
        lines = merge_sorted(&refs, &options);
    } else {
        // C 的并列键规则要在排序这一步才完整：比较半只在 `-S` 之下拿整条记录
        // 破并列，否则返回平。平局怎么走要看 C 的方向位：默认稳定保持输入序
        //（参照件对四行同键的 `-k 2,2` 给出 `b a c d`，正是输入序），而带全局 `-r`
        // 时同一组读数倒装成 `d c a b`。`sort_by` 本身稳定，拿不到“倒装”这一半，
        // 所以这里显式把输入序号带进比较：正序取升号，`-r` 取降号。
        let mut indexed: Vec<(usize, String)> = lines.drain(..).enumerate().collect();
        indexed.sort_by(|(ia, a), (ib, b)| {
            compare(a, b, &options).then_with(|| {
                if options.reverse {
                    ib.cmp(ia)
                } else {
                    ia.cmp(ib)
                }
            })
        });
        lines = indexed.into_iter().map(|(_, line)| line).collect();
    }
    if options.unique {
        lines.dedup_by(|a, b| compare(a, b, &options) == core::cmp::Ordering::Equal);
    }
    // The whole result goes to one stream: C opens it as `outfp` before sorting and
    // hands that handle to `fsort`/`fmerge` (`sort.c:357-363`), so `-o` cannot be
    // reached at all on the `-c` path, which exited above.
    let mut payload: Vec<u8> = Vec::new();
    for line in &lines {
        payload.extend_from_slice(line.as_bytes());
        // C 的 `putrec`（`files.c:194`/`:215`）是「写完记录再补一个 `REC_D`」，而不是
        // 「除最后一条外补分隔符」：带 `-R` 时输出**以分隔符收尾**（参照件对
        // `b,a,d,c\n` 做 `-R ,` 给 `a,b,c\n,d,`，末尾那个逗号是真源写的）。
        payload.push(options.record_delim);
    }
    write_output(prog, &options, &payload);
    support::terminate(0);
}

/// C's output leg, `sort.c:320-379`, in C's order of operations. Without `-o` it is
/// `stdout`. With `-o` the question is whether the name already exists and is not a
/// device: if so, C refuses an unwritable one, spools the result into a
/// `<path>XXXXXX` temporary, copies the old file's permission bits onto it, unlinks
/// the old name, hard-links the temporary to it and only then unlinks the temporary;
/// if not, C opens the name straight through with `"w"`. The temporary is why a
/// sort that dies halfway leaves the previous file intact rather than an empty one.
fn write_output(prog: &str, options: &minix_textfilter::sort::Options, payload: &[u8]) {
    use minix_textfilter::sort::{Output, OutputProbe, decide_output, temp_candidate};

    // The path is probed only when `-o` was given, as C's `lstat` sits inside the
    // `else if (outpath)` branch (`sort.c:324`).
    let given = options.output.as_deref();
    let target = given.map_or_else(String::new, |bytes| {
        String::from_utf8_lossy(bytes).into_owned()
    });
    let probe = given
        .and_then(|_| support::probe_path(&target))
        .map(|(mode, is_device)| OutputProbe { mode, is_device });
    match decide_output(given, probe) {
        Output::StandardOutput => support::emit(payload),
        Output::Direct { .. } => match support::create_sink(&target) {
            Ok(sink) => {
                // C writes through stdio and never inspects the result; the flush at
                // exit can fail just as silently, so no message is invented here.
                let _ = sink.finish(payload);
            }
            // C `sort.c:354-355` 是 `err(2, "output file %s", outfile)`：那句后面还缀
            // 着 `strerror` 的句子（旧写法只印到名字为止）。
            Err(code) => {
                support::warn_line(prog, &format!("output file {target}"), code);
                support::terminate(2);
            }
        },
        Output::ViaTemp { mode, .. } => {
            // `access(outpath, W_OK)` 先走（`sort.c:336-337`）。那一支是
            // `err(2, "%s", outpath)`：程序名、路径、`strerror` 句子三件齐备，
            // 不是一句裸路径。
            if let Err(code) = support::writable(&target) {
                support::warn_line(prog, &target, code);
                support::terminate(2);
            }
            let seed = support::epoch_micros().unwrap_or(0);
            let mut temp = target.clone();
            let mut sink = None;
            // 最后一次的号要留着报：C 是 `err(2, "Cannot create temporary file `%s'", toutpath)`
            // （`sort.c:340-341`），那句后面缀的是 `strerror`。试名过程中的失败不报，
            // 只有整个循环跑空才报（与 C 的 `mkstemp` 只试一次不同：本树拿
            // 时种推候选名，所以上面那六个字符的碰撞要自己跳）。
            let mut last_error = minix_sys::Errno::EEXIST;
            for attempt in 0..64 {
                temp = String::from_utf8_lossy(&temp_candidate(target.as_bytes(), seed, attempt))
                    .into_owned();
                match support::create_exclusive(&temp) {
                    Ok(opened) => {
                        sink = Some(opened);
                        break;
                    }
                    Err(code) => last_error = code,
                }
            }
            let Some(sink) = sink else {
                support::warn_line(
                    prog,
                    &format!("Cannot create temporary file `{temp}'"),
                    last_error,
                );
                support::terminate(2);
            };
            if sink.finish(payload).is_err() {
                // 本壳把写失败报在这里；C 那一支是 `fdopen(outfd, "w")` 失败（`:350-351`），
                // 两者不是同一个事件（写失败在 C 里由 stdio 在 exit 时默默丢弃）。
                // 本轮不动这个形状，只登记。
                support::warn(format!("sort: Cannot open temporary file `{temp}'\n").as_bytes());
                support::terminate(2);
            }
            // `access(outfile, F_OK)` 在换 mode 之前（`sort.c:364-365`），然后原档自己的
            // 权限位落到临时档上（`:373-374`），最后才换名字（`:376-380`）。
            if let Err(code) = support::exists(&temp) {
                // C 这一支也是 `err(2, "%s", outfile)`：临时档不见了也要报出号。
                support::warn_line(prog, &temp, code);
                support::terminate(2);
            }
            if let Err(code) = support::set_mode(&temp, mode) {
                // `err(2, "cannot chmod %s: output left in %s", outpath, outfile)`
                // （`sort.c:373-374`）：两个名字都在句子里，后面还缀号。
                support::warn_line(
                    prog,
                    &format!("cannot chmod {target}: output left in {temp}"),
                    code,
                );
                support::terminate(2);
            }
            // C 对 `unlink(outpath)` 的失败不看（`:376`），这里也不看。
            let _ = support::remove_path(&target);
            if let Err(code) = support::hard_link(&temp, &target) {
                // `err(2, "cannot link %s: output left in %s", outpath, outfile)`
                // （`sort.c:379-380`）。
                support::warn_line(
                    prog,
                    &format!("cannot link {target}: output left in {temp}"),
                    code,
                );
                support::terminate(2);
            }
            let _ = support::remove_path(&temp);
        }
    }
}

/// Splits collected input into records on C's `REC_D` (`-R`, default `'\n'`),
/// dropping the delimiter itself and the final empty piece after a terminating
/// delimiter — that is what C's `getrec`（`files.c:113-124`）do：读到分隔符就收尾这一条，
/// 而 EOF 前一个字节也没读到就返回 0（所以 `a,b,` 是两条记录不是三条）。
/// 记录内容本身不去任何东西：`-R ,` 下 `b,a,d,c\n` 的最后一条就是 `c\n`（带着换行），
/// 因为换行在这一形里只是普通字节。
fn split_records(input: &[u8], rec_d: u8) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut pieces: Vec<String> = input
        .split(|byte| *byte == rec_d)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    if input.ends_with(&[rec_d]) {
        pieces.pop();
    }
    pieces
}

/// Read an input file to the end; an unopenable file is a status-2 error
/// (the C's `access(argv[i], R_OK)` guard at `sort.c:284` and the `fopen` failure
/// in `files.c`). The open-and-read itself is the shared `bin_support` twin, so the
/// hosted door and the target run take the same path through the seam.
fn read_operand(prog: &str, path: &str) -> Vec<u8> {
    match support::read_file(path) {
        Ok(data) => data,
        // C `sort.c:285` 是 `err(2, "%s", argv[i])`：程序名、操作数、`strerror` 句子
        // 三件齐备（旧写法把 `err(3)` 当成 `errx(3)` 用，只留了前两件）。
        Err(code) => {
            support::warn_line(prog, path, code);
            support::terminate(2);
        }
    }
}

/// 把决定半的 [`ParseError`] 落成 C 那一条腿的报文。三条腿各不相同：
/// - libc 先印一行，再是**整个 usage 块**，退 2（`sort.c:252` 的 `usage(NULL)`，
///   报文本体在 `getopt.c:101-104`/`:132-136`）；
/// - `sort: <那五句之一>`，退 2，**没 usage**（`init.c:159-217` 的 `errx`，句文本单源在
///   `sort.rs:KeySpecError::sentence`）——旧写法把五句挤成了一句 `illegal field descriptor`；
/// - `sort: multiple field delimiters` 紧跟整个 usage 块，退 2（`sort.c:214-215` 的
///   `usage("multiple field delimiters")`，走的是 `usage()` 的 `%s: %s` 那头一句）。
fn parse_fail(prog: &str, error: ParseError) -> ! {
    match error {
        ParseError::Getopt(report) => {
            // 字母位走 `line_bytes`：取法已在 `src/sort.rs` 的环里改成按字节，两边同形才成立。
            support::warn(&report.line_bytes(prog));
            usage(prog, None)
        }
        ParseError::KeySpec(spec) => {
            support::warn(format!("{prog}: {}\n", spec.sentence()).as_bytes());
            support::terminate(2);
        }
        ParseError::MultipleFieldDelimiters => usage(prog, Some("multiple field delimiters")),
        // 下面三支都是 `sort.c:223-242` 与 `:217-218` 的形状：`multiple record
        // delimiters` 走 `usage()`（一句话 + 整个 usage 块），而两句 `errx`（clash 与
        // invalid）只有一行、**不接 usage**；三者都退 2。
        ParseError::MultipleRecordDelimiters => usage(prog, Some("multiple record delimiters")),
        ParseError::RecordFieldDelimiterClash => {
            support::warn(format!("{prog}: record/field delimiter clash\n").as_bytes());
            support::terminate(2);
        }
        ParseError::InvalidRecordDelimiter(value) => {
            let shown = String::from_utf8_lossy(&value).into_owned();
            support::warn(format!("{prog}: invalid record delimiter {shown}\n").as_bytes());
            support::terminate(2);
        }
    }
}

/// C `usage()`（`sort.c:399-412`）：可选的 `"%s: %s\n"` 头一句，然后是那**一个**输出行。
/// 那一行由两条 `fprintf` 拼成，第一条不带换行、第二条以十三个空格开头，所以真源的
/// usage 看上去是「一行里裂了一段大间距」——间距与列项都逐字保留（那十三个空格是从
/// 编译产物 `sort.o` 的 `.rodata` 里数出来的，不是手抄）。退 2。
fn usage(prog: &str, message: Option<&str>) -> ! {
    if let Some(message) = message {
        support::warn(format!("{prog}: {message}\n").as_bytes());
    }
    support::warn(
        format!(
            "usage: {prog} [-bcdfHilmnrSsu] [-k field1[,field2]] [-o output] [-R char] [-T dir]             [-t char] [file ...]\n"
        )
        .as_bytes(),
    );
    support::terminate(2)
}

/// The `-c` mode: every adjacent pair must be in order, and with `-u`
/// no pair may compare equal. The first violation is reported by name
/// `-c` 那一路的两句话，逐字对位 C 的 `msort.c:order()`：失序是
/// `errx(1, "found disorder: %s", …)`（`msort.c:403`），`-u` 下键相等是
/// `errx(1, "found non-uniqueness: %s", …)`（`:405-407`）。两句都退 1、都**不接
/// usage**，程序名走 `argv[0]` 的最后一段（`errx` 自己加），而那一个 `%s` 装的是
/// **被报出的后一条记录**——C 在打印前先把记录尾部分隔符写成 `\0`
/// （`crec->data[crec->length-1] = 0;`，`:402`/`:406`），所以输出正好是一行一条。
/// 本壳之前写的是 `sort: disorder: ` 与 `sort: duplicate: `，两句都是自创措辞，
/// 且程序名写死成 `sort`（本轮批次二百九十二按参照件读数改掉：`sort: found disorder: a`
/// 与 `sort: found non-uniqueness: a`，两者退码都是 1）。
fn check(prog: &str, lines: &[String], options: &minix_textfilter::sort::Options) {
    let mut previous: Option<&str> = None;
    for line in lines {
        if let Some(prev) = previous {
            // 判据的先后照 C：先 `0 < cmp(prec, crec)`（失序），再看
            // `UNIQUE && !c`（重复）。两者都对**同一次**比较取值，所以不会各比一遍。
            match compare(prev, line, options) {
                core::cmp::Ordering::Greater => {
                    support::warn(format!("{prog}: found disorder: {line}\n").as_bytes());
                    support::terminate(1);
                }
                core::cmp::Ordering::Equal if options.unique => {
                    support::warn(format!("{prog}: found non-uniqueness: {line}\n").as_bytes());
                    support::terminate(1);
                }
                _ => {}
            }
        }
        previous = Some(line);
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

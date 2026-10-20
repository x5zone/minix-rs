//! Minix-RS units — conversion program.
//!
//! Ground truth: `minix3/usr.bin/units/units.c`, usage
//! `units [-Llqv] [-f filename] [[count] from-unit to-unit]`. The
//! deciding half (units-file parsing, expression reduction,
//! conformability, answer rendering, the `-l`/`-L` listings) is the
//! library's `units` module; this program runs the option ring, loads
//! the database, and drives the interactive prompt face.
//!
//! 前门有三类出口，退码一律 3，句式各有主人。libc `getopt` 的那一行（集外字母、
//! 带值的 `-f` 缺值——`units.c:817` 的 optstring 是 `"lLvqf:"`，只有 `f` 带值，
//! 所以缺值形在本支只有 `f` 一个字母可达）由 [`minix_textfilter::GetoptReport`]
//! 单源，其后必接整块 usage，因为 `units.c:843-845` 写的是 `default: usage()`；
//! `-v` 打两行版本再接 usage，那两行在**标准错误**上（`units.c:836-839` 是
//! `fprintf(stderr, …)`）；操作数个数不对只有 usage 块，没有 libc 那一行。
//! usage 块是九行（两个 `fprintf`：首行自带前导空行，说明段自带前导空行），
//! 里面的 `units` 是写死在句子中的字面量，不跟 `getprogname()`——只有 libc 那一行跟。
//!
//! 两个档口句子分属两支：`-f` 的档打不开是 `err(1, "Unable to open units file
//! '%s'", …)` 的三段形（带 `strerror`），默认档 `/usr/share/misc/units.lib`
//! （`minix3/usr.bin/units/pathnames.h`）找不到是 `errx(1, "Can't find units
//! file '%s'", …)` 的两段形（不带）。操作数个数的检查排在读库**之前**
//! （`units.c:849-857`），所以「坏档加一个操作数」报的是 usage 而不是开档失败。
//!
//! argv 与交互行都按字节进链路（批次四百五十五两片）：选项字母、操作数里的单位名、
//! `-l` 清单与 `Unknown unit` 那句的回显位交回的都是原字节；只有 `-f` 的档名两条腿
//! （打开与回显）还过库侧的 `&str` 口，登记在册。
//!
//! 交互面的每一行都走 `bin_support::read_line_bounded`：宿主与目标各一条腿，
//! 这是同族第十处「薄壳裸 `read` 宿主读不到输入」的修复。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::units::{
    PRECISION, PRECISION_LIST_EXPAND, UnitType, UnitsArgError, UnitsTable, add_unit,
    complete_reduce, list_units, parse_units_args, read_units, render_answer, show_answer,
    units_error_line,
};

/// C 的 `_PATH_UNITSLIB`（`minix3/usr.bin/units/pathnames.h`）。
const UNITSFILE: &str = "/usr/share/misc/units.lib";
/// C 的 `fgets(havestr, 80, stdin)`：`havestr` 是 `char[81]`，一次最多收 79 字节，
/// 第 80 个字节留给下一个换行或下一个数据字符，故截断时换行**留在流里**。
const LINE_LIMIT: usize = 79;

fn run() -> ! {
    // argv 按字节交回：C 的 `crt0` 把 `argv` 当字节数组、`getopt` 的字母取自字节步进，
    // 所以短横后那一枚 `ff` 到报文里还得是那一枚 `ff`（批次四百四十定的字节缝）。
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // 程序名那一腿要交给 `warn_line` 与 `units_error_line` 的 `&str` 口（登记
    // `ARGV-BYTE-PATH-IN-LIBS`），所以在臂内折一次 lossy——与其余各支同一条既定口径。
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let prog = prog.as_str();
    let args: Vec<&[u8]> = argv[1..].iter().map(Vec::as_slice).collect();
    let parsed = match parse_units_args(&args) {
        Ok(parsed) => parsed,
        Err(UnitsArgError::Getopt(report)) => {
            // 走字节形：libc 那一行的字母位是一个裸字节（`getopt.c:79` 的
            // `optopt = *place++` 一次进一个字节、`:103` 用 `%c` 原样写出）。参照件
            // `tmp/units-319/units` 现量 `units -é` 打 `unknown option -- ` + `c3` + 换行，
            // 而按 `char` 渲染会多出第二个字节（U+00C3 的 UTF-8 是 `c3 83`）。
            support::warn(&report.line_bytes(prog));
            usage();
        }
        Err(UnitsArgError::Version) => {
            support::warn(VERSION.as_bytes());
            usage();
        }
        Err(UnitsArgError::Usage) => usage(),
    };

    let text = load_units_file(prog, parsed.userfile);
    let table = read_units(&split_file_lines(&text));

    if parsed.list {
        let prec = if parsed.listexpand {
            PRECISION_LIST_EXPAND
        } else {
            PRECISION
        };
        let (listing, errors) = list_units(&table, parsed.listexpand, prec);
        support::emit(&listing);
        support::terminate(if errors > 0 { 1 } else { 0 });
    }

    match parsed.operands.len() {
        2 | 3 => convert_operands(prog, &table, &parsed.operands),
        _ => interactive(prog, &table, parsed.quiet),
    }
}

/// 两个或三个操作数那一路（`units.c:855-875`）。三个时前两个用空格并成 have 侧；
/// 环不重排，所以 `units -f <档> m sec -l` 的 `-l` 是**第三个操作数**而不是选项。
///
/// 操作数交回的是 argv 的原字节（`support::args_bytes()`），非法 UTF-8 也照样进链路。
fn convert_operands(prog: &str, table: &UnitsTable, operands: &[&[u8]]) -> ! {
    let have_expr: Vec<u8> = if operands.len() == 3 {
        let mut joined = operands[0].to_vec();
        joined.push(b' ');
        joined.extend_from_slice(operands[1]);
        joined
    } else {
        operands[0].to_vec()
    };
    let have = build_unit(prog, table, &have_expr);
    let want = build_unit(prog, table, operands[operands.len() - 1]);
    // C 不看 `addunit`/`completereduce` 的返回值，拿半成品照样出一个答案块再退 0
    // （`units.c:864-875`）；本壳这两支交回 `Err` 时没有半成品可展示，于是报文照 C、
    // 退码照 C，只有答案块那一行缺——登记为 UNITS-PARTIAL-UNIT-ON-ERROR。
    if let (Some(have), Some(want)) = (have, want) {
        support::emit(&render_answer(&show_answer(&have, &want), PRECISION));
    }
    support::terminate(0);
}

/// 一个操作数都没有那一路：`You have: `/`You want: ` 成对提示。
fn interactive(prog: &str, table: &UnitsTable, quiet: bool) -> ! {
    if !quiet {
        support::emit(
            format!(
                "{} units, {} prefixes\n\n",
                table.units.len(),
                table.prefixes.len()
            )
            .as_bytes(),
        );
    }
    loop {
        let have = read_unit(prog, table, quiet, b"You have: ");
        let want = read_unit(prog, table, quiet, b"You want: ");
        support::emit(&render_answer(&show_answer(&have, &want), PRECISION));
    }
}

/// C 的 `do { 提示; fgets; } while (addunit || completereduce)`（`units.c:876-893`）：
/// 一行不认就**重问同一格**，而不是跳到对面那一格。EOF 时非 `-q` 先补一个换行再退 0。
///
/// 那一行交出的字节原样进单位名链路：C 的 `fgets` 之后从不解码，
/// `addunit`→`lookupunit` 的 `strcmp` 与 `mywarnx` 的 `'%s'` 都按字节走。
fn read_unit(prog: &str, table: &UnitsTable, quiet: bool, prompt: &[u8]) -> UnitType {
    loop {
        if !quiet {
            support::emit(prompt);
        }
        let Some(bytes) = support::read_line_bounded(LINE_LIMIT) else {
            if !quiet {
                support::emit(b"\n");
            }
            support::terminate(0);
        };
        let mut unit = match add_unit(&bytes, false) {
            Ok(unit) => unit,
            Err(error) => {
                report(prog, &error);
                continue;
            }
        };
        match complete_reduce(table, &mut unit) {
            Ok(()) => return unit,
            Err(error) => report(prog, &error),
        }
    }
}

fn build_unit(prog: &str, table: &UnitsTable, expr: &[u8]) -> Option<UnitType> {
    let mut unit = match add_unit(expr, false) {
        Ok(unit) => unit,
        Err(error) => {
            report(prog, &error);
            return None;
        }
    };
    match complete_reduce(table, &mut unit) {
        Ok(()) => Some(unit),
        Err(error) => {
            report(prog, &error);
            None
        }
    }
}

/// 推导期的 `mywarnx` 句子（`units.c:109-124`）。非列表模式下 `errprefix` 是 `NULL`，
/// 整句走 `vwarnx` 到标准错误。
fn report(prog: &str, error: &minix_textfilter::units::UnitsError) {
    if let Some(line) = units_error_line(prog, error) {
        support::warn(&line);
    }
}

/// 读词典档。`-f` 与默认档的失败句子分属 `err` 与 `errx` 两支，见模块头的对照。
fn load_units_file(prog: &str, path: Option<&[u8]>) -> Vec<u8> {
    match path {
        Some(raw) => {
            // `-f` 的档名两条腿（打开、回显）都还过库侧的 `&str` 口：
            // `minix_sys::open` 与 `minix_types::errno::err_line` 都收 `&str`，
            // 登记 `ARGV-BYTE-PATH-IN-LIBS`，库侧换字节形之前在这里折一次 lossy。
            let name = String::from_utf8_lossy(raw);
            match support::read_file(&name) {
                Ok(bytes) => bytes,
                Err(code) => {
                    support::warn_line(prog, &format!("Unable to open units file '{name}'"), code);
                    support::terminate(1);
                }
            }
        }
        None => match support::read_file(UNITSFILE) {
            Ok(bytes) => bytes,
            // C 在这里先按 `PATH` 逐个目录找一遍（`units.c:146-176`），那一段登记为
            // UNITS-PATH-SEARCH；两形都落不到本机可测的目录上，最终失败才是这一句。
            Err(_) => {
                support::warn(format!("{prog}: Can't find units file '{UNITSFILE}'\n").as_bytes());
                support::terminate(1);
            }
        },
    }
}

/// 词典的换行切分按字节走（C 的 `fgets(line, 79, unitfile)` 从不解码）。
/// 旧写法先解一次整档 UTF-8、解不过去就把**整份**词典当空串，一枚裸字节能让
/// 全部单位消失——登记 `UNITS-NONUTF8-DROP` 的那一格，本片随 `read_units` 换字节键消掉。
fn split_file_lines(bytes: &[u8]) -> Vec<&[u8]> {
    // `split` 在档尾那个换行之后多给一个空项；`read_units` 对空名一律跳过，
    // 与 C 的 `fgets` 在 EOF 处不再产生一行等效。
    bytes.split(|&byte| byte == b'\n').collect()
}

/// `units.c:793-807` 的 `usage()`：两个 `fprintf` 合起来九行，退码 3。
///
/// 用 `concat!` 而不是带反斜杠续行的字节串：续行会吃掉下一行的**前导空白**，
/// 那四格缩进就是这么丢掉的（本门第一跑逮到的正是这一处）。
const USAGE: &str = concat!(
    "\nunits [-Llqv] [-f filename] [[count] from-unit to-unit]\n",
    "\n    -f specify units file\n",
    "    -L list units in standardized base units\n",
    "    -l list units\n",
    "    -q suppress prompting (quiet)\n",
    "    -v print version number\n",
);

/// `units.c:836-839` 的两行版本，同在一个 `fprintf(stderr, …)` 上。
const VERSION: &str = concat!(
    "\n  units version 1.0  Copyright (c) 1993 by Adrian Mariano\n",
    "                    This program may be freely distributed\n",
);

fn usage() -> ! {
    support::warn(USAGE.as_bytes());
    support::terminate(3);
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

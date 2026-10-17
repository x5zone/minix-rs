//! Minix-RS split — split input into numbered chunk files.
//!
//! Ground truth: `minix3/usr.bin/split/split.c` (NetBSD): chunks of
//! 1000 lines (`-l` moves it) named by a two-letter suffix `aa`, `ab`,
//! ... The deciding half (chunk computation and suffix enumeration) is
//! the library's `split` module; this build reads an input file (or
//! stdin) and writes each chunk to `prefix<suffix>` created with
//! `O_WRONLY|O_CREAT|O_TRUNC` at DEFFILEMODE (`split.c:332`). The input
//! operand special-cases `-` as stdin (`split.c:136`, unlike grep/sed);
//! the second operand is the output prefix (default `x`). All three modes
//! address the input by **byte**: `split.c:185`/`:252` hand the same
//! `read`-filled buffer to `split1`/`split2`, which never look at an
//! encoding. `-a` (suffix width)、`-b` (bytes)、`-n` (chunk count) 都由决定半
//! 解析，模式互斥照 C。Real-machine file creation is gated at C-57/E5.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::split::{
    chunk_bytes, chunks_bytes, chunks_n, lines_bytes, parse_split_args, suffix, SplitError,
};
use minix_textfilter::GetoptReport;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let prog = support::prog_name(&argv);
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // argv folding lives in the deciding half (`parse_split_args`): BSD getopt
    // `"0123456789b:l:a:n:"` with attached-or-next values and the obsolete
    // `-<数字>`. 集外字母与缺值各由 libc 先印一行，本壳再接 usage。
    let parsed = match parse_split_args(&args[1..]) {
        Ok(p) => p,
        Err(e) => arg_error(prog, e),
    };
    let lines_per_chunk = parsed.lines_per_chunk;
    let sfxlen = parsed.sfxlen;
    let byte_size = parsed.byte_size;
    let chunk_count = parsed.chunk_count;
    let operands: Vec<&str> = parsed.operands;
    // `split.c:143`: the second operand is the output prefix, default "x".
    let prefix = operands.get(1).copied().unwrap_or("x");

    // `split.c:135-139`: `-` reads stdin (split, unlike grep/sed, special-cases
    // it), a named input is opened, no operand reads stdin, and an unopenable
    // input is fatal (`err(1, "%s", *argv)`).
    let input = match operands.first() {
        None => support::read_stdin(),
        Some(name) if *name == "-" => support::read_stdin(),
        Some(name) => match support::read_file(name) {
            Ok(data) => data,
            // C 的 `err(1, "%s", *argv)`（`split.c:137-138`）：程序名、操作数原文、
            // `strerror` 句子三段齐备。旧写法只印到名字为止（同族第五例）。
            Err(code) => {
                support::warn_line(prog, name, code);
                support::terminate(1);
            }
        },
    };

    // 行模式吃的是原始字节：`split2` 在读进来的缓冲里数换行（`split.c:252`、
    // `:262-267`），从不校验编码，所以不可解码的字节只是普通数据；换行留在
    // 它结束的那一行上，末行不带换行就不带。
    let lines = lines_bytes(&input);
    // `-b`: byte-addressed split (`split1`) over the raw input, no line
    // reconstruction — each fixed-size chunk is written verbatim.
    if let Some(n) = byte_size {
        for (index, chunk) in chunk_bytes(&input, n).iter().enumerate() {
            let path = match out_path(prefix, index, sfxlen) {
                Some(path) => path,
                None => too_many_files(),
            };
            if let Err(failure) = create_write(&path, chunk) {
                write_failure(prog, &path, failure);
            }
        }
        support::terminate(0);
    }
    // `-n`: exactly `n` byte chunks (`split3`); the last absorbs the remainder.
    // `chunks_n` returns `None` when `n > input.len()` (C 的
    // `errx(1, "can't split into more than %d files", (int)sb.st_size)`，
    // `split.c:298-299`，报的是输入的**字节数**）或片宽为零。
    if let Some(n) = chunk_count {
        match chunks_n(&input, n) {
            Some(chunks) => {
                for (index, chunk) in chunks.iter().enumerate() {
                    let path = match out_path(prefix, index, sfxlen) {
                        Some(path) => path,
                        None => too_many_files(),
                    };
                    if let Err(failure) = create_write(&path, chunk) {
                        write_failure(prog, &path, failure);
                    }
                }
            }
            None => {
                support::warn(
                    format!("{prog}: can't split into more than {} files\n", input.len())
                        .as_bytes(),
                );
                support::terminate(1);
            }
        }
        support::terminate(0);
    }
    for (index, chunk_lines) in chunks_bytes(&lines, lines_per_chunk).iter().enumerate() {
        // `split.c:332`: each chunk lands in `prefix + suffix(index)`, created
        // with O_WRONLY|O_CREAT|O_TRUNC at DEFFILEMODE.
        let path = match out_path(prefix, index, sfxlen) {
            Some(path) => path,
            None => too_many_files(),
        };
        if let Err(failure) = create_write(&path, &render_chunk(chunk_lines)) {
            write_failure(prog, &path, failure);
        }
    }
    support::terminate(0);
}

/// Output path for chunk `index`: `prefix` + the `width`-letter suffix, or
/// `None` once the suffix space (`26^width`) is exhausted.
fn out_path(prefix: &str, index: usize, width: usize) -> Option<String> {
    suffix(index, width).map(|suffix| format!("{prefix}{suffix}"))
}

/// Renders one chunk back into bytes: the chunk's lines concatenated, each
/// already carrying the terminator it had in the input. Adding one here would
/// invent a newline the input never had（C 搬的是 `Ce - Cs + 1` 个原字节）。
fn render_chunk(chunk_lines: &[&[u8]]) -> Vec<u8> {
    let mut data = Vec::new();
    for line in chunk_lines {
        data.extend_from_slice(line);
    }
    data
}

/// Creates (or truncates) `path` and writes `bytes` into it, mirroring
/// `split.c`'s `open(O_WRONLY|O_CREAT|O_TRUNC, DEFFILEMODE)` plus the partial
/// write loop（`newfile()` 与 `bigwrite()`）。
///
/// 缝走 `bin_support::create_sink` 的双生面（宿主 `std::fs::File::create`、真机
/// `minix_sys::open`）：本文件以前直接拿裸 `minix_sys::open`/`write`/`close`，
/// **宿主上一个分片文件都写不出去**，所以 `split` 的输出腿从来没有被进程级测过
/// （读缝同病的第五例：批次三百零六的 `nl`、三百零八的 `fold`、三百零九的 `rev`、
/// 三百一十的 `colrm`）。交回的是 `Errno`，因为 C 那一行是 `err(3)` 三段形。
fn create_write(path: &str, bytes: &[u8]) -> Result<(), ChunkWrite> {
    let sink = support::create_sink(path).map_err(ChunkWrite::Open)?;
    // 短写与写错在 C 那边由 `bigwrite` 检出后 `err(1, "write")`；本壳的 `finish`
    // 不交回平台号，按既有口径退到 `EIO`（`bin_support::errno_of` 同一处置）。
    sink.finish(bytes)
        .map_err(|_| ChunkWrite::Write(minix_sys::Errno::EIO))
}

/// 分片文件的两种失败：开不开（C 报路径）与写不写进（C 报 `write`）。
enum ChunkWrite {
    Open(minix_sys::Errno),
    Write(minix_sys::Errno),
}

/// 两个失败出口各自的 `err(3)` 三段形，印完即退 1（C 的 `err` 本身就是致命支）。
fn write_failure(prog: &str, path: &str, failure: ChunkWrite) -> ! {
    match failure {
        // `split.c:332-333`：`err(1, "%s", fname)`。
        ChunkWrite::Open(code) => support::warn_line(prog, path, code),
        // `split.c:204` 等五处：`err(1, "write")`——标签是 `write` 而不是路径。
        ChunkWrite::Write(code) => support::warn_line(prog, "write", code),
    }
    support::terminate(1);
}

/// C 的 `errx(1, "too many files.")`（`split.c:326-330`）：后缀空间（26 的 width 次方）
/// 用完。旧写法印的是本壳自拟的 `suffixes exhausted`。
fn too_many_files() -> ! {
    support::warn(b"split: too many files.\n");
    support::terminate(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_path_prefixes_the_two_letter_suffix() {
        assert_eq!(out_path("x", 0, 2).as_deref(), Some("xaa"));
        assert_eq!(out_path("x", 1, 2).as_deref(), Some("xab"));
        assert_eq!(out_path("part.", 26, 2).as_deref(), Some("part.ba"));
        // -a 1：单字母后缀。
        assert_eq!(out_path("x", 0, 1).as_deref(), Some("xa"));
        assert_eq!(out_path("x", 26, 1), None);
    }

    #[test]
    fn out_path_is_none_once_suffixes_exhaust() {
        // aa..zz is 676 names; index 675 is the last, 676 has none.
        assert!(out_path("x", 675, 2).is_some());
        assert_eq!(out_path("x", 676, 2), None);
    }

    #[test]
    fn render_chunk_writes_its_lines_as_they_came() {
        let lines: &[&[u8]] = &[b"a\n", b"b"];
        assert_eq!(render_chunk(lines), b"a\nb".to_vec());
    }

    #[test]
    fn render_chunk_of_no_lines_is_empty() {
        assert!(render_chunk(&[]).is_empty());
    }
}

fn usage(prog: &str) -> ! {
    // C 的 `usage()`（`split.c:355-361`）：程序名跟着 `getprogname()`、一整行、`exit(1)`。
    // 旧写法两样都不合：句子是本壳自拟的短式，退码写成 2。
    support::warn(
        format!(
            "usage: {prog} [-b byte_count] [-l line_count] [-n chunk_count] \
[-a suffix_length] [file [prefix]]\n"
        )
        .as_bytes(),
    );
    support::terminate(1);
}

/// Map a [`SplitError`] to the exact C path（哪一行、几行、退码）。
fn arg_error(prog: &str, e: SplitError<'_>) -> ! {
    match e {
        SplitError::Usage => usage(prog),
        SplitError::UnknownOption(letter) => {
            support::warn(GetoptReport::UnknownOption(letter).line(prog).as_bytes());
            usage(prog)
        }
        SplitError::MissingValue(letter) => {
            support::warn(GetoptReport::MissingValue(letter).line(prog).as_bytes());
            usage(prog)
        }
        SplitError::IllegalLineCount(v) => {
            support::warn(format!("{prog}: {v}: illegal line count.\n").as_bytes());
            support::terminate(1);
        }
        SplitError::IllegalByteCount(v) => {
            support::warn(format!("{prog}: {v}: illegal byte count.\n").as_bytes());
            support::terminate(1);
        }
        SplitError::IllegalChunkCount(v) => {
            support::warn(format!("{prog}: {v}: illegal number of chunks.\n").as_bytes());
            support::terminate(1);
        }
        SplitError::IllegalSuffixLength(v) => {
            support::warn(format!("{prog}: {v}: illegal suffix length.\n").as_bytes());
            support::terminate(1);
        }
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

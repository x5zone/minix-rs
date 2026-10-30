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
//! encoding. `-a` (suffix width), `-b` (bytes), and `-n` (chunk count) are all
//! parsed by the deciding half, and the modes are mutually exclusive as in C.
//! Real-machine file creation is gated at E5.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;

/// Bytes read in one go, taken from C's `MAXBSIZE`
/// (`minix3/tools/compat/compat_defs.h:1151` writes it as `64 * 1024`).
const STREAM_CHUNK: usize = 64 * 1024;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::split::{
    ByteCutter, CutPiece, LineCutter, SplitError, chunks_n, parse_split_args, suffix,
};

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // Convert the program name once (the two `&str` entry points of `err3` and
    // `line_bytes`, registered as `ARGV-BYTE-PATH-IN-LIBS`); the operands, the
    // prefix, and the value positions of the four `illegal X count.` messages
    // each have their own legs that build from the original bytes and do not
    // pass through here.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // argv folding lives in the deciding half (`parse_split_args`): BSD getopt
    // `"0123456789b:l:a:n:"` with attached-or-next values and the obsolete
    // `-<number>`. For an unknown option letter and a missing value alike,
    // libc prints one line first and this shell then appends usage.
    let parsed = match parse_split_args(&args[1..]) {
        Ok(p) => p,
        Err(e) => arg_error(&prog, e),
    };
    let lines_per_chunk = parsed.lines_per_chunk;
    let sfxlen = parsed.sfxlen;
    let byte_size = parsed.byte_size;
    let chunk_count = parsed.chunk_count;
    let operands: Vec<&[u8]> = parsed.operands;
    // `split.c:143`: the second operand is the output prefix, default "x".
    let prefix: &[u8] = operands.get(1).copied().unwrap_or(b"x");

    // The third-operand test comes after opening in C (`split.c:135-148`):
    // `open(*argv)` first, and a failed open is `err(1, "%s", *argv)` exiting
    // with the file name; only a successful open lets `usage()` run. `-` does
    // not go through opening (`:136` special-cases it), so that form falls
    // straight into usage. The probe happens only on this error leg; normal
    // splitting still lets each later leg open the input itself.
    if operands.len() > 2 {
        if let Some(name) = operands.first().filter(|name| **name != b"-")
            && let Err(code) = support::open_input(&String::from_utf8_lossy(name))
        {
            err3(&prog, name, code, 1);
        }
        usage(&prog);
    }

    // `split.c:135-139`: `-` reads stdin (split, unlike grep/sed, special-cases
    // it), a named input is opened, no operand reads stdin, and an unopenable
    // input is fatal (`err(1, "%s", *argv)`).
    // The read leg's three forms each have their place: no operand means stdin,
    // a bare `-` also means stdin (`split.c:136` special-cases it), and
    // anything else is opened by name. A failed open exits on the spot with
    // `err(1, "%s", *argv)`; a failed `read` must first cut out what was
    // already read and then add `err(1, "read")` (`:185-190`, `:252-257`), so
    // that error number travels on together with the bytes read.
    // `-b`: fixed-length splitting by bytes takes C's **streaming** `split1`
    // leg — read one block at a time and write as you go, keeping only a
    // counter across reads (`split.c:175-236`). It must branch before the
    // whole file is read, or "the entire input is in memory" would drag this
    // leg's usage back up.
    if let Some(n) = byte_size {
        let source = operands.first().copied();
        stream_outcome(&prog, source, split_bytes(&prog, prefix, source, n, sfxlen));
    }

    // The default and `-l` take C's **streaming** `split2` leg: read one block
    // at a time, keeping only two numbers across reads — how many whole lines
    // the current chunk has counted and whether the current chunk is still
    // open (`split.c:242-282`). Line mode consumes raw bytes; `split2` only
    // counts newlines in the buffer and never validates encoding, so
    // undecodable bytes are ordinary data; a newline stays on the line it
    // terminates, and a last line without a newline simply has none. The
    // branch point is there for the same reason as with `-b`: reading the
    // whole file would drag this leg's usage back to "the entire input is in
    // memory". The test is "not `-n`" rather than "a line count is present",
    // or `-n` would stray into this leg — its size must be asked of the kernel
    // first (see the whole-file leg below).
    if chunk_count.is_none() {
        let source = operands.first().copied();
        stream_outcome(
            &prog,
            source,
            split_lines(&prog, prefix, source, lines_per_chunk, sfxlen),
        );
    }

    let (input, read_failure) = match operands.first() {
        None => support::read_stdin_error(),
        Some(name) if *name == b"-" => support::read_stdin_error(),
        Some(name) => match support::read_operand(&String::from_utf8_lossy(name)) {
            (data, None) => (data, None),
            (_, Some(support::InputFailure::Open(code))) => {
                // C's `err(1, "%s", *argv)` (`split.c:137-138`): program name,
                // the operand's original text, and the `strerror` sentence —
                // all three parts present; the original-text part is the argv
                // bytes and is not converted.
                err3(&prog, name, code, 1);
            }
            (data, Some(support::InputFailure::Read(code))) => (data, Some(code)),
        },
    };

    // `-n`: exactly `n` byte chunks (`split3`); the last absorbs the remainder.
    // Both the chunk-count bound and the number in that message come from the
    // **size the kernel reports**, not from the bytes actually received: C
    // calls `fstat(ifd, &sb)` before reading (`split.c:293-300`), a failure is
    // `err(1, "stat")`, and `sb.st_size` then decides `chunks > st_size`. On a
    // pipe `st_size` is 0, so that form can only report an error, not split.
    if let Some(n) = chunk_count {
        let source = operands.first().copied().filter(|name| *name != b"-");
        let shown = source.map(|name| String::from_utf8_lossy(name).into_owned());
        let size = match support::input_size(shown.as_deref()) {
            Ok(size) => size,
            Err(code) => {
                // In C a failed `fstat` means `err(1, "stat")`: that segment
                // is the word itself, not a path.
                err3(&prog, b"stat", code, 1);
            }
        };
        if (n as u64) > size {
            support::warn(format!("{prog}: can't split into more than {size} files\n").as_bytes());
            support::terminate(1);
        }
        match chunks_n(&input, n) {
            Some(chunks) => {
                for (index, chunk) in chunks.iter().enumerate() {
                    let path = match out_path(prefix, index, sfxlen) {
                        Some(path) => path,
                        None => too_many_files(),
                    };
                    if let Err(failure) = create_write(&path, chunk) {
                        write_failure(&prog, &path, failure);
                    }
                }
            }
            // Reaching here with `n <= size` leaves only one form: fewer bytes
            // received than the size asked for. Same sentence and same number,
            // with no divergence from C's `errx`.
            None => {
                support::warn(
                    format!("{prog}: can't split into more than {size} files\n").as_bytes(),
                );
                support::terminate(1);
            }
        }
        finish(&prog, read_failure);
    }
    // The `-n` branch finishes itself above, and line mode and `-b` return on
    // the spot; this statement is the fallback for the `run() -> !` shape —
    // the compiler cannot see that those legs never come back here.
    finish(&prog, read_failure)
}

/// Finish up: in C a failed `read` is `err(1, "read")` on the spot, but C
/// writes as it reads, so earlier chunks are already on disk; this shell cuts
/// everything first and reports afterwards, with the same one-line shape
/// `split: read: <sentence>` and exit 1, or exit 0 when there was no error.
fn finish(prog: &str, read_failure: Option<minix_sys::Errno>) -> ! {
    if let Some(code) = read_failure {
        support::warn_line(prog, "read", code);
        support::terminate(1);
    }
    support::terminate(0)
}

/// Output path for chunk `index`: `prefix` + the `width`-letter suffix, or
/// `None` once the suffix space (`26^width`) is exhausted.
fn out_path(prefix: &[u8], index: usize, width: usize) -> Option<Vec<u8>> {
    // The prefix is the raw argv bytes and the suffix is base-26 letters; the
    // assembled path is kept as bytes, and conversion happens only at the
    // open step (`create_write`), so the two message arms can print the
    // original bytes.
    suffix(index, width).map(|suffix| [prefix, suffix.as_bytes()].concat())
}

/// Creates (or truncates) `path` and writes `bytes` into it, mirroring
/// `split.c`'s `open(O_WRONLY|O_CREAT|O_TRUNC, DEFFILEMODE)` plus the partial
/// write loop (`newfile()` and `bigwrite()`).
///
/// The seam goes through the twin faces of `bin_support::create_sink` (host
/// `std::fs::File::create`, target `minix_sys::open`): this file used to call
/// raw `minix_sys::open`/`write`/`close` directly, so **not a single chunk
/// file could be written on the host**, and `split`'s output leg had never
/// been tested at process level (the fifth instance of the same read-seam
/// defect, after the earlier `nl`, `fold`, `rev`, and `colrm` cases). It
/// returns `Errno`, because that C line has the three-part `err(3)` shape.
fn create_write(path: &[u8], bytes: &[u8]) -> Result<(), ChunkWrite> {
    let shown = String::from_utf8_lossy(path);
    let sink = support::create_sink(&shown).map_err(ChunkWrite::Open)?;
    write_into(sink, bytes)
}

/// A write-leg failure folds into `ChunkWrite::Write`, and the returned number
/// travels all the way to `write_failure` to be printed. This tiny function
/// exists only to pin the "failure to number" step on its own: in C a
/// `bigwrite()` that cannot match the count does `err(1, "write")`, and the
/// `err` suffix prints the `errno` itself — a short write and a failed write
/// are two different sentences.
fn write_into(sink: support::WriteSink, bytes: &[u8]) -> Result<(), ChunkWrite> {
    sink.finish(bytes).map_err(sink_failure_of)
}

/// Mapping each arm to its counterpart is pure, and it is split out so a gate
/// can pin it: only the target leg returns `Close`, which the host never
/// reaches, so the mapping itself needs its own gate (the split of duties
/// between `split.c:204` and `:319-322`).
fn sink_failure_of(failure: support::SinkFailure) -> ChunkWrite {
    match failure {
        support::SinkFailure::Write(code) => ChunkWrite::Write(code),
        support::SinkFailure::Close(code) => ChunkWrite::Close(code),
    }
}

/// The three failures of a chunk file: cannot open (C reports the path),
/// cannot write it out (C reports `write`), cannot close it after writing (C
/// reports the path). `Debug` exists so a gate's panic message can name the
/// arm.
#[derive(Debug, PartialEq, Eq)]
enum ChunkWrite {
    Open(minix_sys::Errno),
    Write(minix_sys::Errno),
    /// `close` failure: `fname` still points at the chunk just written, so the
    /// path is what gets reported (`split.c:319-322`).
    Close(minix_sys::Errno),
}

/// Each of the two failure exits uses the three-part `err(3)` shape and exits
/// 1 immediately after printing (C's `err` is itself the fatal branch).
fn write_failure(prog: &str, path: &[u8], failure: ChunkWrite) -> ! {
    let code = match failure {
        ChunkWrite::Open(code) | ChunkWrite::Write(code) | ChunkWrite::Close(code) => code,
    };
    err3(prog, failure_label(&failure, path), code, 1);
}

/// Each arm prints its own label: an open failure reports the **path** per
/// `split.c:332-333`'s `err(1, "%s", fname)`, while a write failure reports
/// **the word itself** per `err(1, "write")` at `split.c:204` and four other
/// places. It is a pure function so the "arms do not swap" rule has a gate:
/// the rendering step prints and exits, which the host side cannot observe.
fn failure_label<'a>(failure: &ChunkWrite, path: &'a [u8]) -> &'a [u8] {
    match failure {
        ChunkWrite::Open(_) | ChunkWrite::Close(_) => path,
        ChunkWrite::Write(_) => b"write".as_slice(),
    }
}

/// C's three-part `err(1, "%s", …)` / `err(1, "write")` shape: program name,
/// `: `, label, `: `, `strerror` sentence, newline, then exit. The label is
/// **bytes** — chunk paths and input operands both come from argv, and routing
/// them through `err_line(&str)` would fold raw bytes into U+FFFD, which then
/// fails byte-for-byte against the reference (measured for both `pr` and
/// `sort`).
fn err3(prog: &str, label: &[u8], code: minix_sys::Errno, status: i32) -> ! {
    let mut line = Vec::new();
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(label);
    line.extend_from_slice(b": ");
    line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
    line.push(b'\n');
    support::warn(&line);
    support::terminate(status)
}

/// C's `errx(1, "too many files.")` (`split.c:326-330`): the suffix space
/// (26 to the power `width`) is exhausted. The old form printed this shell's
/// own `suffixes exhausted` instead.
fn too_many_files() -> ! {
    support::warn(b"split: too many files.\n");
    support::terminate(1);
}

/// Shared finish for both streaming legs: if `stream_input` returns a failure,
/// report it with C's respective sentence; otherwise exit 0. An open failure is
/// `err(1, "%s", *argv)` (`split.c:137-138`) reporting the operand's original
/// text, and the stdin form never opens, so it can never reach this arm; a read
/// failure is `case -1:` doing `err(1, "read")` on the spot (`:188-190`), and
/// chunks already written stay on disk — C writes as it reads too.
fn stream_outcome(prog: &str, source: Option<&[u8]>, failure: Option<support::InputFailure>) -> ! {
    match failure {
        Some(support::InputFailure::Open(code)) => {
            if let Some(name) = source.filter(|name| *name != b"-") {
                err3(prog, name, code, 1);
            }
            support::terminate(1);
        }
        Some(support::InputFailure::Read(code)) => {
            err3(prog, b"read", code, 1);
        }
        None => support::terminate(0),
    }
}

/// The on-disk pipeline for chunk files: when the cutter says "start another
/// chunk", close the previous one, advance the suffix, open the next one, and
/// then pour this run of bytes in. The close happens **before** the next open
/// and reports the **previous** chunk's name (C's `newfile()`,
/// `split.c:309-322`: `close(ofd) != 0` does `err(1, "%s", fname)` while
/// `fname` has not advanced yet). Both streaming legs share this one copy: the
/// ordering and the three failure arms must be defined in exactly one place.
struct ChunkWriter<'a> {
    prog: &'a str,
    prefix: &'a [u8],
    width: usize,
    index: usize,
    current: Option<(Vec<u8>, support::WriteSink)>,
}

impl ChunkWriter<'_> {
    /// Write a run of bytes to the current chunk.
    fn write(&mut self, part: &CutPiece<'_>) {
        if part.opens_new_file {
            self.close_current();
            let path = match out_path(self.prefix, self.index, self.width) {
                Some(path) => path,
                None => too_many_files(),
            };
            self.index += 1;
            let sink = match support::create_sink(&String::from_utf8_lossy(&path)) {
                Ok(sink) => sink,
                Err(code) => write_failure(self.prog, &path, ChunkWrite::Open(code)),
            };
            self.current = Some((path, sink));
        }
        let (path, sink) = self.current.as_mut().expect("刚开过的分片必有落点");
        if let Err(failure) = sink.append(part.bytes) {
            write_failure(self.prog, path, sink_failure_of(failure));
        }
    }

    /// Close the chunk still open; a failed `close` reports the **path** as C
    /// does (`split.c:319-322`). The host leg cannot observe a `close` failure;
    /// that difference between the two legs is registered as
    /// `SPLIT-CLOSE-ERRLEG`.
    fn close_current(&mut self) {
        let Some((path, sink)) = self.current.take() else {
            return;
        };
        if let Err(failure) = sink.seal() {
            write_failure(self.prog, &path, sink_failure_of(failure));
        }
    }
}

/// Driver for the `-b` streaming leg: feed each block returned by
/// `bin_support::stream_input` to [`ByteCutter`], and let [`ChunkWriter`]
/// write the returned pieces to disk.
fn split_bytes(
    prog: &str,
    prefix: &[u8],
    source: Option<&[u8]>,
    size: usize,
    width: usize,
) -> Option<support::InputFailure> {
    let mut cutter = ByteCutter::new(size);
    let mut writer = ChunkWriter {
        prog,
        prefix,
        width,
        index: 0,
        current: None,
    };
    // The `&str` entry point for opening (registered as
    // `ARGV-BYTE-PATH-IN-LIBS`): only the path is converted; the chunk name
    // (the copy in `ChunkWriter`) and both messages still use the raw bytes.
    let shown = source.map(|name| String::from_utf8_lossy(name).into_owned());
    let outcome = support::stream_input(shown.as_deref(), STREAM_CHUNK, |piece| {
        for part in cutter.take(piece) {
            writer.write(&part);
        }
    });
    writer.close_current();
    outcome
}

/// Driver for the line-mode streaming leg: the read seam and on-disk pipeline
/// are the same as `-b`; only the cutter changes to [`LineCutter`]. Two numbers
/// still survive across reads — the whole lines counted in the current chunk
/// and whether the current chunk is open — with no data kept, so a line that
/// straddles read blocks is written in two pieces into the **same** open chunk
/// (`split.c:276-280`).
fn split_lines(
    prog: &str,
    prefix: &[u8],
    source: Option<&[u8]>,
    numlines: usize,
    width: usize,
) -> Option<support::InputFailure> {
    let mut cutter = LineCutter::new(numlines);
    let mut writer = ChunkWriter {
        prog,
        prefix,
        width,
        index: 0,
        current: None,
    };
    // The `&str` entry point for opening (registered as
    // `ARGV-BYTE-PATH-IN-LIBS`): only the path is converted; the chunk name
    // (the copy in `ChunkWriter`) and both messages still use the raw bytes.
    let shown = source.map(|name| String::from_utf8_lossy(name).into_owned());
    let outcome = support::stream_input(shown.as_deref(), STREAM_CHUNK, |piece| {
        for part in cutter.take(piece) {
            writer.write(&part);
        }
    });
    writer.close_current();
    outcome
}

/// The one scripted read source: hand out `blocks` and then error. Real
/// hardware cannot produce an input that "succeeds twice and fails the third
/// time", so both mid-stream-failure forms are fed to `drive_reads` through
/// this.
#[cfg(test)]
struct Scripted {
    blocks: Vec<&'static [u8]>,
    next: usize,
}

#[cfg(test)]
impl std::io::Read for Scripted {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if let Some(block) = self.blocks.get(self.next) {
            self.next += 1;
            let give = block.len().min(out.len());
            out[..give].copy_from_slice(&block[..give]);
            return Ok(give);
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "mid-stream failure",
        ))
    }
}

/// The read seam's ordering contract: blocks handed out **before** a mid-stream
/// failure are not withdrawn. Real hardware cannot produce an input that
/// "succeeds twice and fails the third time", so the read loop is pulled out as
/// `bin_support::drive_reads`, generic over any `Read`, and fed a read source
/// that errors by script — exactly the form that `split -b` takes after
/// streaming (same shape as C: `minix3/usr.bin/split/split.c:184-204` writes as
/// it reads and only does `err(1, "read")` on failure) and that previously had
/// no gate to pin it.
#[test]
fn pieces_handed_before_a_midstream_read_error_are_not_withdrawn() {
    use std::io::ErrorKind;
    // Each piece wants 5 bytes while the source gives only 2 at a time: when
    // the failure arrives the buffer holds 4 bytes, and that run must be
    // **handed out first**, then the error reported (C's `split1` has already
    // written it to the current chunk).
    let mut handed: Vec<Vec<u8>> = Vec::new();
    let outcome = support::drive_reads(
        &mut Scripted {
            blocks: vec![b"ab".as_slice(), b"cd".as_slice()],
            next: 0,
        },
        5,
        |piece| handed.push(piece.to_vec()),
    );
    assert_eq!(
        handed,
        vec![b"abcd".to_vec()],
        "失手之前攒在缓冲里的四字节必须已经交出去，不许随错误一起丢"
    );
    assert_eq!(
        outcome.err().map(|failure| failure.kind()),
        Some(ErrorKind::Other),
        "中途失手必须报回去，而不是当成读到尽头"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each of the three arms keeps its own mapping: a write failure to
    /// `write`, a close failure to **this chunk's name** (the open arm reports
    /// a name too, the same sentence shape). If two arms fold into one, or the
    /// number is flattened, this test turns red immediately.
    #[test]
    fn the_sink_arms_land_on_their_own_chunk_arms() {
        let write = minix_sys::Errno::from_i32(28);
        let close = minix_sys::Errno::from_i32(5);
        assert_eq!(
            sink_failure_of(support::SinkFailure::Write(write)),
            ChunkWrite::Write(write)
        );
        assert_eq!(
            sink_failure_of(support::SinkFailure::Close(close)),
            ChunkWrite::Close(close)
        );
    }

    #[test]
    fn the_two_write_arms_do_not_swap_their_labels() {
        // The open arm reports the path and the write arm reports the word
        // `write` (`split.c:332-333` and `:204` are two different `err`
        // calls); an arm swap only shows at the rendering step, so this pins
        // the choice itself.
        assert_eq!(
            failure_label(
                &ChunkWrite::Open(minix_sys::Errno::ENOENT),
                b"xaa".as_slice()
            ),
            b"xaa".as_slice()
        );
        assert_eq!(
            failure_label(
                &ChunkWrite::Write(minix_sys::Errno::from_i32(28)),
                b"xaa".as_slice()
            ),
            b"write".as_slice()
        );
        // The close arm reports **this chunk's name**, unlike the write arm
        // (`split.c:319-322`).
        assert_eq!(
            failure_label(
                &ChunkWrite::Close(minix_sys::Errno::from_i32(28)),
                b"xaa".as_slice()
            ),
            b"xaa".as_slice()
        );
    }

    /// The gate pins "is the number still there after the failure": a read-only
    /// handle wrapped as a write leg makes the host's `write` fail with
    /// `EBADF` (9) on the spot, and this shell must return 9 as
    /// `ChunkWrite::Write`. If it folded back to `EIO` or to a unit value here,
    /// `split` would print something other than C's sentence.
    #[test]
    fn a_write_failure_keeps_the_platform_number_for_the_write_leg() {
        let dir = std::env::temp_dir().join(format!("split-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("payload");
        std::fs::write(&path, b"seed").expect("seed file");
        let readable = std::fs::File::open(&path).expect("read-only handle");
        match write_into(support::WriteSink(readable), b"more") {
            Err(ChunkWrite::Write(code)) => {
                assert_eq!(code.to_i32(), 9, "应当原样交回 EBADF");
                assert_ne!(
                    code.to_i32(),
                    minix_sys::Errno::EIO.to_i32(),
                    "又被压成 EIO"
                );
            }
            Err(arm) => panic!("失败被归到了别的臂：{arm:?}"),
            Ok(()) => panic!("只读句柄上的写不可能成功"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn out_path_prefixes_the_two_letter_suffix() {
        assert_eq!(out_path(b"x", 0, 2).as_deref(), Some(b"xaa".as_slice()));
        assert_eq!(out_path(b"x", 1, 2).as_deref(), Some(b"xab".as_slice()));
        assert_eq!(
            out_path(b"part.", 26, 2).as_deref(),
            Some(b"part.ba".as_slice())
        );
        // -a 1: single-letter suffix.
        assert_eq!(out_path(b"x", 0, 1).as_deref(), Some(b"xa".as_slice()));
        assert_eq!(out_path(b"x", 26, 1), None);
    }

    /// The prefix is the raw argv bytes, and the assembled chunk name carries
    /// that raw byte too: a bare `ff` is not folded into the three bytes
    /// `ef bf bd`, nor blocked from name assembly by "must be text". The
    /// `&str` entry point for opening (registered as
    /// `ARGV-BYTE-PATH-IN-LIBS`) is not on this leg, so this pins **name
    /// assembly**: if folding happened here, the file name in the message
    /// would not be the same byte string the reference prints.
    #[test]
    fn a_raw_byte_in_the_prefix_survives_into_the_chunk_name() {
        assert_eq!(
            out_path(b"a\xff", 0, 2),
            Some([b"a\xff".as_slice(), b"aa".as_slice()].concat()),
            "分片名里那枚 ff 必须还是 ff，而不是 ef bf bd 三枚"
        );
        assert_eq!(
            out_path(b"a\xff", 1, 2),
            Some([b"a\xff".as_slice(), b"ab".as_slice()].concat())
        );
    }

    #[test]
    fn out_path_is_none_once_suffixes_exhaust() {
        // aa..zz is 676 names; index 675 is the last, 676 has none.
        assert!(out_path(b"x", 675, 2).is_some());
        assert_eq!(out_path(b"x", 676, 2), None);
    }

    /// A newly visible behavior after streaming: **the moment a read failure
    /// surfaces, earlier chunks are already on disk** (C's `split2` writes as it
    /// reads, `minix3/usr.bin/split/split.c:252-257`). This form cannot be
    /// produced end to end — a directory operand fails the very first `read`,
    /// so neither the whole-file nor the streaming version writes anything, and
    /// both pass the old gate — so it is pinned at the driver layer using the
    /// same machinery `split_lines` uses (`drive_reads` + `LineCutter` +
    /// `ChunkWriter`), the same layer as the existing read-seam gate.
    #[test]
    fn chunk_files_are_already_on_disk_when_the_read_fails_midstream() {
        let dir = std::env::temp_dir().join(format!("split-midstream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let prefix = dir.join("x").into_os_string().into_encoded_bytes();
        let mut cutter = LineCutter::new(1);
        let mut writer = ChunkWriter {
            prog: "split",
            prefix: &prefix,
            width: 2,
            index: 0,
            current: None,
        };
        // One read wants 8 bytes but the source errors after only 3: the
        // accumulated `a\nb` must be handed out first, then the error reported.
        let outcome = support::drive_reads(
            &mut Scripted {
                blocks: vec![b"a\nb".as_slice()],
                next: 0,
            },
            8,
            |piece| {
                for part in cutter.take(piece) {
                    writer.write(&part);
                }
            },
        );
        assert_eq!(
            outcome.err().map(|failure| failure.kind()),
            Some(std::io::ErrorKind::Other),
            "中途失手必须报回去，而不是当成读到尽头"
        );
        // The moment the error surfaces: `xaa` is full and closed, `xab` is
        // still open but that half line is already in the file.
        assert_eq!(
            std::fs::read(dir.join("xaa")).expect("`xaa` 该在失手之前就落盘"),
            b"a\n"
        );
        assert_eq!(
            std::fs::read(dir.join("xab")).expect("`xab` 该带着已交出的半行落盘"),
            b"b",
            "已经交出去的片段不许随错误一起收回"
        );
        writer.close_current();
        let mut names = std::fs::read_dir(&dir)
            .expect("临时目录读得出")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<String>>();
        names.sort();
        assert_eq!(
            names,
            vec!["xaa".to_string(), "xab".to_string()],
            "收尾之后只剩这两枚分片，不许多出一枚空的"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn usage(prog: &str) -> ! {
    // C's `usage()` (`split.c:355-361`): the program name follows
    // `getprogname()`, one whole line, `exit(1)`. The old form got both wrong:
    // the sentence was this shell's own short form and the exit code was 2.
    support::warn(
        format!(
            "usage: {prog} [-b byte_count] [-l line_count] [-n chunk_count] \
[-a suffix_length] [file [prefix]]\n"
        )
        .as_bytes(),
    );
    support::terminate(1);
}

/// Map a [`SplitError`] to the exact C path (which line, how many lines, exit
/// code).
fn arg_error(prog: &str, e: SplitError<'_>) -> ! {
    match e {
        SplitError::Usage => usage(prog),
        SplitError::UnknownOption(letter) => {
            support::warn(&GetoptReport::UnknownOption(letter).line_bytes(prog));
            usage(prog)
        }
        SplitError::MissingValue(letter) => {
            support::warn(&GetoptReport::MissingValue(letter).line_bytes(prog));
            usage(prog)
        }
        SplitError::IllegalLineCount(v) => {
            // C uses the "value first, sentence after" form of the
            // `errx(1, "%s: illegal line co %s", …)` family:
            // `{value}: illegal line count.`, with Minix's `errx` prefixing
            // the program name. The value position is the raw argv bytes —
            // one conversion and `é` becomes three bytes, a bare `ff` too.
            let mut line = Vec::new();
            line.extend_from_slice(prog.as_bytes());
            line.extend_from_slice(b": ");
            line.extend_from_slice(v);
            line.extend_from_slice(b": illegal line count.\n");
            support::warn(&line);
            support::terminate(1);
        }
        SplitError::IllegalByteCount(v) => {
            // C uses the "value first, sentence after" form of the
            // `errx(1, "%s: illegal byte co %s", …)` family:
            // `{value}: illegal byte count.`, with Minix's `errx` prefixing
            // the program name. The value position is the raw argv bytes —
            // one conversion and `é` becomes three bytes, a bare `ff` too.
            let mut line = Vec::new();
            line.extend_from_slice(prog.as_bytes());
            line.extend_from_slice(b": ");
            line.extend_from_slice(v);
            line.extend_from_slice(b": illegal byte count.\n");
            support::warn(&line);
            support::terminate(1);
        }
        SplitError::IllegalChunkCount(v) => {
            // C uses the "value first, sentence after" form of the
            // `errx(1, "%s: illegal number of chu %s", …)` family:
            // `{value}: illegal number of chunks.`, with Minix's `errx`
            // prefixing the program name. The value position is the raw argv
            // bytes — one conversion and `é` becomes three bytes, a bare `ff`
            // too.
            let mut line = Vec::new();
            line.extend_from_slice(prog.as_bytes());
            line.extend_from_slice(b": ");
            line.extend_from_slice(v);
            line.extend_from_slice(b": illegal number of chunks.\n");
            support::warn(&line);
            support::terminate(1);
        }
        SplitError::IllegalSuffixLength(v) => {
            // C uses the "value first, sentence after" form of the
            // `errx(1, "%s: illegal suffix len %s", …)` family:
            // `{value}: illegal suffix length.`, with Minix's `errx` prefixing
            // the program name. The value position is the raw argv bytes —
            // one conversion and `é` becomes three bytes, a bare `ff` too.
            let mut line = Vec::new();
            line.extend_from_slice(prog.as_bytes());
            line.extend_from_slice(b": ");
            line.extend_from_slice(v);
            line.extend_from_slice(b": illegal suffix length.\n");
            support::warn(&line);
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

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

/// 一次读进来的字节数，取自 C 的 `MAXBSIZE`（`minix3/tools/compat/compat_defs.h:1151` 写作 `64 * 1024`）。
const STREAM_CHUNK: usize = 64 * 1024;

use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::GetoptReport;
use minix_textfilter::split::{
    ByteCutter, CutPiece, LineCutter, SplitError, chunks_n, parse_split_args, suffix,
};

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

    // 第三支操作数的判据在 C 那里排在开档之后（`split.c:135-148`）：先 `open(*argv)`，
    // 开不成是 `err(1, "%s", *argv)` 带着档名退出，开成了才轮到 `usage()`。
    // `-` 不走开档（`:136` 为它开特例），所以它那一形直接落进 usage。
    // 探测只在这条错误腿里发生，正常切分仍是后面各腿自己开一次档。
    if operands.len() > 2 {
        if let Some(name) = operands.first().filter(|name| **name != "-")
            && let Err(code) = support::open_input(name)
        {
            support::warn_line(prog, name, code);
            support::terminate(1);
        }
        usage(prog);
    }

    // `split.c:135-139`: `-` reads stdin (split, unlike grep/sed, special-cases
    // it), a named input is opened, no operand reads stdin, and an unopenable
    // input is fatal (`err(1, "%s", *argv)`).
    // 读腿的三形各归各位：没有操作数是标准输入，裸 `-` 也是标准输入（`split.c:136`
    // 为它开了特例），其余按名字开档。开档失手当场 `err(1, "%s", *argv)` 退出；
    // `read` 失手要先把已经读到的切出去、再补一句 `err(1, "read")`（`:185-190`、
    // `:252-257`），所以那个号带着读到的字节一起往下走。
    // `-b`: 按字节定长切分，走 C 的 `split1` 那条**流式**腿——一次读一块、边读边写，
    // 跨读只留一个计数器（`split.c:175-236`）。它必须在整档读之前分岔，否则
    // 「内存里有整个输入」这件事会把这条腿的用量重新拉回去。
    if let Some(n) = byte_size {
        let source = operands.first().copied();
        stream_outcome(prog, source, split_bytes(prog, prefix, source, n, sfxlen));
    }

    // 默认与 `-l` 走 C 的 `split2` 那条**流式**腿：一次读一块，跨读只留「当前分片已数到
    // 几整行」与「当前分片是否还开着」两个数（`split.c:242-282`）。行模式吃的是原始字节，
    // `split2` 只在缓冲里数换行、从不校验编码，所以不可解码的字节只是普通数据；换行留在
    // 它结束的那一行上，末行不带换行就不带。分岔位置与 `-b` 同一条理由：整档读会把这条腿
    // 的用量重新拉回「内存里有整个输入」。判据取「不是 `-n`」而不是「有列数」，否则 `-n`
    // 会误入这条腿——它的规模要先问内核（见下面的全量腿）。
    if chunk_count.is_none() {
        let source = operands.first().copied();
        stream_outcome(
            prog,
            source,
            split_lines(prog, prefix, source, lines_per_chunk, sfxlen),
        );
    }

    let (input, read_failure) = match operands.first() {
        None => support::read_stdin_error(),
        Some(name) if *name == "-" => support::read_stdin_error(),
        Some(name) => match support::read_operand(name) {
            (data, None) => (data, None),
            (_, Some(support::InputFailure::Open(code))) => {
                // C 的 `err(1, "%s", *argv)`（`split.c:137-138`）：程序名、操作数原文、
                // `strerror` 句子三段齐备。
                support::warn_line(prog, name, code);
                support::terminate(1);
            }
            (data, Some(support::InputFailure::Read(code))) => (data, Some(code)),
        },
    };

    // `-n`: exactly `n` byte chunks (`split3`); the last absorbs the remainder.
    // 块数上限与那句报文里的数都取自**内核给的大小**，不取自已收到的字节数：C 在读之前
    // `fstat(ifd, &sb)`（`split.c:293-300`），失手是 `err(1, "stat")`，`sb.st_size` 再判
    // `chunks > st_size`。管道上的 `st_size` 是 0，于是那一形只能报错而不能切。
    if let Some(n) = chunk_count {
        let source = operands.first().copied().filter(|name| *name != "-");
        let size = match support::input_size(source) {
            Ok(size) => size,
            Err(code) => {
                support::warn_line(prog, "stat", code);
                support::terminate(1);
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
                        write_failure(prog, &path, failure);
                    }
                }
            }
            // 走到这里 `n <= size`，剩下的只有「收到的字节比问到的少」这一形：
            // 同一句话、同一个数，与 C 的 `errx` 不分岔。
            None => {
                support::warn(
                    format!("{prog}: can't split into more than {size} files\n").as_bytes(),
                );
                support::terminate(1);
            }
        }
        finish(prog, read_failure);
    }
    // `-n` 那一支在上面自己收尾，行模式与 `-b` 更是当场就退；这一句是给
    // `run() -> !` 的形状兜底——编译器看不出那几条腿已经不会回到这里。
    finish(prog, read_failure)
}

/// 收尾：C 的 `read` 失手是当场 `err(1, "read")`，但它边读边写，前面的分片已在盘上；
/// 本壳先切完再报，形状同为一行 `split: read: <句子>` 加退 1，无错即退 0。
fn finish(prog: &str, read_failure: Option<minix_sys::Errno>) -> ! {
    if let Some(code) = read_failure {
        support::warn_line(prog, "read", code);
        support::terminate(1);
    }
    support::terminate(0)
}

/// Output path for chunk `index`: `prefix` + the `width`-letter suffix, or
/// `None` once the suffix space (`26^width`) is exhausted.
fn out_path(prefix: &str, index: usize, width: usize) -> Option<String> {
    suffix(index, width).map(|suffix| format!("{prefix}{suffix}"))
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
    write_into(sink, bytes)
}

/// 写腿的失败折成 `ChunkWrite::Write`，交回的号一路带到 `write_failure` 打出去。
/// 拆成这一小函数只为把「失败到号」这一步单独钉住：C 那边是 `bigwrite()` 对不上数
/// 就 `err(1, "write")`，`err` 后缀打的就是 `errno`，写满与写错是两句话。
fn write_into(sink: support::WriteSink, bytes: &[u8]) -> Result<(), ChunkWrite> {
    sink.finish(bytes).map_err(sink_failure_of)
}

/// 两臂的对号入座是纯映射，拆出来才能被门钉住：真机那条腿才会交回 `Close`，
/// 宿主跑不到，于是映射本身必须有门（`split.c:204` 与 `:319-322` 的分工）。
fn sink_failure_of(failure: support::SinkFailure) -> ChunkWrite {
    match failure {
        support::SinkFailure::Write(code) => ChunkWrite::Write(code),
        support::SinkFailure::Close(code) => ChunkWrite::Close(code),
    }
}

/// 分片文件的三种失败：开不开（C 报路径）、写不写进（C 报 `write`）、
/// 写完关不关得掉（C 报路径）。带 `Debug` 是为了让门的 panic 消息说得出是哪一臂。
#[derive(Debug, PartialEq, Eq)]
enum ChunkWrite {
    Open(minix_sys::Errno),
    Write(minix_sys::Errno),
    /// `close` 失败：`fname` 还停在刚写完的那枚分片上，报的是路径（`split.c:319-322`）。
    Close(minix_sys::Errno),
}

/// 两个失败出口各自的 `err(3)` 三段形，印完即退 1（C 的 `err` 本身就是致命支）。
fn write_failure(prog: &str, path: &str, failure: ChunkWrite) -> ! {
    let code = match failure {
        ChunkWrite::Open(code) | ChunkWrite::Write(code) | ChunkWrite::Close(code) => code,
    };
    support::warn_line(prog, failure_label(&failure, path), code);
    support::terminate(1);
}

/// 两臂各打各的标签：开档失败按 `split.c:332-333` 的 `err(1, "%s", fname)` 报**路径**，
/// 写失败按 `split.c:204` 等五处的 `err(1, "write")` 报**这个词本身**。拆成纯函数是为了
/// 让「两臂不互换」这一条有门可钉——渲染那一步要打要退出，宿主侧没法看。
fn failure_label<'a>(failure: &ChunkWrite, path: &'a str) -> &'a str {
    match failure {
        ChunkWrite::Open(_) | ChunkWrite::Close(_) => path,
        ChunkWrite::Write(_) => "write",
    }
}

/// C 的 `errx(1, "too many files.")`（`split.c:326-330`）：后缀空间（26 的 width 次方）
/// 用完。旧写法印的是本壳自拟的 `suffixes exhausted`。
fn too_many_files() -> ! {
    support::warn(b"split: too many files.\n");
    support::terminate(1);
}

/// 两条流式腿共用的收尾：`stream_input` 交回失手就按 C 的两句话分别报，无失手即退 0。
/// 开档失手是 `err(1, "%s", *argv)`（`split.c:137-138`）报操作数原文，而标准输入那一形
/// 不 open，所以永远不会走到这一臂；读失手是 `case -1:` 当场 `err(1, "read")`（`:188-190`），
/// 此前已经写出去的分片照旧留在盘上——C 也是边读边写。
fn stream_outcome(prog: &str, source: Option<&str>, failure: Option<support::InputFailure>) -> ! {
    match failure {
        Some(support::InputFailure::Open(code)) => {
            if let Some(name) = source.filter(|name| *name != "-") {
                support::warn_line(prog, name, code);
            }
            support::terminate(1);
        }
        Some(support::InputFailure::Read(code)) => {
            support::warn_line(prog, "read", code);
            support::terminate(1);
        }
        None => support::terminate(0),
    }
}

/// 分片文件的落盘管线：切块器说「另起一枚」就先把上一枚关掉、后缀推进一位、再开下一枚，
/// 然后把这段字节灌进去。关档发生在开下一枚**之前**，报的是**上一枚**的名字（C 的
/// `newfile()`，`split.c:309-322`：`close(ofd) != 0` 就 `err(1, "%s", fname)`，此时
/// `fname` 还没推进）。两条流式腿共用这一份：这套次序与三种失败臂只能有一处定义。
struct ChunkWriter<'a> {
    prog: &'a str,
    prefix: &'a str,
    width: usize,
    index: usize,
    current: Option<(String, support::WriteSink)>,
}

impl ChunkWriter<'_> {
    /// 把一段字节写给当前分片。
    fn write(&mut self, part: &CutPiece<'_>) {
        if part.opens_new_file {
            self.close_current();
            let path = match out_path(self.prefix, self.index, self.width) {
                Some(path) => path,
                None => too_many_files(),
            };
            self.index += 1;
            let sink = match support::create_sink(&path) {
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

    /// 关掉还开着的那一枚；`close` 失手按 C 报**路径**（`split.c:319-322`）。
    /// 宿主这条腿看不见 `close` 的失败，两腿差异已在 `SPLIT-CLOSE-ERRLEG` 登记。
    fn close_current(&mut self) {
        let Some((path, sink)) = self.current.take() else {
            return;
        };
        if let Err(failure) = sink.seal() {
            write_failure(self.prog, &path, sink_failure_of(failure));
        }
    }
}

/// `-b` 那条流式腿的驱动：把 `bin_support::stream_input` 交回来的每一块喂给
/// [`ByteCutter`]，交回的片段由 [`ChunkWriter`] 落盘。
fn split_bytes(
    prog: &str,
    prefix: &str,
    source: Option<&str>,
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
    let outcome = support::stream_input(source, STREAM_CHUNK, |piece| {
        for part in cutter.take(piece) {
            writer.write(&part);
        }
    });
    writer.close_current();
    outcome
}

/// 行模式那条流式腿的驱动：读缝与落盘管线与 `-b` 同一份，只把切块器换成 [`LineCutter`]。
/// 跨两次读留下的还是两个数——当前分片已数到几整行、当前分片是否还开着——一段数据都不留，
/// 所以横跨读块的那一行是分两次写进**同一枚**还开着的分片（`split.c:276-280`）。
fn split_lines(
    prog: &str,
    prefix: &str,
    source: Option<&str>,
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
    let outcome = support::stream_input(source, STREAM_CHUNK, |piece| {
        for part in cutter.take(piece) {
            writer.write(&part);
        }
    });
    writer.close_current();
    outcome
}

/// 只此一份的脚本化读源：把 `blocks` 给完就报错。真机上造不出「读两次成功、
/// 第三次失手」的输入，所以「中途失手」那两形都由它喂给 `drive_reads`。
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

/// 读缝的顺序契约：中途失手**之前**交出去的块不收回。真机上造不出「读两次成功、
/// 第三次失手」的输入，所以把读循环抽成对任意 `Read` 通用的
/// `bin_support::drive_reads`，用一枚脚本报错的读源喂它——这正是 `split -b`
/// 流式化之后与 C 同形（`minix3/usr.bin/split/split.c:184-204` 边读边写，
/// 失手才 `err(1, "read")`）、以前没有门可钉的那一形。
#[test]
fn pieces_handed_before_a_midstream_read_error_are_not_withdrawn() {
    use std::io::ErrorKind;
    // 每片要 5 字节，而读源每次只给 2 字节：失手来临时缓冲里正攒着 4 字节，
    // 那一截必须**先交出去**再报错（C 的 `split1` 已经把它写给当前分片了）。
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

    /// 三臂各归各的：写失败对 `write`，close 失败对**这一枚分片的名字**（开档臂
    /// 报的也是名字，两者同句形）。若两臂折成一臂、或号被压平，这一枚当场变红。
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
        // 开档臂报路径、写臂报 `write` 这个词（`split.c:332-333` 与 `:204` 是两句不同的
        // `err`），两臂互换在渲染那一步才看得见，这里钉住选择本身
        assert_eq!(
            failure_label(&ChunkWrite::Open(minix_sys::Errno::ENOENT), "xaa"),
            "xaa"
        );
        assert_eq!(
            failure_label(&ChunkWrite::Write(minix_sys::Errno::from_i32(28)), "xaa"),
            "write"
        );
        // close 臂报的是**这一枚分片的名字**，与写臂不同（`split.c:319-322`）
        assert_eq!(
            failure_label(&ChunkWrite::Close(minix_sys::Errno::from_i32(28)), "xaa"),
            "xaa"
        );
    }

    /// 门钉的是「失败之后号还在不在」：一枚只读句柄包成写腿，宿主那次 `write` 当场
    /// `EBADF`（9 号），本壳必须把 9 交回 `ChunkWrite::Write`。若这里又折回 `EIO`
    /// 或折成单位，`split` 打出去的就不是 C 那一句。
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

    /// 流式化之后新出现的可见行为：**读失手浮出来的那一刻，前面的分片已经在盘上了**
    /// （C 的 `split2` 边读边写，`minix3/usr.bin/split/split.c:252-257`）。这一形端到端
    /// 造不出来——目录操作数第一圈 `read` 就失手，全量版与流式版都不落盘，所以两版都得
    /// 那枚旧门绿——于是按 `split_lines` 用的同一台机器（`drive_reads` + `LineCutter` +
    /// `ChunkWriter`）在驱动层钉，与批次三百五十八为读缝立的门同一层。
    #[test]
    fn chunk_files_are_already_on_disk_when_the_read_fails_midstream() {
        let dir = std::env::temp_dir().join(format!("split-midstream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let prefix = dir
            .join("x")
            .to_str()
            .expect("临时路径可作前缀")
            .to_string();
        let mut cutter = LineCutter::new(1);
        let mut writer = ChunkWriter {
            prog: "split",
            prefix: &prefix,
            width: 2,
            index: 0,
            current: None,
        };
        // 一次读要 8 字节，读源只给 3 字节就报错：攒下的 `a\nb` 必须先交出去再报。
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
        // 报错浮出的这一刻：`xaa` 已经切满并关上，`xab` 还开着但那半行已在档里。
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
            support::warn(&GetoptReport::UnknownOption(letter).line_bytes(prog));
            usage(prog)
        }
        SplitError::MissingValue(letter) => {
            support::warn(&GetoptReport::MissingValue(letter).line_bytes(prog));
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

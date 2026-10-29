//! Head/tail line windows behind one trait.
//!
//! Ground truth: `head` keeps the first N lines, `tail` the last N. Both
//! answer "which lines survive?" from opposite ends, so both implement
//! [`LineWindow`]. The caller feeds lines in order and drains the survivors
//! at the end; the window never holds more than N borrowed lines.
//!
//! N is the caller's count, with no ceiling of its own: C `head.c` counts down
//! from it and C `forward.c` + `RLINES` walks back through the mapped file for
//! any of them. The storage therefore grows as lines arrive and stops at N, so a
//! `tail -n 1000000` on a twenty-line file holds twenty lines, not a million
//! slots — the count cannot be turned into an allocation bomb.

use crate::TextError;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// 一行：内容（**按字节**）加上它在输入里到底有没有那枚换行。
///
/// C 的行腿把串尾的换行连同内容一起交回，从不替输入补：`nl.c:288` 的 `fgets` 连着换行
/// 一起读进缓冲、`:340` 的 `printf("%s%s", sep, buffer)` 原样打出；`head.c:169-172` 干脆
/// 逐字符 `getc`/`putchar` 搬运；`reverse.c:133-144` 记录的行区间也含那枚换行。所以
/// 「末行没有换行」是**输入的属性**，不是程序要填的空。
///
/// 内容为什么是 `&[u8]` 而不是 `&str`：C 的这些行腿一个字节也不解读——`fgetln`、`fgets`、
/// `getc` 交回的就是字节。行一旦变成 `&str`，一枚非法 UTF-8 字节就要求先做一次解码，而那一步
/// 无论怎么取舍都在改输入：整份解失败就一份也交不出，逐行解失败就把那一行换成别的东西。
/// 字节没有这个问题，所以取法侧（切列、比较、计数）与写出侧都在同一份字节上工作。
///
/// 中间行一定带着换行，只有输入的最后一行可能没有。把终止状态做成一行数据之后，写出侧只看
/// [`Line::eol`]，谁都不必再问「我现在写的这行是不是输入的末行」——`tail -r` 会把末行翻到
/// 输出的最前面，靠行号问这句话正好会问错。参照件现量读数（`tmp/head-415`、`tmp/tail-416`、
/// `tmp/nl-417`、`tmp/uniq-408`）与两个例外（`cut`/`rev` 在 C 里**确实**归一化补换行）见
/// 18 篇批次四百二十三。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    bytes: &'a [u8],
    terminated: bool,
}

impl<'a> Line<'a> {
    /// 内容（不含换行）加上「输入里有那枚换行」。
    pub fn new(bytes: &'a [u8]) -> Line<'a> {
        Line {
            bytes,
            terminated: true,
        }
    }

    /// 按输入原样交回一行：末行没有换行时 `terminated` 为假。
    pub fn raw(bytes: &'a [u8], terminated: bool) -> Line<'a> {
        Line { bytes, terminated }
    }

    /// 行的内容，按字节。
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    /// 写出这一行时该跟的字节：输入里有就原样交回，没有就**不造**。
    pub fn eol(&self) -> &'static [u8] {
        if self.terminated { b"\n" } else { b"" }
    }
    /// 这一行在输入里是否带着换行。
    pub fn terminated(&self) -> bool {
        self.terminated
    }
}

impl<'a> From<&'a str> for Line<'a> {
    /// 借一段文本当「带着换行的行」：测试与只会造内容的调用方走这条。
    fn from(text: &'a str) -> Line<'a> {
        Line::new(text.as_bytes())
    }
}

/// 输入的末行在输入里有没有那枚换行——一行判据。按字节自己切行的腿（`nl` 的编号腿要容忍
/// 非 UTF-8，不走 [`Line`]）用它，`lines_of` 也用它给末行定 `terminated`，两边同源于此。
/// 空输入没有行，交回 `true`（补与不补都走不到任何一行）。
pub fn final_line_terminated(input: &[u8]) -> bool {
    input.is_empty() || input.ends_with(b"\n")
}

/// 按字节切输入，交回每一行与它在输入里的终止状态——**不做任何解码**。
///
/// `input` 为空交回空表（没有行）。输入以换行结尾时，`split` 多交回的那片空串是
/// 换行之后的东西、不是行；两枚相邻换行之间那片空串仍是真行。
///
/// 这里为什么不先把整份输入解成 `&str` 再切：解一次就要求「整份都是合法 UTF-8」，
/// 一枚非法字节会让整份输入变成空串，于是每一行都消失——参照件对同样的输入照旧逐行
/// 交回（`tmp/head-415/head_ref`、`tmp/nl-417/nl_ref` 现量：含 `\xff` 的行连字节一起打）。
/// 按字节切则行是输入自己的切分，与内容是不是 UTF-8 无关。
pub fn lines_of(input: &[u8]) -> Vec<Line<'_>> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut pieces: Vec<Line<'_>> = input.split(|byte| *byte == b'\n').map(Line::new).collect();
    if final_line_terminated(input) {
        // 换行结尾时 `split` 多交回一片空串，它是换行**之后**的东西，不是行；
        // 内部那片空串（两枚相邻换行之间）仍是真行。
        pieces.pop();
    } else if let Some(last) = pieces.last_mut() {
        // 末行在输入里没有换行：把这件事记在行上，写出的腿就不会去造它。
        *last = Line::raw(last.bytes(), false);
    }
    pieces
}

/// One line selection strategy: feed lines in order, drain survivors.
pub trait LineWindow<'a> {
    /// Offer one line (1 based number for diagnostics); survivors stay
    /// inside the window.
    fn push(&mut self, line: Line<'a>);
    /// Survivors in original order; call once after the last push.
    fn drain(&mut self, out: &mut [Line<'a>]) -> usize;
    /// How many lines were offered in total.
    fn seen(&self) -> u64;
}

/// Keep the first N lines (`head -n`).
pub struct HeadWindow<'a> {
    lines: Vec<Line<'a>>,
    kept: usize,
    limit: usize,
    seen: u64,
}

impl<'a> HeadWindow<'a> {
    /// A window keeping the first `n` lines. Only `0` is refused: `head.c`'s
    /// counter never starts, which the callers treat as "print nothing" rather
    /// than as a window request.
    pub fn new(n: usize) -> Result<Self, TextError> {
        if n == 0 {
            return Err(TextError::InvalidArgument);
        }
        Ok(HeadWindow {
            lines: Vec::new(),
            kept: 0,
            limit: n,
            seen: 0,
        })
    }
}

impl<'a> LineWindow<'a> for HeadWindow<'a> {
    fn push(&mut self, line: Line<'a>) {
        self.seen += 1;
        if self.kept < self.limit {
            self.lines.push(line);
            self.kept += 1;
        }
    }

    fn drain(&mut self, out: &mut [Line<'a>]) -> usize {
        let count = self.kept.min(out.len());
        out[..count].copy_from_slice(&self.lines[..count]);
        count
    }

    fn seen(&self) -> u64 {
        self.seen
    }
}

/// Keep the last N lines (`tail -n`): a ring holding the most recent N.
pub struct TailWindow<'a> {
    ring: Vec<Line<'a>>,
    limit: usize,
    seen: u64,
}

impl<'a> TailWindow<'a> {
    /// A window keeping the last `n` lines. `0` is refused (C `forward.c` seeks
    /// to the end of the file in that case and prints nothing, so no window is
    /// needed); there is no upper bound, matching `RLINES`' backward walk.
    pub fn new(n: usize) -> Result<Self, TextError> {
        if n == 0 {
            return Err(TextError::InvalidArgument);
        }
        Ok(TailWindow {
            ring: Vec::new(),
            limit: n,
            seen: 0,
        })
    }
}

impl<'a> LineWindow<'a> for TailWindow<'a> {
    fn push(&mut self, line: Line<'a>) {
        // Fill before wrapping, so a count larger than the input costs the
        // input's size and not the count.
        if self.ring.len() < self.limit {
            self.ring.push(line);
        } else {
            self.ring[(self.seen as usize) % self.limit] = line;
        }
        self.seen += 1;
    }

    fn drain(&mut self, out: &mut [Line<'a>]) -> usize {
        let kept = (self.seen as usize).min(self.limit).min(out.len());
        if kept == 0 {
            return 0;
        }
        // Oldest survivor first: it sits right past the write cursor when
        // the ring wrapped, else at slot zero.
        let start = if (self.seen as usize) > self.limit {
            (self.seen as usize) % self.limit
        } else {
            0
        };
        for index in 0..kept {
            out[index] = self.ring[(start + index) % self.limit];
        }
        kept
    }

    fn seen(&self) -> u64 {
        self.seen
    }
}

/// Parsed `head` options and file operands — the deciding half's front door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadArgs {
    /// `-n` line count, or the default ten.
    pub count: usize,
    /// `-c` byte count; `0` (the default) selects line mode.
    pub bytecnt: usize,
    /// `-q`: never print per-file banners.
    pub qflag: bool,
    /// `-v`: always print per-file banners.
    pub vflag: bool,
    /// File operands in order (a lone `-` names a file, unlike `cut`).
    /// 交回的是 argv 里那串**原字节**：横幅那一腿由壳自己拼（`head.c:130-131` 的
    /// `printf("%s==> %s <==\\n", …, *argv)`），所以它可以逐字节回显；只有开档与
    /// `warn("%s", *argv)` 那两条 `&str` 口要折一次（在册 `ARGV-BYTE-PATH-IN-LIBS`）。
    pub operands: Vec<Vec<u8>>,
}

/// `head` argument-parsing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadArgError {
    /// 集外字母与带值选项缺值。head 没有自己的裸 usage 腿（C 全文只有一处
    /// `usage()`，就在 `head.c:115-117` 的 `case '?'`/`default:` 里），而那一支前面
    /// 还有 libc 自己印的一行（`getopt.c:101-104`、`:132-136`），所以本型只留
    /// 这一个出口，不另设 `Usage`。
    Getopt(crate::GetoptReport),
    /// A `-c`/`-n` value that is not a strictly positive integer: trailing
    /// garbage, `<= 0`, or overflow (C `strtoimax` reject, `head.c:90-103`).
    IllegalCount {
        /// `true` for a `-c` (byte) count, `false` for `-n` (line).
        byte: bool,
        /// The offending value **as the argv bytes**, echoed into C's
        /// `errx(1, "illegal … count -- %s", optarg)`（`head.c:96-97`、`:102-103`）。
        value: Vec<u8>,
    },
}

/// Split `head`'s argv tail into [`HeadArgs`], faithful to C's
/// `obsolete(argv)` pre-pass (`head.c:84`, defined `head.c:178-195`) plus
/// `getopt(argc, argv, "c:n:qv")` (`head.c:87`):
/// - `obsolete` rewrites only the **leading run** of `-<digit>…` tokens into
///   `-n<digit>…`; its `while` returns at the first token that is not one (a
///   file, `--`, or a non-digit flag). So `head -5 f` becomes `head -n5 f`,
///   while `head f -5` is not rewritten at all.
/// - `getopt` **does not permute**: it looks at one token per call and, when
///   that token is absent or does not start with `-`, ends option scanning right
///   there (`getopt.c:69-75` returns `-1` with `optind` left in place). The
///   caller then takes `argv + optind` as its operands (`head.c:118-119`), so
///   **every token from the first non-option on is an operand** — a later `-q`
///   never sets `qflag`, a later `-x` never reaches the unknown-letter arm, a
///   later `-n 3` never changes the count, and a later `--` is a file named
///   `--`. Only inside a leading option cluster does `getopt` keep consuming:
///   letters cluster (`-qv` ≡ `-q -v`), a colon option's value attaches to its
///   own token (`-n5`) or is the next token (`-n 5`), a bare `--` as the leading
///   token ends scanning so everything after it is an operand, and a solitary
///   `-` is treated as a non-option (`getopt.c:96-104`: `head` does not list `-`
///   in its option string) and therefore becomes an operand too.
/// - `-q`/`-v` are mutually exclusive with the last one winning
///   (`head.c:105-113`); any other letter inside a leading option token is
///   [`HeadArgError::Getopt`] with that letter, because libc prints its own line
///   before `usage()` runs.
pub fn parse_head_args(args: &[&[u8]]) -> Result<HeadArgs, HeadArgError> {
    // Phase 1 — obsolete(): rewrite the leading `-<digit>…` run, stop at the
    // first non-matching token and copy the remainder verbatim.
    let mut scanned: Vec<Vec<u8>> = Vec::new();
    let mut still_obsolete = true;
    for arg in args {
        if still_obsolete
            && arg.len() >= 2
            && arg[0] == b'-'
            && arg[1] != b'-'
            && arg[1].is_ascii_digit()
        {
            // C 的 `obsolete()` 拼的是字符数组（`head.c:188-192` 的 `-`、`n` 加
            // `strcpy(ap + 2, *argv + 1)`），搬到这里就是「前缀两枚字节 + 原串去掉
            // 短横后的全部字节」——尾巴里那枚非 UTF-8 字节必须原样跟着走，它随后会被
            // `strtoimax` 拒掉并**回显进报文**（`head.c:96-97` 的 `illegal ... -- %s`）。
            let mut rewritten: Vec<u8> = b"-n".to_vec();
            rewritten.extend_from_slice(&arg[1..]);
            scanned.push(rewritten);
        } else {
            still_obsolete = false;
            scanned.push((*arg).to_vec());
        }
    }

    // Phase 2 — getopt("c:n:qv").
    let mut result = HeadArgs {
        count: 10,
        bytecnt: 0,
        qflag: false,
        vflag: false,
        operands: Vec::new(),
    };
    let mut index = 0;
    while index < scanned.len() {
        let token: &[u8] = &scanned[index];
        if token == b"--" {
            result.operands.extend_from_slice(&scanned[index + 1..]);
            break;
        }
        let bytes = token;
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'c' | b'n' => {
                        let byte_opt = bytes[pos] == b'c';
                        let value = if pos + 1 < bytes.len() {
                            token[pos + 1..].to_vec()
                        } else {
                            index += 1;
                            if index >= scanned.len() {
                                return Err(HeadArgError::Getopt(
                                    crate::GetoptReport::MissingValue(char::from(bytes[pos])),
                                ));
                            }
                            scanned[index].clone()
                        };
                        // 数值腿仍交给 `str::parse::<i64>`：能转成 i64 的文本必是 ASCII，
                        // 所以 `from_utf8` 失败与「这不是一个数」是同一件事——把解码当**纯检查**
                        // 用，非 UTF-8 的值走 C 的那句 `illegal ... count` 并被**原字节回显**。
                        match core::str::from_utf8(&value)
                            .ok()
                            .and_then(|text| text.parse::<i64>().ok())
                        {
                            Some(parsed) if parsed > 0 => {
                                if byte_opt {
                                    result.bytecnt = parsed as usize;
                                } else {
                                    result.count = parsed as usize;
                                }
                            }
                            _ => {
                                return Err(HeadArgError::IllegalCount {
                                    byte: byte_opt,
                                    value,
                                });
                            }
                        }
                        pos = bytes.len(); // the value consumed the token's tail
                    }
                    b'q' => {
                        result.qflag = true;
                        result.vflag = false;
                        pos += 1;
                    }
                    b'v' => {
                        result.qflag = false;
                        result.vflag = true;
                        pos += 1;
                    }
                    _ => {
                        return Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                            char::from(bytes[pos]),
                        )));
                    }
                }
            }
        } else {
            // C 的 `getopt` 在这里交回 -1 且不推进 `optind`（`getopt.c:71-74`），
            // 于是 `head.c:118-119` 的 `argv += optind` 把这个 token **和它后面的
            // 全部**当成操作数：后面的 `-q` 不再置旗标、`-x` 不再走未知字母那一句、
            // `-n 3` 不再改计数、`--` 也不再是「选项结束」而是一个叫 `--` 的档名。
            result.operands.extend_from_slice(&scanned[index..]);
            break;
        }
        index += 1;
    }
    Ok(result)
}

/// Parsed `tail` options and file operands — the deciding half's front door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailArgs {
    /// C's `(style, off)` plus `rflag`, already folded (`tail.c:135-164`), so
    /// the execution half only has to pick a printing shape.
    pub style: TailStyle,
    /// C's `fflag` (`extern.h:51`, set by `-f`/`-F` at `tail.c:110-121`).
    pub follow: Follow,
    /// File operands in order; `-` is a literal file name (`tail` opens operands
    /// through `fopen`, unlike `cut`).
    pub operands: Vec<String>,
}

/// `tail` 的跟随三态，对位 C 的整型全局 `fflag`（`extern.h:51`）：`-f` 置 1、`-F` 置 2、
/// 不给是 0（`tail.c:110-121`）。三个值都被真源用在不同的判据上，所以不能压成一个布尔：
/// 1 只读新字节、2 还按名字认换档（`forward.c:255`），0 则连睡都不睡（`:201-202`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow {
    /// 没有跟随选项：打印完就退出。
    Off,
    /// `-f`（`fflag == 1`）：跟着**已打开的那个文件**读新字节，永不按名字重开。
    ByHandle,
    /// `-F`（`fflag == 2`）：除读新字节外，每拍按操作数名字查身份，认出的换档就重开。
    ByName,
}

impl Follow {
    /// 是否由 `fflag` 认出来的跟随（C 的 `if (fflag)`、`if (fflag && argc > 1)`、
    /// `if (rflag) { if (fflag) usage(); }` 三处都只问这一件事）。
    #[must_use]
    pub fn active(self) -> bool {
        self != Follow::Off
    }

    /// 这一拍要不要按名字查身份（C `forward.c:255` 的 `fflag == 2 && fp != stdin`）。
    /// 两个条件各挡一支：`-f` 不比身份，标准输入没有名字可查（`tail.c:186` 把
    /// `fname` 写成字面量 `"stdin"`，拿它去 `stat` 只会得到一个虚构的路径）。
    #[must_use]
    pub fn probes_identity(self, is_stdin: bool) -> bool {
        self == Follow::ByName && !is_stdin
    }

    /// C `tail.c:197-201`：无操作数时先探一次标准输入能不能定位，`lseek` 报
    /// `ESPIPE`（管道）就把 `fflag` 清零，注释写着「POSIX.2 requires this」。
    /// 所以 `cat x | tail -f` 打印完就退出，不是永久等待。
    #[must_use]
    pub fn kept_for_stdin(self, stdin_is_pipe: bool) -> Follow {
        if stdin_is_pipe { Follow::Off } else { self }
    }
}

/// How `tail` selects what to print — C's `enum STYLE` (`extern.h:38`) carried
/// with its `off`, folded the way C's `main()` folds it before dispatching to
/// `forward()` or `reverse()` (`tail.c:135-164`). Keeping mode and count in one
/// variant is what lets the `+N` forms express `RBYTES`/`RLINES` with `off == 0`
/// (which C prints as *nothing*, `reverse.c:79-80`) instead of as "no option".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailStyle {
    /// `forward()` + `RLINES`: the last `off` lines in order (default `off = 10`).
    LastLines { off: usize },
    /// `forward()` + `RBYTES`: the trailing `off` bytes (`-b` folded to 512×N).
    LastBytes { off: usize },
    /// `forward()` + `FLINES`: drop `off` newlines, print everything after them.
    SkipLines { off: usize },
    /// `forward()` + `FBYTES`: drop the first `off` bytes, print the remainder.
    SkipBytes { off: usize },
    /// `reverse()` + `RLINES`: the last `off` lines bottom-up.
    ReverseLines { off: usize },
    /// `reverse()` + `RBYTES`: the lines of the trailing `off`-byte window,
    /// bottom-up.
    ReverseBytes { off: usize },
    /// `reverse()` + `REVERSE`: every line bottom-up (`-r` with no number).
    ReverseAll,
}

impl TailStyle {
    /// Whether this selection prints nothing at all. C has two early exits that
    /// both come down to "a backward count of zero selects nothing":
    /// `forward.c`'s `RLINES`/`RBYTES` arms seek to the end of the file when
    /// `off` is zero (and for a pipe just drain it), and `reverse.c` returns
    /// before touching the input when `style != REVERSE && off == 0`. So
    /// `tail -n 0`, `tail -c 0`, `tail -b 0` and the folded `tail -r -n +1` all
    /// print nothing while still exiting successfully — and none of them is the
    /// `REVERSE` shape, which prints the whole file bottom-up.
    pub fn selects_nothing(self) -> bool {
        matches!(
            self,
            TailStyle::LastLines { off: 0 }
                | TailStyle::LastBytes { off: 0 }
                | TailStyle::ReverseLines { off: 0 }
                | TailStyle::ReverseBytes { off: 0 }
        )
    }
}

/// `tail` argument-parsing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailError {
    /// 不带 getopt 前置消息的用法错：C 的 `ARG` 宏首句 `if (style) usage()`
    /// （`tail.c:86-87`，两个数字选项无论重复还是混用）与 `-r` 配跟随（`:143-145`）
    /// 都是直接走 usage，前面不会多一行 getopt 自己的报文。
    Usage,
    /// 集外字母与带值选项缺值：那一行由 libc 的 getopt 自己印，随后才是本命令的
    /// usage，所以真源是**两行**。句子住在 [`GetoptReport`] 里（同族的头三个命令共用）。
    Getopt(crate::GetoptReport),
    /// `-f`/`-F` 配了两个以上操作数：C 在解析完选项、打开任何文件之前就致命退出
    /// （`tail.c:135-137` 的 `xerrx(1, "-f and -F options only appropriate for a
    /// single file")`）。这句话没有 `errno` 后缀（`xerrx` 走 `vwarnx`，
    /// `misc.c:78-89`），且排在 `-r` 的用法检查之前。
    FollowMultiFile,
    /// A `-b`/`-c`/`-n` count that is not a decimal integer (C `ARG`'s `*p` test
    /// on the leftover of `strtoll`, `tail.c:89-90`); carries the text. A zero
    /// count is *not* this error: C accepts it and selects nothing (see
    /// [`TailStyle::selects_nothing`]).
    IllegalCount(String),
    /// An old-style `-<digit>…` token whose trailing unit letter is none of
    /// `b`/`c`/`l`/digit — C `obsolete()`'s `default:` fatal arm
    /// (`xerrx(1, "illegal option -- %s", *argv)`, `tail.c`); carries the token.
    IllegalOption(String),
}

/// One token's outcome in C's `obsolete()` pre-pass.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ObsoleteStep {
    /// The token is an old-style option and was rewritten for `getopt`.
    Rewritten(String),
    /// A recognised letter option (`f`, `r`, or `b`/`c`/`n` with its value
    /// attached): kept verbatim, and the scan continues.
    Verbatim,
    /// `b`/`c`/`n` written as its own token: the *next* token is its value and is
    /// not scanned at all (C's `if (!ap[1]) ++argv`).
    SkipsValue,
    /// C `return`s here: this token and everything after it stay verbatim.
    StopScan,
    /// C's `default:` inside the rewrite — fatal, `xerrx(1, "illegal option")`.
    IllegalOption,
}

/// Classify and rewrite one argv token the way C's `obsolete()` walks `argv`
/// (`tail.c`). A token starting with `-` or `+` whose second char is a digit is
/// rewritten: a trailing `f`/`r` is lifted to the front as its own letter; the
/// (possibly new) last char picks the unit letter — `b` and `c` stay, `l` and a
/// digit become `n` — and is dropped from the digits; the token's own first char
/// is then re-inserted before the digits, so the number arrives as the option's
/// value *with its sign*: `-3c` becomes `-c-3`, which `ARG` reads back as three
/// bytes because its `case '-'` negates the parsed number. `tail +10` is the same
/// machine with a `+` first char: it rewrites to `-n+10`, i.e. the forward form
/// starting at line 10.
///
/// The walk is what the two `Verbatim`-shaped steps model: C does **not** stop at
/// the first ordinary option, it keeps scanning past `f`/`r` and past the letter
/// of a value-taking option (skipping only that option's value). So `tail -r -3c`
/// rewrites `-3c`, while `tail f -3c` does not — an operand ends the pre-pass.
fn obsolete_step(arg: &str) -> ObsoleteStep {
    let bytes = arg.as_bytes();
    // C: `ap[0] != '-' && ap[0] != '+'` returns, and so does `ap[1] == '-'`
    // (`--`). A lone `-`/`+` reads the terminator and falls to `default:`.
    if bytes.first() != Some(&b'-') && bytes.first() != Some(&b'+') {
        return ObsoleteStep::StopScan;
    }
    let Some(second) = bytes.get(1).copied() else {
        return ObsoleteStep::StopScan;
    };
    if second == b'-' {
        return ObsoleteStep::StopScan;
    }
    if second.is_ascii_digit() {
        let mut body = &arg[1..];
        let mut trail = "";
        match body.as_bytes().last() {
            Some(&b'f') | Some(&b'r') => {
                trail = &body[body.len() - 1..];
                body = &body[..body.len() - 1];
            }
            _ => {}
        }
        let unit = match body.as_bytes().last() {
            Some(b'b') => {
                body = &body[..body.len() - 1];
                'b'
            }
            Some(b'c') => {
                body = &body[..body.len() - 1];
                'c'
            }
            Some(b'l') => {
                body = &body[..body.len() - 1];
                'n'
            }
            Some(c) if c.is_ascii_digit() => 'n',
            // C `default: xerrx(1, "illegal option -- %s", *argv)` — fatal.
            Some(_) | None => return ObsoleteStep::IllegalOption,
        };
        return ObsoleteStep::Rewritten(format!("-{trail}{unit}{}{body}", &arg[0..1]));
    }
    match second {
        b'b' | b'c' | b'n' => {
            if bytes.len() == 2 {
                ObsoleteStep::SkipsValue
            } else {
                ObsoleteStep::Verbatim
            }
        }
        b'f' | b'r' => ObsoleteStep::Verbatim,
        // Any other letter: C returns and lets `getopt` reject it (`case '?'` →
        // usage), so the pre-pass stops without raising its own error.
        _ => ObsoleteStep::StopScan,
    }
}

/// Split `tail`'s argv tail into [`TailArgs`], faithful to C's `obsolete(argv)`
/// pre-pass (`tail.c:106`) plus `getopt(argc, argv, "Fb:c:fn:r")` (`tail.c:108`):
/// old-style `-<digit>…` / `+<digit>…` tokens are rewritten into getopt-shaped
/// options (see [`obsolete_arg`]); option letters cluster (`-rn` ≡ `-r -n`); a
/// colon option's value attaches to its token (`-n10`, `-c100`) or follows it
/// (`-n 10`); `--` ends option scanning; a lone `-` is an operand. `-f`/`-F` are
/// reported, never silently leaked as an operand (as the old whole-token matcher
/// did with `-n10`). Like `head`, `getopt` here **does not permute** (see
/// [`parse_head_args`]): the first non-option token ends option scanning and
/// every token from there on is an operand, so `tail f -r` reads two files and
/// never reverses (`tail_ref f -r` 现量 `tail: -r: No such file or directory`).
///
/// A `+N` value takes the `forward` arm of C's `ARG` macro (`tail.c:91-96`),
/// which is a *different* style, not a negated count: `tail -n +20` starts at
/// line 20, and because the macro's number options are 1-based it decrements
/// `off` by one unit first, so `+1` skips nothing and prints the whole file. A
/// `-N` value takes the other arm: `ARG` negates the parsed number and keeps the
/// backward style, so `-c -3` means the same as `-c 3`.
pub fn parse_tail_args(args: &[&str]) -> Result<TailArgs, TailError> {
    // Phase 1 — obsolete(): walk the tokens the way C does. The walk outlives
    // ordinary options and skips only a value-taking option's value; it stops at
    // the first token C returns on, and everything from there on is handed to
    // `getopt` unchanged.
    let mut scanned: Vec<String> = Vec::new();
    let mut rest = args;
    let mut frozen = false;
    while let Some((head, tail)) = rest.split_first() {
        rest = tail;
        if frozen {
            scanned.push(String::from(*head));
            continue;
        }
        match obsolete_step(head) {
            ObsoleteStep::Rewritten(token) => scanned.push(token),
            ObsoleteStep::Verbatim => scanned.push(String::from(*head)),
            ObsoleteStep::SkipsValue => {
                scanned.push(String::from(*head));
                // C `if (!ap[1]) ++argv`: the value word is consumed by the
                // option, so it is copied through without being scanned.
                if let Some((value, tail2)) = rest.split_first() {
                    scanned.push(String::from(*value));
                    rest = tail2;
                }
            }
            ObsoleteStep::StopScan => {
                frozen = true;
                scanned.push(String::from(*head));
            }
            ObsoleteStep::IllegalOption => {
                return Err(TailError::IllegalOption(String::from(*head)));
            }
        }
    }

    // Phase 2 — getopt("Fb:c:fn:r"). C keeps `style` and `off` as two locals and
    // only fills them in when a number option appears (`NOTSET` until then,
    // `tail.c:107`); the default and the `-r` folding happen after the loop.
    let mut seen: Option<(RawStyle, usize)> = None;
    let mut rflag = false;
    // C 的 `fflag` 是 0/1/2 三值而不是布尔（`-f` 置 1、`-F` 置 2，`tail.c:110-121`），
    // 后到者覆盖前者：`tail -f -F` 在 C 就是 `fflag = 2`，所以这里也是赋值。
    let mut follow = Follow::Off;
    let mut operands: Vec<String> = Vec::new();
    let mut index = 0;
    while index < scanned.len() {
        let token = scanned[index].as_str();
        if token == "--" {
            operands.extend_from_slice(&scanned[index + 1..]);
            break;
        }
        let bytes = token.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'f' => {
                        follow = Follow::ByHandle;
                        pos += 1;
                    }
                    b'F' => {
                        follow = Follow::ByName;
                        pos += 1;
                    }
                    b'r' => {
                        rflag = true;
                        pos += 1;
                    }
                    b'n' | b'c' | b'b' => {
                        let letter = bytes[pos];
                        // C `ARG`'s first statement: `if (style) usage()` — a
                        // second number option of any kind is a usage error, not
                        // a silent override (`tail.c:86-87`).
                        if seen.is_some() {
                            return Err(TailError::Usage);
                        }
                        // Value attaches to this token (`-n10`) or is the next
                        // word (`-n 10`).
                        let value = if pos + 1 < bytes.len() {
                            String::from(&token[pos + 1..])
                        } else {
                            index += 1;
                            if index >= scanned.len() {
                                return Err(TailError::Getopt(crate::GetoptReport::MissingValue(
                                    letter as char,
                                )));
                            }
                            scanned[index].clone()
                        };
                        // C `ARG(units, forward, backward)` (`tail.c:85-104`):
                        // `off = strtoll(optarg, &p, 10) * units`, a leading `+`
                        // selects the forward style and gives back one unit
                        // (only when `off` is non-zero), anything else is the
                        // backward style. `-b` is `ARG(512, FBYTES, RBYTES)`, so
                        // it shares `-c`'s two styles at 512-byte units.
                        let forward = value.starts_with('+');
                        // C `ARG`'s `case '-'`: `off = -off`, which lands on the
                        // same backward style as an unsigned value, so only the
                        // sign prefix is stripped here.
                        let backward_negated = !forward && value.starts_with('-');
                        let digits = &value[if forward || backward_negated { 1 } else { 0 }..];
                        let (units, fwd, bwd) = match letter {
                            b'n' => (1usize, RawStyle::SkipLines, RawStyle::LastLines),
                            b'c' => (1, RawStyle::SkipBytes, RawStyle::LastBytes),
                            _ /* b */ => (512, RawStyle::SkipBytes, RawStyle::LastBytes),
                        };
                        let parsed = digits
                            .parse::<usize>()
                            .map_err(|_| TailError::IllegalCount(value.clone()))?;
                        // C `ARG` does not reject zero: `off = 0 * units` stays
                        // zero and the style is set anyway, which is the
                        // "selects nothing" shape of [`TailStyle::selects_nothing`].
                        let mut off = parsed.saturating_mul(units);
                        if forward && off != 0 {
                            off -= units;
                        }
                        seen = Some((if forward { fwd } else { bwd }, off));
                        pos = bytes.len(); // the value consumed the token's tail
                    }
                    _ => {
                        return Err(TailError::Getopt(crate::GetoptReport::UnknownOption(
                            bytes[pos] as char,
                        )));
                    }
                }
            }
        } else {
            // 与 `head` 同一条判据：`getopt` 在第一个非选项 token 上收 -1 且不推进
            // `optind`（`getopt.c:71-74`），`tail.c:110` 的 `argv += optind` 于是把
            // 这个 token 与其后的全部当操作数——参照件现量 `tail a.txt -r` 只出
            // 「`tail: -r: No such file or directory`」而**不**反序。
            operands.extend_from_slice(&scanned[index..]);
            break;
        }
        index += 1;
    }

    // C 的两道门按 `tail.c` 里的先后排：单文件门在 `:135-137`（紧接 `argc -= optind`），
    // 反序与跟随互斥在 `:143-145`。因此 `tail -r -f a b` 报的是单文件那句，
    // 而不是 usage——顺序反了会把一条已经到口的诊断吞掉。
    if follow.active() && operands.len() > 1 {
        return Err(TailError::FollowMultiFile);
    }
    if rflag && follow.active() {
        return Err(TailError::Usage);
    }

    // C `tail.c:135-164`: when no number option was given the default is the
    // whole file for `-r` (`REVERSE`), else the last ten lines (`RLINES`).
    // Under `-r` a forward style becomes the same-unit backward one with `off`
    // kept, which is how `tail -r -n +5` ends up the last four lines bottom-up.
    let style = match (rflag, seen) {
        (false, None) => TailStyle::LastLines { off: 10 },
        (true, None) => TailStyle::ReverseAll,
        (false, Some((RawStyle::LastLines, off))) => TailStyle::LastLines { off },
        (false, Some((RawStyle::LastBytes, off))) => TailStyle::LastBytes { off },
        (false, Some((RawStyle::SkipLines, off))) => TailStyle::SkipLines { off },
        (false, Some((RawStyle::SkipBytes, off))) => TailStyle::SkipBytes { off },
        (true, Some((kind, off))) => match kind {
            RawStyle::LastLines | RawStyle::SkipLines => TailStyle::ReverseLines { off },
            RawStyle::LastBytes | RawStyle::SkipBytes => TailStyle::ReverseBytes { off },
        },
    };
    Ok(TailArgs {
        style,
        follow,
        operands,
    })
}

/// The `NOTSET`-resolved pair C holds while parsing: a style without its count,
/// so the post-loop fold can tell "no number option" from "`off == 0`".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawStyle {
    LastLines,
    LastBytes,
    SkipLines,
    SkipBytes,
}

/// `tail -f`/`-F` 每拍决策要比的身份（接线轮见 18 篇台账批次二百八十那笔）。
///
/// 对位 C `forward.c` 的 `USE_SLEEP` 支（`:248-272`）——kqueue 那半在 Minix 上被
/// `#if !defined(__minix)` 排除（`:207-246`），所以真源的跟随就是「睡一拍 + 按名字
/// 查身份」。字段就是 C 比的那四个：`st_size` 不在判据里（真源从不比它，尺寸的变化
/// 由句柄的读位置自己反映），时间戳也不在——所以这两个都不进这个记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatIdentity {
    /// `struct stat::st_ino`：C 用它认出文件被换过。
    pub ino: u64,
    /// `st_dev`。
    pub dev: u64,
    /// `st_rdev`（C 一并比，设备文件换指向时也算换档）。
    pub rdev: u64,
    /// `st_nlink`：为 0 表示最后一个链接已删。
    pub nlink: u64,
}

/// 手上这个文件与名字现在指向的是不是同一个（C `forward.c:257-260` 的四个比较：
/// `st_ino`/`st_dev`/`st_rdev` 任一变了，或 `st_nlink == 0`）。
///
/// 这里**不再问 `fflag`**：「只有 `-F` 才查」是 `:255` 那道门的活，已经由
/// [`Follow::probes_identity`] 承载。两处各重复一份同一个事实的话，抹平任何一处
/// 都不会改变行为，变异负控也就咬不住（本轮就是这么发现的：先写成两处重复，
/// 直到一枚负控「改坏了却全绿」）。
/// 重开后调用方把新的身份接过来继续比（C 的 `*sbp = statbuf`，`:266`），并把
/// 读位置交回新档的开头（C 的 `freopen`，`:261`）。
#[must_use]
pub fn reopen_needed(opened: &StatIdentity, current: &StatIdentity) -> bool {
    current.ino != opened.ino
        || current.dev != opened.dev
        || current.rdev != opened.rdev
        || current.nlink == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed<'a, W: LineWindow<'a>>(window: &mut W, lines: &[&'a str]) -> [Line<'a>; 8] {
        for line in lines {
            window.push(Line::from(*line));
        }
        let mut out: [Line<'a>; 8] = [Line::from(""); 8];
        let count = window.drain(&mut out);
        assert!(count <= 8);
        out
    }

    /// 比内容时只看文本——终止状态由另两枚门专门钉，不在这里重复承载。行现在是字节，
    /// 而本文件的夹具全是 ASCII，所以这里直接 expect：真出现非 ASCII 就是夹具写错了。
    fn texts<'a>(lines: &[Line<'a>]) -> Vec<&'a str> {
        lines
            .iter()
            .map(|line| core::str::from_utf8(line.bytes()).expect("测试夹具都是 ASCII"))
            .collect()
    }

    /// 换档比较的四个字段与「不问 fflag」的分工（对位 `forward.c:255` 与 `:257-260`）。
    #[test]
    fn follow_reopen_needs_a_changed_identity() {
        let opened = StatIdentity {
            ino: 7,
            dev: 1,
            rdev: 0,
            nlink: 1,
        };
        // 认三种形：ino 变、dev 变、最后一个链接被删（nlink == 0）
        let rotated = StatIdentity { ino: 8, ..opened };
        assert!(reopen_needed(&opened, &rotated));
        assert!(reopen_needed(&opened, &StatIdentity { dev: 2, ..opened }));
        assert!(reopen_needed(&opened, &StatIdentity { nlink: 0, ..opened }));
        // `st_rdev` 也算：设备文件换了指向就是换了档
        assert!(reopen_needed(&opened, &StatIdentity { rdev: 5, ..opened }));
        // 四个字段全同就不重开（尺寸不进这个记录，因此也不参与比较）
        assert!(!reopen_needed(&opened, &opened));
    }

    /// 探身份的那道门与标准输入的两支例外（`forward.c:255` 与 `tail.c:197-201`）。
    #[test]
    fn follow_probes_only_a_named_file_and_a_pipe_stdin_gives_it_up() {
        // 「谁去查」全部住在这里：`-F` 只对真文件查，`-f` 不查，标准输入也不查
        // （它没有名字可查）。比较那一步不再重复这个判断。
        assert!(Follow::ByName.probes_identity(false));
        assert!(!Follow::ByName.probes_identity(true));
        assert!(!Follow::ByHandle.probes_identity(false));
        assert!(!Follow::Off.probes_identity(false));
        // 标准输入是管道时，C 把 fflag 清零（POSIX.2），连 `-F` 也算放弃
        assert_eq!(Follow::ByName.kept_for_stdin(true), Follow::Off);
        assert_eq!(Follow::ByHandle.kept_for_stdin(true), Follow::Off);
        assert_eq!(Follow::ByName.kept_for_stdin(false), Follow::ByName);
        assert_eq!(Follow::Off.kept_for_stdin(true), Follow::Off);
    }

    #[test]
    fn test_head_keeps_first() {
        let mut window = HeadWindow::new(2).unwrap();
        let out = feed(&mut window, &["a", "b", "c"]);
        assert_eq!(texts(&out[..2]), ["a", "b"]);
        assert_eq!(window.seen(), 3);
    }

    #[test]
    fn test_tail_keeps_last() {
        let mut window = TailWindow::new(2).unwrap();
        let out = feed(&mut window, &["a", "b", "c", "d"]);
        assert_eq!(texts(&out[..2]), ["c", "d"]);
        assert_eq!(window.seen(), 4);
    }

    #[test]
    fn test_tail_short_input() {
        let mut window = TailWindow::new(5).unwrap();
        let out = feed(&mut window, &["a", "b"]);
        assert_eq!(texts(&out[..2]), ["a", "b"]);
    }

    #[test]
    fn test_zero_count_is_refused_and_large_counts_are_not() {
        assert_eq!(
            HeadWindow::new(0).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
        assert_eq!(
            TailWindow::new(0).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
        // No ceiling: C `forward.c` + `RLINES` walks back through the mapped
        // file for any count, so a count above the old 32 must survive.
        assert!(TailWindow::new(33).is_ok());
        assert!(HeadWindow::new(33).is_ok());
        let mut tail = TailWindow::new(1000).unwrap();
        for line in ["a", "b"] {
            tail.push(Line::from(line));
        }
        // A count far above the input yields the whole input, in order.
        let mut out: [Line; 2] = [Line::from(""); 2];
        assert_eq!(tail.drain(&mut out), 2);
        assert_eq!(texts(&out), ["a", "b"]);
    }

    #[test]
    fn test_windows_share_the_trait() {
        let mut head = HeadWindow::new(1).unwrap();
        let mut tail = TailWindow::new(1).unwrap();
        {
            let windows: [&mut dyn LineWindow<'_>; 2] = [&mut head, &mut tail];
            for window in windows {
                window.push(Line::from("x"));
                window.push(Line::from("y"));
            }
        }
        let mut out = [Line::from(""); 2];
        assert_eq!(head.drain(&mut out), 1);
        assert_eq!(out[0].bytes(), b"x");
        assert_eq!(tail.drain(&mut out), 1);
        assert_eq!(out[0].bytes(), b"y");
    }

    /// 夹具仍写 `"…"` 字面量以便读，这一层把它们统一交回 `&[u8]` —— 被测的
    /// `parse_head_args` 收的是 argv 的原字节切片。
    fn argv<'a>(words: &'a [&'a str]) -> Vec<&'a [u8]> {
        words.iter().map(|w| w.as_bytes()).collect()
    }

    /// `tail` 的操数期望仍是文本形（`TailArgs.operands: Vec<String>` 未在本轮射程内）。
    fn operands(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| String::from(*w)).collect()
    }

    /// head 的操数期望：`HeadArgs.operands` 是 argv 的原字节，所以这里逐条转字节。
    fn operands_bytes(words: &[&str]) -> Vec<Vec<u8>> {
        words.iter().map(|w| w.as_bytes().to_vec()).collect()
    }

    #[test]
    fn test_parse_head_args_faithful_getopt() {
        // Defaults: no args → ten lines, no operands.
        assert_eq!(
            parse_head_args(&[] as &[&[u8]]),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: Vec::new(),
            })
        );
        // `-n` value as the next token or attached.
        assert_eq!(
            parse_head_args(&argv(&["-n", "5", "f"])),
            Ok(HeadArgs {
                count: 5,
                operands: operands_bytes(&["f"]),
                ..HeadArgs {
                    count: 10,
                    bytecnt: 0,
                    qflag: false,
                    vflag: false,
                    operands: Vec::new(),
                }
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["-n5", "f"])),
            Ok(HeadArgs {
                count: 5,
                operands: operands_bytes(&["f"]),
                ..HeadArgs {
                    count: 10,
                    bytecnt: 0,
                    qflag: false,
                    vflag: false,
                    operands: Vec::new(),
                }
            })
        );
        // obsolete(): the leading `-<digit>` run rewrites to `-n<digit>`.
        assert_eq!(
            parse_head_args(&argv(&["-5", "f"])).map(|a| (a.count, a.operands)),
            Ok((5, operands_bytes(&["f"])))
        );
        // `-c` byte count (attached or separate).
        assert_eq!(
            parse_head_args(&argv(&["-c", "10"])).map(|a| a.bytecnt),
            Ok(10)
        );
        // clustered `-q`/`-v`, last one wins.
        assert_eq!(
            parse_head_args(&argv(&["-qv", "f"])).map(|a| (a.qflag, a.vflag)),
            Ok((false, true))
        );
        assert_eq!(
            parse_head_args(&argv(&["-vq"])).map(|a| (a.qflag, a.vflag)),
            Ok((true, false))
        );
        // `--` ends options; a lone `-` is a file operand.
        assert_eq!(
            parse_head_args(&argv(&["--", "-5", "f"])).map(|a| a.operands),
            Ok(operands_bytes(&["-5", "f"]))
        );
        assert_eq!(
            parse_head_args(&argv(&["-"])).map(|a| a.operands),
            Ok(operands_bytes(&["-"]))
        );
    }

    /// `tail` 的环同样交回短横后的**第一个字节**（`bytes[pos] as char`，`getopt.c:79` 的
    /// `optopt = *place++` 一次进一个字节）。选项串是 `"Fb:c:fn:r"`（`tail.c:108`），
    /// `b`/`c`/`n` 三个都带值，所以反面的 `-né` 里 `é` 是**值**、走
    /// `tail: illegal offset -- é`（参照件 `tmp/tail-416/tail_ref` 现量）。这一枚钉的是
    /// **已经是那样**的取法；逐字节门在 `tests/tail_face.rs`。
    #[test]
    fn the_tail_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_tail_args(&["-\u{e9}"]).err(),
            Some(TailError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00c3}'
            ))),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // `r` 在选项串里合法且不带值，聚簇继续吃到第一字节。
        assert_eq!(
            parse_tail_args(&["-r\u{e9}"]).err(),
            Some(TailError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00c3}'
            ))),
            "-r\u{e9}"
        );
        // 环停在第一个坏字母上。
        assert_eq!(
            parse_tail_args(&["-Z\u{e9}"]).err(),
            Some(TailError::Getopt(crate::GetoptReport::UnknownOption('Z'))),
            "-Z\u{e9}"
        );
        // 反面：`n` 是带值字母，其后的字节整个被当值收走，轮不到字母位。
        assert_eq!(
            parse_tail_args(&["-n\u{e9}"]).err(),
            Some(TailError::IllegalCount("\u{e9}".to_string())),
            "-n\u{e9} 走的是 `illegal offset` 那一句"
        );
    }

    /// 集外字母交回的是短横后的**第一个字节**而不是第一个字符：`-\u{e9}`（`c3 a9`）在
    /// `char` 域里是 U+00C3。环那一侧本来就是 `bytes[pos] as char`（`getopt.c:79` 的
    /// `optopt = *place++` 一次进一个字节），所以这一枚钉的是**已经是那样**的取法，
    /// 免得将来有人把它改回按字符取时只有面门红、说不清坏的是哪一半；参照件
    /// `tmp/head-415/head_ref` 现量 `-é` 报单个 `303`，逐字节门在 `tests/head_face.rs`。
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_head_args(&argv(&["-\u{e9}"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00c3}'
            ))),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // `q` 是选项串里合法且不带值的字母（`head.c:87` 的 `"c:n:qv"`），聚簇继续吃到第一字节。
        assert_eq!(
            parse_head_args(&argv(&["-q\u{e9}"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00c3}'
            ))),
            "-q\u{e9}"
        );
        // 环停在第一个坏字母上。
        assert_eq!(
            parse_head_args(&argv(&["-Z\u{e9}"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                'Z'
            ))),
            "-Z\u{e9}"
        );
        // 反面：带值字母把其后的字节整个当值收走，轮不到字母位（参照件同句
        // `head: illegal byte count -- é`，单独一行、不接 usage）。
        assert_eq!(
            parse_head_args(&argv(&["-c\u{e9}"])),
            Err(HeadArgError::IllegalCount {
                byte: true,
                value: "é".as_bytes().to_vec()
            })
        );
    }

    /// `-c`/`-n` 的值在 C 里是**被回显**的（`head.c:96-97`、`:102-103` 的
    /// `errx(1, "illegal byte count -- %s", optarg)`），所以交回的必须是 argv 原字节；
    /// 操数同样按字节交回（横幅那一腿由壳自己拼，不经 `&str`）。旧签名 `&[&str]` 让下面
    /// 前两形在库里根本写不出来——一枚裸 `ff` 在走到环之前就已经不是它自己了。
    /// 参照件 `tmp/head-415/head` 现量三形：`-c<ff>` → `head: illegal byte count -- <ff>`、
    /// `-c 5<ff>` → 同句带尾巴那枚、obsolete 形 `-5<ff>` → `illegal line count -- 5<ff>`
    /// （`obsolete()` 拼的是 `-n` 加原串去掉短横后的**全部**字节，`head.c:188-192`）。
    #[test]
    fn an_illegal_count_value_and_the_operands_carry_the_argv_bytes() {
        let bad: &[&[u8]] = &[b"-c\xff"];
        assert_eq!(
            parse_head_args(bad).err(),
            Some(HeadArgError::IllegalCount {
                byte: true,
                value: vec![0xff],
            }),
            "`-c<ff>` 的值交回的是那一枚裸字节"
        );
        let tail: &[&[u8]] = &[b"-c", b"5\xff"];
        assert_eq!(
            parse_head_args(tail).err(),
            Some(HeadArgError::IllegalCount {
                byte: true,
                value: vec![b'5', 0xff],
            }),
            "尾巴带裸字节也整串交回（C 的 `%s` 打的是 optarg 全体）"
        );
        let obsolete: &[&[u8]] = &[b"-5\xff", b"f"];
        assert_eq!(
            parse_head_args(obsolete).err(),
            Some(HeadArgError::IllegalCount {
                byte: false,
                value: vec![b'5', 0xff],
            }),
            "obsolete 重写之后，值仍是那一串原字节"
        );
        let names: &[&[u8]] = &[b"a\xffb", b"c"];
        assert_eq!(
            parse_head_args(names).map(|a| a.operands),
            Ok(vec![b"a\xffb".to_vec(), b"c".to_vec()]),
            "操数按原字节交回"
        );
    }

    #[test]
    fn test_parse_head_args_errors() {
        // The regression: an unknown option no longer leaks as a file operand.
        // 两支都是「libc 先印一行，再轮到 usage」，所以带的是那个字母。
        assert_eq!(
            parse_head_args(&argv(&["-z", "f"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                'z'
            )))
        );
        // obsolete() 只折前导那一串，所以 `f -5` 不写成 `-n5`；而 `getopt` 不重排，
        // 第一个操作数一到就叫环结束（`getopt.c:71-74`），`-5` 于是**连着**成为操作数
        // ——参照件 `head_ref a.txt -5` 现量：`==> a.txt <==` 十行，再 `head: -5: No
        // such file or directory` 退 1，一个字母都不报。
        assert_eq!(
            parse_head_args(&argv(&["f", "-5"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-5".as_bytes().to_vec()],
            })
        );
        // 操作数之后的旗标也不再生效：`-q` 在这里是一个叫 `-q` 的档名（参照件同形，
        // 报 `head: -q: No such file or directory`），集外字母同理（不再有那句
        // `unknown option`），带值选项连它的值一起落进操作数。
        assert_eq!(
            parse_head_args(&argv(&["f", "-q"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-q".as_bytes().to_vec()],
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["f", "-x"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-x".as_bytes().to_vec()],
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["f", "-n", "3"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec![
                    "f".as_bytes().to_vec(),
                    "-n".as_bytes().to_vec(),
                    "3".as_bytes().to_vec()
                ],
            })
        );
        // 选项在前仍照常生效，只是环一撞上操作数就再也回不来（参照件
        // `head_ref -n 3 a.txt -5`：三行 + `-5` 那一句）。
        assert_eq!(
            parse_head_args(&argv(&["-n", "3", "f", "-5"])),
            Ok(HeadArgs {
                count: 3,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-5".as_bytes().to_vec()],
            })
        );
        // 操作数之后的 `--` 不是「选项结束」而是一个档名（C 的环根本没走到它）；
        // 单个 `-` 也在同一处收 -1，于是它自己与后面的都是操作数。
        assert_eq!(
            parse_head_args(&argv(&["f", "--", "-5"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec![
                    "f".as_bytes().to_vec(),
                    "--".as_bytes().to_vec(),
                    "-5".as_bytes().to_vec()
                ],
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["f", "-"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-".as_bytes().to_vec()],
            })
        );
        // A colon option with no value argument → the libc 缺值那一行。
        assert_eq!(
            parse_head_args(&argv(&["-n"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::MissingValue('n')))
        );
        // Illegal counts (<= 0, trailing garbage) → the byte/line reject arm.
        assert_eq!(
            parse_head_args(&argv(&["-n", "0"])),
            Err(HeadArgError::IllegalCount {
                byte: false,
                value: "0".as_bytes().to_vec()
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["-c", "x"])),
            Err(HeadArgError::IllegalCount {
                byte: true,
                value: "x".as_bytes().to_vec()
            })
        );
    }

    #[test]
    fn test_parse_tail_args_faithful_getopt() {
        use alloc::string::String;
        use alloc::vec;
        // Defaults: no args → ten lines, no operands (`tail.c:161-162`).
        assert_eq!(
            parse_tail_args(&[]),
            Ok(TailArgs {
                style: TailStyle::LastLines { off: 10 },
                follow: Follow::Off,
                operands: Vec::new(),
            })
        );
        // `getopt` 不重排：第一个操作数之后的 `-r` 不是旗标而是档名（参照件
        // `tail_ref a.txt -r` 现量 `tail: -r: No such file or directory`，输出仍按
        // 正序取后十行）。
        assert_eq!(
            parse_tail_args(&["f", "-r"]),
            Ok(TailArgs {
                style: TailStyle::LastLines { off: 10 },
                follow: Follow::Off,
                operands: vec![String::from("f"), String::from("-r")],
            })
        );
        assert_eq!(
            parse_tail_args(&["-r", "f", "-n", "3"]),
            Ok(TailArgs {
                style: TailStyle::ReverseAll,
                follow: Follow::Off,
                operands: vec![String::from("f"), String::from("-n"), String::from("3")],
            })
        );
        // `-n` attached (regression: the old matcher leaked `-n10` to operands)
        // and next-word forms both select `RLINES` with that count.
        assert_eq!(
            parse_tail_args(&["-n10", "f"]),
            Ok(TailArgs {
                style: TailStyle::LastLines { off: 10 },
                follow: Follow::Off,
                operands: operands(&["f"]),
            })
        );
        assert_eq!(
            parse_tail_args(&["-n", "10", "f"]).unwrap().style,
            TailStyle::LastLines { off: 10 }
        );
        // obsolete leading digit `-N` rewrites to `-n<N>`.
        assert_eq!(
            parse_tail_args(&["-5", "f"]).unwrap().style,
            TailStyle::LastLines { off: 5 }
        );
        // `-c`/`-b` attached values reach the byte style (`-b` counts 512-byte
        // blocks, C `ARG(512, FBYTES, RBYTES)`).
        assert_eq!(
            parse_tail_args(&["-c100", "f"]).unwrap().style,
            TailStyle::LastBytes { off: 100 }
        );
        assert_eq!(
            parse_tail_args(&["-b3", "f"]).unwrap().style,
            TailStyle::LastBytes { off: 1536 }
        );
        // clustering: `-r` then an attached `-n` is C `RLINES` under `reverse()`.
        assert_eq!(
            parse_tail_args(&["-rn3", "f"]).unwrap().style,
            TailStyle::ReverseLines { off: 3 }
        );
        // bare `-r` is C `REVERSE` (whole file, `tail.c:157-159`), and bare `-r`
        // with `-c`/`-b` is `RBYTES`.
        assert_eq!(
            parse_tail_args(&["-r", "f"]).unwrap().style,
            TailStyle::ReverseAll
        );
        assert_eq!(
            parse_tail_args(&["-rc", "8"]).unwrap().style,
            TailStyle::ReverseBytes { off: 8 }
        );
        // `--` ends scanning; a bare `-` is a literal operand.
        assert_eq!(
            parse_tail_args(&["--", "-n"]).unwrap().operands,
            vec![String::from("-n")]
        );
        assert_eq!(
            parse_tail_args(&["-"]).unwrap().operands,
            vec![String::from("-")]
        );
        // error paths
        // 集外字母与缺值都带着 libc 那一行走（句子单源在 `GetoptReport`）。
        assert_eq!(
            parse_tail_args(&["-x", "f"]).err(),
            Some(TailError::Getopt(crate::GetoptReport::UnknownOption('x')))
        );
        assert_eq!(
            parse_tail_args(&["-n"]).err(),
            Some(TailError::Getopt(crate::GetoptReport::MissingValue('n')))
        );
        assert_eq!(
            parse_tail_args(&["-c"]).err(),
            Some(TailError::Getopt(crate::GetoptReport::MissingValue('c')))
        );
        assert_eq!(
            parse_tail_args(&["-n", "x"]).err(),
            Some(TailError::IllegalCount(String::from("x")))
        );
        // C `ARG` does not reject a zero count — it selects nothing. Refusing
        // `-c 0`/`-b 0` as `illegal offset` is not C behavior.
        assert_eq!(
            parse_tail_args(&["-c", "0"]).unwrap().style,
            TailStyle::LastBytes { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&["-b", "0"]).unwrap().style,
            TailStyle::LastBytes { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&["-n", "0"]).unwrap().style,
            TailStyle::LastLines { off: 0 }
        );
    }

    #[test]
    fn test_tail_style_selects_nothing_covers_the_zero_arms() {
        // C's two early exits: `forward.c` seeks to the end for `RLINES`/`RBYTES`
        // with `off == 0`, `reverse.c` returns for any non-`REVERSE` style with
        // `off == 0`.
        assert!(TailStyle::LastLines { off: 0 }.selects_nothing());
        assert!(TailStyle::LastBytes { off: 0 }.selects_nothing());
        assert!(TailStyle::ReverseLines { off: 0 }.selects_nothing());
        assert!(TailStyle::ReverseBytes { off: 0 }.selects_nothing());
        // Bare `-r` is `REVERSE`: it prints the whole file, and a forward offset
        // of `+1` is `off == 0` in the *skip* sense — everything after byte/line
        // one, i.e. the whole file.
        assert!(!TailStyle::ReverseAll.selects_nothing());
        assert!(!TailStyle::SkipLines { off: 0 }.selects_nothing());
        assert!(!TailStyle::SkipBytes { off: 0 }.selects_nothing());
        assert!(!TailStyle::LastLines { off: 10 }.selects_nothing());
        assert!(!TailStyle::ReverseBytes { off: 8 }.selects_nothing());
    }

    #[test]
    fn test_parse_tail_args_forward_offset_is_one_based() {
        // C `ARG` (`tail.c:91-96`): a `+` value takes the forward style and,
        // when non-zero, gives back one unit — so `+1` skips nothing and `+N`
        // starts *at* line/byte N.
        assert_eq!(
            parse_tail_args(&["-n", "+1"]).unwrap().style,
            TailStyle::SkipLines { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&["-n", "+20"]).unwrap().style,
            TailStyle::SkipLines { off: 19 }
        );
        assert_eq!(
            parse_tail_args(&["-c", "+3"]).unwrap().style,
            TailStyle::SkipBytes { off: 2 }
        );
        // The attached form (`-c+3`, and the clustered `-rc+3`) goes through the
        // same `ARG` path as the next-word form.
        assert_eq!(
            parse_tail_args(&["-c+3"]).unwrap().style,
            TailStyle::SkipBytes { off: 2 }
        );
        // `-b` counts 512-byte blocks, so `+2` starts at byte 512.
        assert_eq!(
            parse_tail_args(&["-b", "+2"]).unwrap().style,
            TailStyle::SkipBytes { off: 512 }
        );
        // `+0` keeps `off` at zero (C skips the decrement when `off` is zero).
        assert_eq!(
            parse_tail_args(&["-n", "+0"]).unwrap().style,
            TailStyle::SkipLines { off: 0 }
        );
        // A count past the ring's declared cap is not the ring's business: C
        // `forward()` + `FLINES` walks the file with `getc` and needs no buffer.
        assert_eq!(
            parse_tail_args(&["-n", "+40"]).unwrap().style,
            TailStyle::SkipLines { off: 39 }
        );
    }

    #[test]
    fn test_parse_tail_args_reverse_folds_forward_offset() {
        // C `tail.c:146-149`: under `-r` `FBYTES` becomes `RBYTES` and `FLINES`
        // becomes `RLINES`, `off` unchanged — `tail -r -n +5` is the last four
        // lines bottom-up, not a forward walk.
        assert_eq!(
            parse_tail_args(&["-r", "-n", "+5"]).unwrap().style,
            TailStyle::ReverseLines { off: 4 }
        );
        assert_eq!(
            parse_tail_args(&["-r", "-c", "+9"]).unwrap().style,
            TailStyle::ReverseBytes { off: 8 }
        );
        // `-r -n +1` folds to `RLINES` with `off == 0`, which C prints as
        // nothing (`reverse.c:79-80`) — the shape must not collapse into
        // `ReverseAll`, which prints the whole file.
        assert_eq!(
            parse_tail_args(&["-r", "-n", "+1"]).unwrap().style,
            TailStyle::ReverseLines { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&["-r", "-c", "+1"]).unwrap().style,
            TailStyle::ReverseBytes { off: 0 }
        );
    }

    #[test]
    fn test_parse_tail_args_forward_offset_rejects_non_numeric() {
        // C `if (*p) xerrx(1, "illegal offset -- %s", optarg)` (`tail.c:89-90`)
        // applies to the `+` form too: a bare `+` is not a number.
        assert_eq!(
            parse_tail_args(&["-n", "+"]).err(),
            Some(TailError::IllegalCount(String::from("+")))
        );
        assert_eq!(
            parse_tail_args(&["-c", "+x"]).err(),
            Some(TailError::IllegalCount(String::from("+x")))
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_unit_letters() {
        // C `obsolete()`: a trailing unit letter selects the option and the digits
        // come back as a *negated* value, which `ARG`'s `case '-'` reads as a
        // backward count. So `-3c` is `tail -c 3`, `-3b` three 512-byte blocks,
        // `-3l` and a bare `-3` are lines.
        assert_eq!(
            parse_tail_args(&["-3c"]).unwrap().style,
            TailStyle::LastBytes { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-3b"]).unwrap().style,
            TailStyle::LastBytes { off: 1536 }
        );
        assert_eq!(
            parse_tail_args(&["-3l"]).unwrap().style,
            TailStyle::LastLines { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-3"]).unwrap().style,
            TailStyle::LastLines { off: 3 }
        );
        // A trailing `r` is lifted to the front, so `-3r` ≡ `-r -n 3` and
        // `-3cr` ≡ `-r -c 3`.
        assert_eq!(
            parse_tail_args(&["-3r"]).unwrap().style,
            TailStyle::ReverseLines { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-3cr"]).unwrap().style,
            TailStyle::ReverseBytes { off: 3 }
        );
        // The `+` first char runs through the same machine: `tail +10` ≡
        // `tail -n +10`, and `+4c` ≡ `tail -c +4`.
        assert_eq!(
            parse_tail_args(&["+10"]).unwrap().style,
            TailStyle::SkipLines { off: 9 }
        );
        assert_eq!(
            parse_tail_args(&["+4c"]).unwrap().style,
            TailStyle::SkipBytes { off: 3 }
        );
        // A trailing `f` lifts to the front of the rewritten token (`-3f` becomes
        // `-fn-3`, `tail.c:247-251`), so the old-style form sets the follow flag
        // and the count together instead of leaking away as an operand.
        let oldstyle = parse_tail_args(&["-3f"]).unwrap();
        assert_eq!(oldstyle.style, TailStyle::LastLines { off: 3 });
        assert_eq!(oldstyle.follow, Follow::ByHandle);
    }

    /// 跟随两形的取值、聚簇、覆盖与 C 的两道门（`tail.c:110-121`、`:135-137`、`:143-145`）。
    #[test]
    fn test_parse_tail_args_follow_flags_and_gates() {
        // `-f` 与 `-F` 是两个不同的值，不是同一个布尔
        assert_eq!(
            parse_tail_args(&["-f", "f"]).unwrap().follow,
            Follow::ByHandle
        );
        assert_eq!(
            parse_tail_args(&["-F", "f"]).unwrap().follow,
            Follow::ByName
        );
        // C 的 switch 对两个字母都是赋值，所以后到者覆盖前者
        assert_eq!(
            parse_tail_args(&["-f", "-F", "f"]).unwrap().follow,
            Follow::ByName
        );
        // 同一 token 里的聚簇也认（`-Ff` 是两个无值选项）
        assert_eq!(
            parse_tail_args(&["-Ff", "f"]).unwrap().follow,
            Follow::ByHandle
        );
        // 单文件门只看操作数个数，0 个（读标准输入）与 1 个都不拦
        assert_eq!(parse_tail_args(&["-f"]).unwrap().follow, Follow::ByHandle);
        assert_eq!(parse_tail_args(&["-f", "a"]).unwrap().operands.len(), 1);
        assert_eq!(
            parse_tail_args(&["-f", "a", "b"]).err(),
            Some(TailError::FollowMultiFile)
        );
        assert_eq!(
            parse_tail_args(&["-F", "a", "b"]).err(),
            Some(TailError::FollowMultiFile)
        );
        // 单文件门排在 `-r` 的用法检查之前，所以 `-r -f a b` 报的是前一句
        assert_eq!(
            parse_tail_args(&["-r", "-f", "a", "b"]).err(),
            Some(TailError::FollowMultiFile)
        );
        // `-r` 与跟随互斥，带不带计数都一样
        assert_eq!(
            parse_tail_args(&["-r", "-f", "a"]).err(),
            Some(TailError::Usage)
        );
        // 互斥只在 `rflag` 那一支里查（`tail.c:143-145`），所以只给计数与 `-F`
        // 不报错——这一条是本轮自己写错过一次期望、由门反证回来的
        let counted = parse_tail_args(&["-n", "3", "-F", "a"]).unwrap();
        assert_eq!(counted.style, TailStyle::LastLines { off: 3 });
        assert_eq!(counted.follow, Follow::ByName);
        assert_eq!(parse_tail_args(&["-rF", "a"]).err(), Some(TailError::Usage));
    }

    #[test]
    fn test_parse_tail_args_obsolete_stops_at_first_non_match() {
        // C `obsolete()` returns from the whole pre-pass at the first token it
        // does not recognise, so a later old-style token is *not* rewritten. A
        // `+<digit>` token then reaches the tail as a plain operand, and because
        // `getopt` does not permute (`getopt.c:71-74`) the operand run ends
        // option scanning there — everything after it is a file name too.
        let frozen = parse_tail_args(&["f", "+10"]).unwrap();
        assert_eq!(frozen.style, TailStyle::LastLines { off: 10 });
        assert_eq!(
            frozen.operands,
            alloc::vec![String::from("f"), String::from("+10")]
        );
        // 冻在环里的 `-3c` 也不再是「未知字母 3」：参照件 `tail_ref f -3c` 现量
        // `tail: -3c: No such file or directory` 退 1，一句选项错都不打。
        let after = parse_tail_args(&["f", "-3c"]).unwrap();
        assert_eq!(after.style, TailStyle::LastLines { off: 10 });
        assert_eq!(
            after.operands,
            alloc::vec![String::from("f"), String::from("-3c")]
        );
        // Before the freeze point the rewrite does apply, and the operand list is
        // just the file name.
        let live = parse_tail_args(&["-3c", "f"]).unwrap();
        assert_eq!(live.style, TailStyle::LastBytes { off: 3 });
        assert_eq!(live.operands, alloc::vec![String::from("f")]);
        // `--` is the other stop signal, and it still ends option scanning.
        assert_eq!(
            parse_tail_args(&["-3c", "--", "f"]).unwrap().operands,
            alloc::vec![String::from("f")]
        );
        // A lone `-` is an operand, not a rewrite — and must not index past the
        // token's end while deciding that.
        assert_eq!(
            parse_tail_args(&["-"]).unwrap().operands,
            alloc::vec![String::from("-")]
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_illegal_unit_is_fatal() {
        // C's `default:` inside the rewrite is fatal and quotes the whole token:
        // `xerrx(1, "illegal option -- %s", *argv)`.
        assert_eq!(
            parse_tail_args(&["-3x"]).err(),
            Some(TailError::IllegalOption(String::from("-3x")))
        );
        // Only one trailing letter is lifted, so a second one lands on the same
        // arm (`-3ff` → after lifting `f`, the unit char is `f`).
        assert_eq!(
            parse_tail_args(&["-3ff"]).err(),
            Some(TailError::IllegalOption(String::from("-3ff")))
        );
    }

    #[test]
    fn test_parse_tail_args_negated_value_is_backward() {
        // C `ARG`'s `case '-'` (`tail.c:97-102`): `off = -off` and the backward
        // style, so `-c -3` means the same as `-c 3`.
        assert_eq!(
            parse_tail_args(&["-c", "-3"]).unwrap().style,
            TailStyle::LastBytes { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-n", "-7"]).unwrap().style,
            TailStyle::LastLines { off: 7 }
        );
        // A value that is not a plain number still reaches `ARG`'s `*p` test with
        // its sign attached: `tail -n -3c` takes `-3c` as the number, and C
        // reports `illegal offset -- -3c`.
        assert_eq!(
            parse_tail_args(&["-n", "-3c"]).err(),
            Some(TailError::IllegalCount(String::from("-3c")))
        );
        // A lone `-` after the sign is not a number either; the message carries
        // the whole option argument.
        assert_eq!(
            parse_tail_args(&["-n", "-"]).err(),
            Some(TailError::IllegalCount(String::from("-")))
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_scan_survives_letter_options() {
        // C's `obsolete()` walk does not stop at `f`/`r`, and for `b`/`c`/`n`
        // written as its own token it steps over the *value* and keeps scanning.
        // So `-r -3c` rewrites: `-c-3` under `rflag` is the three trailing bytes
        // bottom-up (`RBYTES`), not a usage error.
        assert_eq!(
            parse_tail_args(&["-r", "-3c"]).unwrap().style,
            TailStyle::ReverseBytes { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-r", "-5"]).unwrap().style,
            TailStyle::ReverseLines { off: 5 }
        );
        // The value word is consumed by its option, so an old-style number after
        // `-n` is *not* rewritten: `-n -3c` above is the proof, and here the
        // operand is untouched.
        assert_eq!(
            parse_tail_args(&["-n", "3", "file"]).unwrap().operands,
            alloc::vec![String::from("file")]
        );
    }

    #[test]
    fn test_parse_tail_args_second_number_option_is_usage_error() {
        // C `ARG` opens with `if (style) usage()` (`tail.c:86-87`): two number
        // options of any kind — repeated or mixed — are a usage error, never a
        // silent override.
        assert_eq!(
            parse_tail_args(&["-n", "3", "-n", "5"]).err(),
            Some(TailError::Usage)
        );
        assert_eq!(
            parse_tail_args(&["-c10", "-b2"]).err(),
            Some(TailError::Usage)
        );
        assert_eq!(
            parse_tail_args(&["-n", "3", "-c", "5"]).err(),
            Some(TailError::Usage)
        );
        // One number option alongside `-r` is still fine.
        assert_eq!(
            parse_tail_args(&["-r", "-n", "3"]).unwrap().style,
            TailStyle::ReverseLines { off: 3 }
        );
    }

    /// 行腿的终止状态：中间行永远带着输入里那枚换行，只有输入的最后一行可能没有。
    /// 判据来自参照件对 `head`/`tail`/`nl`/`uniq` 四支壳的无尾换行读数（18 篇批次
    /// 四百二十三），C 的行腿从不像旧写法那样替输入补一枚换行。
    #[test]
    fn lines_of_marks_only_the_final_line_as_unterminated() {
        let marked = lines_of(b"alpha\nbeta\ngamma");
        assert_eq!(
            marked
                .iter()
                .map(|line| (line.bytes(), line.terminated()))
                .collect::<Vec<_>>(),
            vec![
                (b"alpha".as_slice(), true),
                (b"beta".as_slice(), true),
                (b"gamma".as_slice(), false)
            ]
        );
        // 换行结尾时多交回的那片空串不是行，两枚相邻换行之间那片才是真行。
        assert_eq!(lines_of(b"alpha\n").len(), 1);
        let blanks = lines_of(b"a\n\nb");
        assert_eq!(
            blanks
                .iter()
                .map(|line| (line.bytes(), line.terminated()))
                .collect::<Vec<_>>(),
            vec![
                (b"a".as_slice(), true),
                (b"".as_slice(), true),
                (b"b".as_slice(), false)
            ]
        );
        assert!(lines_of(b"").is_empty());
    }

    /// 一枚解不出 UTF-8 的字节不是「这一行没了」，更不是「这份输入没了」。C 的行腿
    /// （`nl.c:288` 的 `fgets`、`head.c:169-172` 的 `getc`）读的是字节，参照件对同样的
    /// 输入连字节一起交回（`tmp/head-415/head_ref`、`tmp/nl-417/nl_ref` 现量）。
    /// 行一旦先解码再切，一枚非法字节就让整份输入变成空串，这里交出的是零行。
    #[test]
    fn an_undecodable_byte_is_still_a_line() {
        let marked = lines_of(b"a\xffb\nc\n");
        assert_eq!(
            marked
                .iter()
                .map(|line| (line.bytes(), line.terminated()))
                .collect::<Vec<_>>(),
            vec![(b"a\xffb".as_slice(), true), (b"c".as_slice(), true)],
            "含非法字节的行必须留在行里，且两行都带着输入里那枚换行"
        );
        // 末行只剩一枚非法字节、且没有换行：行还在，终止状态也还得如实记。
        let tail = lines_of(b"\xff");
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].bytes(), b"\xff");
        assert!(!tail[0].terminated());
    }

    /// `eol` 是写出侧唯一的取字节处：带着换行就交回换行，没有就交回空。
    #[test]
    fn a_line_writes_back_the_newline_it_had() {
        assert_eq!(Line::from("a").eol(), b"\n");
        assert_eq!(Line::raw(b"a", false).eol(), b"");
        assert!(final_line_terminated(b""));
        assert!(final_line_terminated(b"a\n"));
        assert!(!final_line_terminated(b"a"));
    }
}

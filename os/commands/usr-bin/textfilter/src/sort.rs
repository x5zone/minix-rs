//! Key parsing and comparison for `sort`.
//!
//! Ground truth: `minix3/usr.bin/sort/` (NetBSD). The C extracts each
//! record's sort keys into transformed key buffers and compares those
//! buffers byte-wise (`fields.c`); the observable semantics — which this
//! module reimplements at the value level — are: key specifications
//! `-k F[.C][flags][,F[.C][flags]]` with per-key flags from `optval`'s table
//! `b d f i l n x r` (init.c:222-242; the scan **stops at the first letter not in
//! that table** — init.c:173 — and the leftover decides the next sentence, while
//! `.0` on the start column is an `illegal offset` and on the end column is legal
//! because only the start column's indent gets decremented — init.c:168-171),
//! fields separated by blank runs or by the `-t` delimiter byte, default
//! whole-line keys, the modifier transformations dictionary (`d`), fold
//! (`f`), ignore non-printable (`i`), numeric (`n`, blank-skipping sign
//! and fraction aware), sort-by-field-length (`l`, `fields.c` 的 `length()`：跳完前导
//! 空白后取跨度字节数），and reverse (`r`; a local `r` is dropped when a global
//! `-r` stands — init.c:195-197). There is **no month flag in this C**: `M` is in
//! neither `SORT_OPTS` (sort.c:148) nor `optval`, so `-M` falls to `usage()` and
//! `-k 1M` to `illegal field descriptor`. The last letter of `optval`'s table, `x`
//! (hex numeric), is **not** modelled: C 的那句 `numhex()` 把 `sscanf(… "%llx", &n)`
//! 施在**目的缓冲区游标** `pos` 而不是字段本身（`fields.c` 里 `numhex` 的那三行），所以它的
//! 读数依赖键缓冲的复用，在值级无从复制——本壳把 `x` 留在集外字母那一支自报，不假装同形。
//! Ties fall back to the whole line in byte order, and the global `-r`
//! inverts the final outcome.
//!
//! The engine (external merge over temporary files) is the execution
//! layer; this module compares slices, which is what the observable
//! output order is made of. Nothing allocates: modifiers apply as
//! predicates over the existing bytes. The option surface itself needs
//! one growable list (the key specifications), served by `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Blank bytes for field separation: space and tab (the C locale's
/// `<blank>` class).
fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// One parsed `-k` key specification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeySpec {
    /// First field (1 based).
    pub start_field: usize,
    /// First character within the field (1 based; zero means field start).
    pub start_char: usize,
    /// Skip leading blanks at the key start (`b` on the start segment).
    pub blank_start: bool,
    /// Last field (1 based; zero means the last field of the line).
    pub end_field: usize,
    /// Last character within the end field (zero means field end).
    pub end_char: usize,
    /// Skip leading blanks in the end field (`b` on the end segment).
    pub blank_end: bool,
    /// Per-key modifiers.
    pub dictionary: bool,
    pub fold: bool,
    pub ignore_nonprint: bool,
    pub numeric: bool,
    /// Sort by the **length** of the selected span (`l`, `fields.c` 的 `length()`)。
    /// C 在 `enterfield` 里把这一位排在数值位之前（`:208-211`），所以 `l` 与 `n` 同时存在时按长度走。
    pub by_length: bool,
    /// This key's comparison inverts (dropped when a global `-r` stands).
    pub reverse: bool,
}

impl KeySpec {
    /// The default whole-line key (no `-k` given).
    pub fn whole_line() -> Self {
        KeySpec {
            start_field: 1,
            start_char: 1,
            blank_start: false,
            end_field: 0,
            end_char: 0,
            blank_end: false,
            dictionary: false,
            fold: false,
            ignore_nonprint: false,
            numeric: false,
            by_length: false,
            reverse: false,
        }
    }
}

/// Global options for one `sort` run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Key specifications (empty = one whole-line key).
    pub keys: Vec<KeySpec>,
    /// Global reverse.
    pub reverse: bool,
    pub fold: bool,
    pub dictionary: bool,
    pub ignore_nonprint: bool,
    pub numeric: bool,
    /// Global sort-by-length (`-l`); same shape as the other flag letters.
    pub by_length: bool,
    /// Leading-blank skip as a global flag.
    pub blank_skip: bool,
    /// Suppress lines that compare equal to their predecessor.
    pub unique: bool,
    /// Check only: report disorder and duplicates, sort nothing.
    pub check: bool,
    /// Field delimiter byte (`-t`).
    pub delimiter: Option<u8>,
    /// Merge already-sorted runs instead of sorting (`-m`, C `sort -m`).
    pub merge: bool,
    /// `-o path`: where the result goes. `None` is C's null `outpath`, which writes
    /// to standard output (`sort.c:320-323`); `Some("")` is *not* the same thing —
    /// C carries the empty string into `lstat`/`fopen` and fails there, so the
    /// option has to be distinguishable from its absence. The path also decides
    /// *how* it is written, see [`decide_output`], because C replaces an existing
    /// file through a temporary rather than truncating it in place.
    pub output: Option<Vec<u8>>,
}

/// Merge several already-sorted runs into one sorted vector, the way C's
/// `sort -m` concatenates sorted inputs by merging rather than re-sorting. Each
/// run is assumed already ordered under `options`; equal keys keep the earlier
/// run's line first (a stable, left-biased merge). Because nothing is
/// re-sorted, `-m` on runs that are not each sorted yields a merge of the runs,
/// exactly as the C does.
pub fn merge_sorted(runs: &[&[String]], options: &Options) -> Vec<String> {
    let mut acc: Vec<String> = Vec::new();
    for run in runs {
        acc = merge_two(&acc, run, options);
    }
    acc
}

fn merge_two(a: &[String], b: &[String], options: &Options) -> Vec<String> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        if compare(&b[j], &a[i], options) == core::cmp::Ordering::Less {
            out.push(b[j].clone());
            j += 1;
        } else {
            out.push(a[i].clone());
            i += 1;
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

/// 一条不可解析的选项参数错。三条出口对位 C 的三条真出路（见各变体注释），
/// 不是一句「bad option」到底。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// libc 的 `getopt` 自己那一行（集外字母或带值选项缺值），随后才是 `sort.c:252`
    /// 的 `usage(NULL)`——所以真源是两行。两类各自带字母。
    Getopt(crate::GetoptReport),
    /// `-k` 的键形不对：`setcolumn`/`setfield` 里的五句 `errx(2, …)` 之一（
    /// [`KeySpecError`]）。五句各有判据，但出口相同：退 2、**不接 usage**。
    /// 之前五句挤在一个 `IllegalFieldDescriptor` 里，所以四句的报文都在拿别人的句子签字。
    KeySpec(KeySpecError),
    /// `-t` 给了第二次：`sort.c:214-215` 的 `usage("multiple field delimiters")`，
    /// 即先一句带程名的话，再接整个 usage 块。
    MultipleFieldDelimiters,
}

/// `minix3/usr.bin/sort/init.c` 里 `setcolumn()` 与 `setfield()` 的五句 `errx(2, …)`。
/// 句文本单源在 [`KeySpecError::sentence`]，壳只负责加程序名与退 2。
///
/// 五句都是拿编译出来的参照件逐形量出来的（`tmp/family-diag-ref/sort_ref`，由
/// `minix3/usr.bin/sort/` 的真源加垫片链成），不是推出来的：
///
/// | 形 | C 的那一句 |
/// |---|---|
/// | `-k 0`、`-k x`、`-k ''`、`-k ,5`、`-k 1,-2` | `field numbers must be positive` |
/// | `-k 1.0` | `illegal offset` |
/// | `-k 1,0.5` | `cannot indent end of line` |
/// | `-k 1 2`、`-k 1n2`、`-k 1.2.3` | `illegal field descriptor` |
/// | `-k 2.5,1.2` | `fields out of order` |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySpecError {
    /// `init.c:160-161` 的 `col->num <= 0 && !(col->num == 0 && col == &tcol)`：
    /// 开始列必须为正数；结束列允许 0（那表示到行尾），但负数两列都拒。
    /// 注意判据用的是 `strtol` 的结果，所以 `-k x` 与 `-k ''` 也是零（一个数字也没转出来）。
    FieldNumbersNotPositive,
    /// `init.c:163-164`：列号是 0（到行尾）时不能再给 `.C` 缩进。
    CannotIndentEndOfLine,
    /// `init.c:168-171`：缩进算出来是负数。只有**开始列**会 `col->indent--`，
    /// 所以 `1.0` 报错而 `1,2.0` 合法——两列的 `.0` 不对称。
    IllegalOffset,
    /// `init.c:188-190`：开始列停下后既不是串尾也不是逗号。
    IllegalFieldDescriptor,
    /// `init.c:210-216`：结束列的（字段号，缩进）整体落在开始列之前。三个条件全要
    /// 成立才报，缺一不报——`-k 2,1` 就不报（结束列没给 `.C` 时缩进是 0，源码里那句
    /// 注释写着「indent if 0 -> end of field, i.e. okay」）。
    FieldsOutOfOrder,
}

impl KeySpecError {
    /// C 那句 `errx` 的格式串原文（五句都没有 `%s`，坏文本不会被拼回句子）。
    pub fn sentence(self) -> &'static str {
        match self {
            KeySpecError::FieldNumbersNotPositive => "field numbers must be positive",
            KeySpecError::CannotIndentEndOfLine => "cannot indent end of line",
            KeySpecError::IllegalOffset => "illegal offset",
            KeySpecError::IllegalFieldDescriptor => "illegal field descriptor",
            KeySpecError::FieldsOutOfOrder => "fields out of order",
        }
    }
}

/// Parse global options from the argument words, returning the options
/// plus the remaining operand words (file paths in the C; the doing half
/// rejects them while the open-existing call is gated). `-k` and `-t`
/// take a value, given separately or inline; 集外字母与缺值各自成一个
/// [`ParseError::Getopt`]，键形不对与 `-t` 给第二次则是另两条出口。
///
/// 本解析器已建模的带值字母是 `k`、`o`、`t`：C 的选项串是 `sort.c:148` 的
/// `#define SORT_OPTS "bcdD:fHik:lmno:rR:sSt:T:ux"`，其中 `:` 标出的是 `D`、`k`、`o`、
/// `R`、`t`、`T` 六个；但 `D`/`R`/`T`（以及旗标 `S`/`x`）在本树尚未建模，它们现在
/// 落进「集外字母」那一支——那是**选项面**的缺口（另案登记）。分成两条 libc 报文
/// （集外字母走 `getopt.c:101-104`，缺值走 `:132-136`）的做法就体现在下面各臂里。
pub fn parse_options<'a>(args: &[&'a str]) -> Result<(Options, Vec<&'a str>), ParseError> {
    let mut options = Options::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if !arg.starts_with('-') || arg.len() <= 1 {
            operands.push(arg);
            index += 1;
            continue;
        }
        let mut chars = arg[1..].chars().peekable();
        while let Some(flag) = chars.next() {
            match flag {
                'r' => options.reverse = true,
                'f' => options.fold = true,
                'd' => options.dictionary = true,
                'i' => options.ignore_nonprint = true,
                'n' => options.numeric = true,
                // `-l` 在 C 里与 `-d`/`-f`/`-i`/`-n` 同臂（`sort.c:168`），效果也是那一句
                // `fldtab[0].flags |= optval(ch, 0)`（`:172`），所以它是一个全局默认标志。
                'l' => options.by_length = true,
                'b' => options.blank_skip = true,
                'u' => options.unique = true,
                'c' => options.check = true,
                'm' => options.merge = true,
                // `-H`: a documented no-op in C — `sort.c:174-176` comments that
                // `-H` "was; use merge sort for blocks of large files — that is
                // now the default", so it accepts the flag and does nothing. We
                // match that: accept, no effect (previously it errored as bad).
                'H' => {}
                // `-s`: stable sort. C `sort.c:217-222` sets `posix_sort = 0`,
                // which its own comment notes is already the NetBSD default, and
                // `slice::sort_by` (used below) is stable — so it is a no-op here
                // too, matching C's observable behavior.
                's' => {}
                // `-o` takes a value, inline (`-oNAME`) or as the next word
                // (`sort.c:188-190`); a missing one is C's `?` and lands on the
                // usage path. A later `-o` overwrites the earlier one, as C's
                // `outpath = optarg` assignment does.
                'o' => {
                    let mut value: String = chars.by_ref().collect();
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = (*word).to_string(),
                            None => {
                                return Err(ParseError::Getopt(crate::GetoptReport::MissingValue(
                                    flag,
                                )));
                            }
                        }
                    }
                    options.output = Some(value.into_bytes());
                    break;
                }
                'k' | 't' => {
                    let mut value: String = chars.by_ref().collect();
                    if value.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(word) => value = (*word).to_string(),
                            None => {
                                return Err(ParseError::Getopt(crate::GetoptReport::MissingValue(
                                    flag,
                                )));
                            }
                        }
                    }
                    if flag == 'k' {
                        match parse_key(&value, &options) {
                            // C 这一支不写 usage：`setfield`/`setcolumn` 里是那五句
                            // `errx(2, …)` 之一（`init.c:160-216`），句子逐句不同。
                            Ok(spec) => options.keys.push(spec),
                            Err(error) => return Err(ParseError::KeySpec(error)),
                        }
                    } else {
                        // `-t` 在 C 里只取首字节（`sort.c:219` 的 `d_mask[(u_char)*optarg]`），
                        // 从不查长度；但同一个选项给第二次就是
                        // `usage("multiple field delimiters")`（`:214-215`）。
                        if options.delimiter.is_some() {
                            return Err(ParseError::MultipleFieldDelimiters);
                        }
                        // 空值（`-t ''`）在 C 取到那个 `\0`，不报错。
                        options.delimiter = Some(value.bytes().next().unwrap_or(0));
                    }
                    break;
                }
                _ => {
                    return Err(ParseError::Getopt(crate::GetoptReport::UnknownOption(flag)));
                }
            }
        }
        index += 1;
    }
    Ok((options, operands))
}

/// What a `lstat` of the requested output path tells the C, narrowed to the two
/// questions its branch asks (`sort.c:324-326`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputProbe {
    /// `st_mode & ALLPERMS`, the permission bits C copies onto the temporary before
    /// it becomes the output name (`sort.c:366`, `ALLPERMS` = `0o7777`).
    pub mode: u32,
    /// `S_ISCHR(st.st_mode) || S_ISBLK(st.st_mode)`: writing straight through is
    /// correct for a device, and C says so out loud.
    pub is_device: bool,
}

/// Where the sorted text goes, the three ways C reads `sort.c:320-355`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// No `-o`: `stdout`, and no path is ever touched.
    StandardOutput,
    /// Open the named path for writing and truncate it — the branch C takes when
    /// `lstat` fails (no such file) or reports a character/block special
    /// (`sort.c:352-355`).
    Direct { path: Vec<u8> },
    /// Write `<path>XXXXXX`, then copy the probed mode onto it, unlink the original
    /// name, hard-link the temporary to it, and unlink the temporary
    /// (`sort.c:327-379`). A crash mid-write therefore leaves the old file standing.
    ViaTemp { path: Vec<u8>, mode: u32 },
}

/// C's choice between those three, as a function of the option and the probe. An
/// absent `-o` is the only way to reach standard output; an empty one still names a
/// path, and C's `fopen("", "w")` fails on it (`sort.c:354-355`).
pub fn decide_output(output: Option<&[u8]>, probe: Option<OutputProbe>) -> Output {
    let Some(output) = output else {
        return Output::StandardOutput;
    };
    let path = output.to_vec();
    match probe {
        Some(found) if !found.is_device => Output::ViaTemp {
            path,
            mode: found.mode,
        },
        _ => Output::Direct { path },
    }
}

/// The six characters that replace the `XXXXXX` of `sort.c:338-339`. C's `mkstemp`
/// fills them from `padchar` — sixty-two characters in the order digits, uppercase,
/// lowercase (`lib/libc/stdio/gettemp.c:49-50`) — drawing one at random per slot, and
/// then carries the name forward while a create keeps failing (`:120-170`). This
/// shell takes the same alphabet and walks it the same way from a per-process seed
/// instead of a random one; all the caller relies on is that the name keeps the
/// requested prefix, stays six characters long, and differs between attempts.
pub fn temp_candidate(path: &[u8], seed: u64, attempt: usize) -> Vec<u8> {
    const PAD_CHAR: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut value = seed.wrapping_add(attempt as u64);
    let mut suffix = [0u8; 6];
    for slot in suffix.iter_mut().rev() {
        *slot = PAD_CHAR[(value % PAD_CHAR.len() as u64) as usize];
        value /= PAD_CHAR.len() as u64;
    }
    let mut out = path.to_vec();
    out.extend_from_slice(&suffix);
    out
}

/// 解一条 `-k` 规格。形状逐句对位 `init.c:181-220` 的 `setfield()`，包括**判据的先后**：
/// 先解两列（各自的句子），再看残留（`illegal field descriptor`），然后才做四次标志
/// 合成（局部为空才继承全局、全局 `-r` 抹掉局部 `r`、全局的 BI/BT 无条件并上、结束列
/// 没缩进时抹掉 BT），最后才查 `fields out of order`。
///
/// 与旧写法的两处实质差别：① 旧写法把五句 `errx` 挤成一句，所以四句的报文都在拿别人的
/// 句子签字；② 旧写法把全局标志写成「逐位或」，而 C 是「局部一个字母也没有才整体继承」
///（`init.c:193-194`），于是 `-n -k 1r` 这种形在本树按数值排、在 C 按字典序反排。
///
/// 本树尚未建模的修饰字母只剩 `x`（十六进制数，`__minix` 专有）：C 的那句 `numhex()`
/// 把 `sscanf(… "%llx", &n)` 施在目的缓冲区的写游标而不是字段本身（`fields.c` 里 `numhex`
/// 的那几行），读数依赖键缓冲的复用，值级无从复制。所以下面的循环**不把 `x` 收进来**（它
/// 现在被当残留字符拒掉，报 `illegal field descriptor`），全局 `-x` 也仍走集外字母那一支——
/// 故意不静默接受：接受而无效会把「顺序错」伪装成「顺序对」。
/// `M` 不在 `optval` 表里，也不在 `SORT_OPTS`（`sort.c:148`）里，`switch` 同样没有
/// `case 'M'`——所以 `-M` 与 `-k 1M` 在 C 都不认（参照件实测：前者落 `usage`，后者报
/// `illegal field descriptor`）；旧写法把月名排序当成本树功能，那是无源建模。
pub fn parse_key(spec: &str, global: &Options) -> Result<KeySpec, KeySpecError> {
    let start = scan_column(spec, false)?;
    // `setfield` 只看开始列停下的地方：串尾就是「键延伸到行尾」，逗号就开始解结束列，
    // 别的字符是 `illegal field descriptor`（`init.c:188-190`）。
    let end_text = if start.rest.is_empty() {
        None
    } else if let Some(tail) = start.rest.strip_prefix(',') {
        Some(tail)
    } else {
        return Err(KeySpecError::IllegalFieldDescriptor);
    };
    let end = match end_text {
        Some(text) => scan_column(text, true)?,
        // 没给结束列：C 把 `tcol.num` 留在 0（到行尾），也不扫任何字母。
        None => Column::absent(),
    };

    let local = start.merge(&end);
    // `init.c:193-194`：`if (!cur_fld->flags) cur_fld->flags = gflag;`——局部只要有
    // 一个修饰字母就不再继承全局标志。
    let (dictionary, fold, ignore_nonprint, numeric, by_length, reverse) = if local.any {
        (
            local.dictionary,
            local.fold,
            local.ignore_nonprint,
            local.numeric,
            local.by_length,
            local.reverse,
        )
    } else {
        (
            global.dictionary,
            global.fold,
            global.ignore_nonprint,
            global.numeric,
            global.by_length,
            local.reverse,
        )
    };
    // `init.c:195-197`：全局 `-r` 存在时，局部 `r` 被抹掉（不是再反一次）。
    let reverse = if global.reverse { false } else { reverse };
    // `init.c:206`：全局的 BI/BT 是**无条件并上**的（与上面那句不一样），而 C 的全局
    // `-b` 一次就点亮两个位（`sort.c:159` 的 `fldtab[0].flags |= BI | BT`）。
    let blank_start = local.bi || global.blank_skip;
    let mut blank_end = local.bt || global.blank_skip;
    // `init.c:207-208`：`if (!cur_fld->tcol.indent) flags &= ~BT`——结束列没缩进时 BT 无意义。
    if end.eff_indent == 0 {
        blank_end = false;
    }
    // `init.c:210-216` 的三个条件全要成立：结束列有字段号、不是「只有 BT 没有 BI」、
    // 且结束列（号，缩进）落在开始列之前。C 写的是 `!(!(flags & BI) && (flags & BT))`
    //（`:211`），下面那句是它的德摩根改写（clippy 不接受双重否定形，同值）。
    let not_bt_without_bi = !local.bt || local.bi;
    if end.num != 0
        && not_bt_without_bi
        && end.num <= start.num
        && end.eff_indent != 0
        && end.eff_indent < start.eff_indent
    {
        return Err(KeySpecError::FieldsOutOfOrder);
    }
    Ok(KeySpec {
        start_field: start.num as usize,
        start_char: start.raw_indent as usize,
        blank_start,
        end_field: end.num as usize,
        end_char: end.eff_indent as usize,
        blank_end,
        dictionary,
        fold,
        ignore_nonprint,
        numeric,
        by_length,
        reverse,
    })
}

/// 一列扫出来的形状，对位 C 的 `struct column`（`sort.h`）。
#[derive(Debug, Clone, Copy)]
struct Column<'a> {
    /// 字段号；结束列允许 0（表示到行尾）。
    num: i64,
    /// `.C` 的原样读数（没给就是 0），交给 [`KeySpec::start_char`]。
    raw_indent: i64,
    /// C 存在 `col->indent` 里的那个数：开始列在 `.C` 上减一（`init.c:168-169`），
    /// 没给 `.C` 时两列都是 0（结构体是 memset 过的）。
    eff_indent: i64,
    /// 下面八个位就是 `optval` 给这一列置上的标志。
    bi: bool,
    bt: bool,
    dictionary: bool,
    fold: bool,
    ignore_nonprint: bool,
    numeric: bool,
    by_length: bool,
    reverse: bool,
    /// 一个修饰字母也没扫到（C 用 `if (!cur_fld->flags)` 判这件事）。
    any: bool,
    /// 停点之后的残留，就是 C 返回给 `setfield` 的那个 `pos`。
    rest: &'a str,
}

impl<'a> Column<'a> {
    /// 结束列缺席时的形状：C 里就是那个 memset 后没动过的 `tcol`。
    fn absent() -> Self {
        Column {
            num: 0,
            raw_indent: 0,
            eff_indent: 0,
            bi: false,
            bt: false,
            dictionary: false,
            fold: false,
            ignore_nonprint: false,
            numeric: false,
            by_length: false,
            reverse: false,
            any: false,
            rest: "",
        }
    }

    /// 两列的标志在 C 里是同一个 `cur_fld->flags` 上的或，这里同形。
    fn merge(&self, other: &Self) -> Self {
        Column {
            num: self.num,
            raw_indent: self.raw_indent,
            eff_indent: self.eff_indent,
            bi: self.bi || other.bi,
            bt: self.bt || other.bt,
            dictionary: self.dictionary || other.dictionary,
            fold: self.fold || other.fold,
            ignore_nonprint: self.ignore_nonprint || other.ignore_nonprint,
            numeric: self.numeric || other.numeric,
            by_length: self.by_length || other.by_length,
            reverse: self.reverse || other.reverse,
            any: self.any || other.any,
            rest: "",
        }
    }
}

/// C 的 `setcolumn()`（`init.c:152-178`）逐句对位。`filling_end` 对应 C 里那句
/// `col = cur_fld->icol.num ? (&cur_fld->tcol) : (&cur_fld->icol)`（`:157`）：它决定
/// `.C` 要不要减一。而 `b` 记成 BI 还是 BT 用的是另一个判据——`optval` 的 `tcolflag`
/// 是 `cur_fld->tcol.num`（`:173`），所以在结束列填了 0 的那一形（`-k 1,0b`）里，
/// `b` 仍然算 BI。两个判据不许合为一个。
fn scan_column<'a>(text: &'a str, filling_end: bool) -> Result<Column<'a>, KeySpecError> {
    let bytes = text.as_bytes();
    let (num, mut position) = scan_long(text, 0);
    // `init.c:160-161`：开始列非正即错；结束列允许 0（那表示到行尾）。
    if num <= 0 && !(num == 0 && filling_end) {
        return Err(KeySpecError::FieldNumbersNotPositive);
    }
    let tcolflag = filling_end && num != 0;
    let mut raw_indent = 0i64;
    let mut eff_indent = 0i64;
    if bytes.get(position) == Some(&b'.') {
        // `init.c:162-164`：列号是 0（到行尾）时 `.C` 无从谈起。
        if num == 0 {
            return Err(KeySpecError::CannotIndentEndOfLine);
        }
        position += 1;
        let (value, next) = scan_long(text, position);
        position = next;
        raw_indent = value;
        eff_indent = if filling_end { value } else { value - 1 };
        if eff_indent < 0 {
            return Err(KeySpecError::IllegalOffset);
        }
    }
    let mut column = Column {
        num,
        raw_indent,
        eff_indent,
        bi: false,
        bt: false,
        dictionary: false,
        fold: false,
        ignore_nonprint: false,
        numeric: false,
        by_length: false,
        reverse: false,
        any: false,
        rest: "",
    };
    // `init.c:173` 的 `for (; (tmp = optval(*pos, cur_fld->tcol.num)); pos++)`：`optval`
    // 返回 0 就**停下**，那个字符留给 `setfield` 当 `*pos` 判据，不是「略过继续」。
    while position < bytes.len() {
        match bytes[position] {
            b'b' => {
                if tcolflag {
                    column.bt = true;
                } else {
                    column.bi = true;
                }
            }
            b'd' => column.dictionary = true,
            b'f' => column.fold = true,
            b'i' => column.ignore_nonprint = true,
            b'n' => column.numeric = true,
            // `optval` 的 `case 'l': return L;`（`init.c:234`）。
            b'l' => column.by_length = true,
            b'r' => column.reverse = true,
            _ => break,
        }
        column.any = true;
        position += 1;
    }
    column.rest = &text[position..];
    Ok(column)
}

/// C 那句 `strtol(pos, &npos, 10)`（`init.c:158`、`:166`）：跳前导空白、可带一个正负号，
/// 然后才是十进制位。一个数字也没转出来时值是 0 且指针**退回起点**（这正是 `-k x`、
/// `-k ''` 落进「字段号必须为正」的原因）。超出 `i64` 时 C 饱和到 `LONG_MAX`/`LONG_MIN`
/// 且**不看 `errno`**，所以那里不报错；本树同样饱和。
fn scan_long(text: &str, from: usize) -> (i64, usize) {
    let bytes = text.as_bytes();
    let mut position = from;
    while position < bytes.len() && bytes[position].is_ascii_whitespace() {
        position += 1;
    }
    let mut negative = false;
    match bytes.get(position) {
        Some(b'-') => {
            negative = true;
            position += 1;
        }
        Some(b'+') => position += 1,
        _ => {}
    }
    let digits_start = position;
    let mut value: i64 = 0;
    while position < bytes.len() && bytes[position].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(bytes[position] - b'0'));
        position += 1;
    }
    if position == digits_start {
        return (0, from);
    }
    (if negative { -value } else { value }, position)
}

/// C `fields.c:length()` 的那一步：先 `SKIP_BLANKS(line)`，再取 `lineend - line`。
/// 所以量的不是「补白后的跨度」也不是过滤后的键字节，而是跳完前导空白到跨度末尾的
/// 字节数（`enterfield` 里的 `start`/`end`，与 [`key_bytes`] 同一套算式）。
fn span_length(line: &str, key: &KeySpec, delimiter: Option<u8>) -> usize {
    let span = key_bytes(line, key, delimiter);
    let bytes = span.as_bytes();
    let mut offset = 0usize;
    while offset < bytes.len() && is_blank(bytes[offset]) {
        offset += 1;
    }
    bytes.len() - offset
}

/// The span of the `field`-th field of a line (1 based), byte offsets
/// exclusive at the end.
///
/// With a delimiter the pieces split on every delimiter byte (empty
/// pieces are real fields). Without one, fields are the maximal
/// non-blank runs; the first field starts at byte zero so leading blanks
/// ride with it (the reason plain `sort` often needs `-b`), and a field
/// beyond the last run is the empty span at end of line.
fn field_span(line: &[u8], field: usize, delimiter: Option<u8>) -> (usize, usize) {
    match delimiter {
        Some(delimiter) => {
            let mut start = 0usize;
            let mut index = 1usize;
            let mut cursor = 0usize;
            while cursor < line.len() {
                if line[cursor] == delimiter {
                    if index == field {
                        return (start, cursor);
                    }
                    index += 1;
                    start = cursor + 1;
                }
                cursor += 1;
            }
            if index == field {
                (start, line.len())
            } else {
                (line.len(), line.len())
            }
        }
        None => {
            if field == 1 {
                let mut cursor = 0usize;
                while cursor < line.len() && is_blank(line[cursor]) {
                    cursor += 1;
                }
                while cursor < line.len() && !is_blank(line[cursor]) {
                    cursor += 1;
                }
                return (0, cursor);
            }
            // Field `field` is the (`field` - 1)th non-blank run,
            // 0 based (field 1 is handled above with its leading blanks).
            let target = field - 1;
            let mut cursor = 0usize;
            let mut run = 0usize;
            while cursor < line.len() {
                while cursor < line.len() && is_blank(line[cursor]) {
                    cursor += 1;
                }
                if cursor >= line.len() {
                    break;
                }
                let start = cursor;
                while cursor < line.len() && !is_blank(line[cursor]) {
                    cursor += 1;
                }
                if run == target {
                    return (start, cursor);
                }
                run += 1;
            }
            (line.len(), line.len())
        }
    }
}

/// Extracts one key's bytes from a line per the specification.
fn key_bytes<'a>(line: &'a str, key: &KeySpec, delimiter: Option<u8>) -> &'a str {
    let bytes = line.as_bytes();
    let (start_field_start, start_field_end) = field_span(bytes, key.start_field, delimiter);
    let mut start = start_field_start;
    if key.blank_start {
        while start < start_field_end && is_blank(bytes[start]) {
            start += 1;
        }
    }
    start = start
        .saturating_add(key.start_char.saturating_sub(1))
        .min(line.len());

    if key.end_field == 0 {
        return &line[start..];
    }
    let (end_field_start, end_field_end) = field_span(bytes, key.end_field, delimiter);
    let mut base = end_field_start;
    if key.blank_end {
        while base < end_field_end && is_blank(bytes[base]) {
            base += 1;
        }
    }
    let mut end = if key.end_char > 0 {
        (base + key.end_char).min(end_field_end)
    } else {
        end_field_end
    };
    if end < start {
        end = start;
    }
    &line[start..end]
}

/// A byte passes this key's `d`/`i` filters; `f` folds through the
/// mapping below.
fn visible_and_folded(byte: u8, key: &KeySpec) -> Option<u8> {
    if key.dictionary && !(byte.is_ascii_alphanumeric() || is_blank(byte)) {
        return None;
    }
    if key.ignore_nonprint && !(0x20..=0x7E).contains(&byte) {
        return None;
    }
    Some(if key.fold {
        byte.to_ascii_uppercase()
    } else {
        byte
    })
}

/// Compares two key slices through the `d`/`f`/`i` filters without
/// materialising them: both streams are filtered lazily and compared in
/// lockstep, then by visible length.
fn cmp_filtered(a: &[u8], b: &[u8], key: &KeySpec) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let mut ai = a.iter().filter_map(|b| visible_and_folded(*b, key));
    let mut bi = b.iter().filter_map(|b| visible_and_folded(*b, key));
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
            }
        }
    }
}

/// A `-n` numeric key parsed in place: blank-skipping optional sign,
/// digit run, optional fraction; an empty parse is zero.
#[derive(Debug, PartialEq, Eq)]
struct NumericKey<'a> {
    negative: bool,
    integer: &'a [u8],
    fraction: &'a [u8],
}

fn numeric_key(bytes: &[u8]) -> NumericKey<'_> {
    let mut cursor = 0usize;
    while cursor < bytes.len() && is_blank(bytes[cursor]) {
        cursor += 1;
    }
    let negative = bytes.get(cursor) == Some(&b'-');
    if negative {
        cursor += 1;
    }
    let integer_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    let integer = &bytes[integer_start..cursor];
    let fraction_start = cursor + 1;
    if cursor < bytes.len() && bytes[cursor] == b'.' {
        cursor += 1;
        let fraction = &bytes[fraction_start
            ..cursor
                + bytes[fraction_start..]
                    .iter()
                    .take_while(|b| b.is_ascii_digit())
                    .count()];
        return NumericKey {
            negative,
            integer,
            fraction,
        };
    }
    NumericKey {
        negative,
        integer,
        fraction: &[],
    }
}

fn trim_leading_zeros(mut digits: &[u8]) -> &[u8] {
    while digits.len() > 1 && digits[0] == b'0' {
        digits = &digits[1..];
    }
    digits
}

fn is_numeric_zero(key: &NumericKey<'_>) -> bool {
    trim_leading_zeros(key.integer) == b"0" && {
        let mut fraction = key.fraction;
        while fraction.last() == Some(&b'0') {
            fraction = &fraction[..fraction.len() - 1];
        }
        fraction.is_empty()
    }
}

fn compare_numeric(a: &NumericKey<'_>, b: &NumericKey<'_>) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    if is_numeric_zero(a) && is_numeric_zero(b) {
        return Ordering::Equal;
    }
    match (a.negative, b.negative) {
        (false, true) => return Ordering::Greater,
        (true, false) => return Ordering::Less,
        _ => {}
    }
    let magnitude = {
        let ai = trim_leading_zeros(a.integer);
        let bi = trim_leading_zeros(b.integer);
        let by_length = ai.len().cmp(&bi.len());
        let by_lex = ai.cmp(bi);
        if by_length != Ordering::Equal {
            by_length
        } else {
            by_lex
        }
    };
    let magnitude = match magnitude {
        Ordering::Equal => {
            let mut longer = a.fraction.len().max(b.fraction.len());
            let mut index = 0usize;
            let mut ordering = Ordering::Equal;
            while index < longer {
                let x = a.fraction.get(index).copied().unwrap_or(b'0');
                let y = b.fraction.get(index).copied().unwrap_or(b'0');
                if x != y {
                    ordering = x.cmp(&y);
                    break;
                }
                index += 1;
            }
            longer = 0;
            let _ = longer;
            ordering
        }
        other => other,
    };
    if a.negative {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// Compares two lines per the options: each key spec contributes a
/// comparison, ties fall through to the next key and then to the whole
/// line in byte order, and the global `-r` inverts the outcome.
pub fn compare(a: &str, b: &str, options: &Options) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    // Without `-k` the global modifiers apply to the whole line: one
    // whole-line key carrying them (and the global reverse) is built
    // here.
    let owned_key;
    let keys: &[KeySpec] = if options.keys.is_empty() {
        owned_key = KeySpec {
            fold: options.fold,
            dictionary: options.dictionary,
            ignore_nonprint: options.ignore_nonprint,
            numeric: options.numeric,
            by_length: options.by_length,
            blank_start: options.blank_skip,
            ..KeySpec::whole_line()
        };
        alloc::slice::from_ref(&owned_key)
    } else {
        &options.keys[..]
    };
    let mut result = Ordering::Equal;
    for key in keys {
        let ka = key_bytes(a, key, options.delimiter);
        let kb = key_bytes(b, key, options.delimiter);
        let ordering = if key.by_length {
            // C 那句是 `length()`：跳完前导空白后把「跨度还有几个字节」写成十进制串，
            // 再交给 `number()`。对纯十进制串来说数值序就是整数序，所以直接比长度。
            span_length(a, key, options.delimiter).cmp(&span_length(b, key, options.delimiter))
        } else if key.numeric {
            compare_numeric(&numeric_key(ka.as_bytes()), &numeric_key(kb.as_bytes()))
        } else {
            cmp_filtered(ka.as_bytes(), kb.as_bytes(), key)
        };
        let ordering = if key.reverse {
            ordering.reverse()
        } else {
            ordering
        };
        if ordering != Ordering::Equal {
            result = ordering;
            break;
        }
    }
    if result == Ordering::Equal {
        // The C's last-resort comparison: the raw whole lines in byte
        // order, with the global reverse applied (the modifiers belong
        // to the keys, not to this fallback).
        let raw = a.as_bytes().cmp(b.as_bytes());
        return if options.reverse { raw.reverse() } else { raw };
    }
    if options.reverse {
        result.reverse()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    fn opts(extra: &str) -> Options {
        let mut words: Vec<&str> = Vec::new();
        for flag in extra.split_whitespace() {
            words.push(flag);
        }
        match parse_options(&words) {
            Ok((options, _operands)) => options,
            Err(_) => panic!("options should parse"),
        }
    }

    #[test]
    fn test_plain_byte_order() {
        let options = opts("");
        assert_eq!(compare("apple", "banana", &options), Ordering::Less);
        assert_eq!(compare("ab", "ab", &options), Ordering::Equal);
    }

    #[test]
    fn test_reverse_inverts() {
        let options = opts("-r");
        assert_eq!(compare("apple", "banana", &options), Ordering::Greater);
    }

    #[test]
    fn test_numeric_with_signs_and_fractions() {
        let options = opts("-n");
        assert_eq!(compare("  -3.5", "2", &options), Ordering::Less);
        assert_eq!(compare("2.10", "2.5", &options), Ordering::Less);
        // POSIX -n has no plus sign: "+7" is not a number and sorts as
        // zero, equal to "007"; the whole-line fallback then separates
        // them by raw bytes ("+" sorts before "0").
        assert_eq!(compare("+7", "007", &options), Ordering::Less);
        // Non-numeric prefixes compare as zero; the whole-line fallback
        // then orders them by raw bytes.
        assert_eq!(compare("abc", "  ", &options), Ordering::Greater);
    }

    #[test]
    fn test_fold_and_dictionary() {
        // The fold makes the keys equal; the C's last-resort comparison
        // then falls back to the raw whole line, where 'A' < 'a'.
        let options = opts("-f");
        assert_eq!(compare("Apple", "apple", &options), Ordering::Less);
        assert_eq!(compare("Apple", "Apple", &options), Ordering::Equal);
        let dictionary = opts("-d");
        // The '+' byte filters away on both sides: "ab" versus "abc".
        assert_eq!(compare("a+b", "a+bc", &dictionary), Ordering::Less);
    }

    #[test]
    fn test_key_spec_field_selection_with_delimiter() {
        let global = opts("-t:");
        let spec = parse_key("2,2", &global).unwrap();
        let mut options = opts("-t:");
        options.keys.clear();
        options.keys.push(spec);
        // -t: -k2,2 compares the second colon field only; different keys
        // order by it, and the whole line breaks the tie.
        assert_eq!(compare("x:b:zz", "a:a:yy", &options), Ordering::Greater);
        assert_eq!(compare("x:b:zz", "a:b:yy", &options), Ordering::Greater);
        assert_eq!(compare("x:b:1", "x:b:2", &options), Ordering::Less);
    }

    /// `.0` 在两列上不对称：只有开始列会 `col->indent--`（`init.c:168-169`），所以
    /// 开始列的 `.0` 算出 -1 走 `illegal offset`，而结束列的 `.0`（以及光一个点）
    /// 缩进是 0，表示「到字段末」——参照件同样接受。
    #[test]
    fn dot_zero_is_an_error_only_on_the_start_column() {
        assert_eq!(
            parse_key("1.0", &opts("")).err(),
            Some(KeySpecError::IllegalOffset)
        );
        assert_eq!(parse_key("1.2", &opts("")).unwrap().start_char, 2);
        assert!(parse_key("1,2.0", &opts("")).is_ok());
        assert!(parse_key("1,2.", &opts("")).is_ok());
    }

    /// 五句 `errx` 各自有自己的形与句子，读数拿 `tmp/family-diag-ref/sort_ref` 逐形量过。
    /// 旧写法把五句挤成一句，所以除 `1 2` 之外四句都在拿别人的句子签字。
    #[test]
    fn the_five_key_spec_sentences_are_distinct() {
        // `field numbers must be positive`（`init.c:160-161`）：零、负数、以及一个数字
        // 也没转出来的形（`strtol` 交回 0 且指针不动）。
        for spec in ["0", "x", "", ",5", "1,-2", "-1"] {
            assert_eq!(
                parse_key(spec, &opts("")).err(),
                Some(KeySpecError::FieldNumbersNotPositive),
                "键形 {spec:?}"
            );
        }
        // `illegal offset`（`:168-170`）：开始列 `.0` 减一后为负。
        assert_eq!(
            parse_key("1.0", &opts("")).err(),
            Some(KeySpecError::IllegalOffset)
        );
        // `cannot indent end of line`（`:162-163`）：列号是 0 还要给 `.C`。
        assert_eq!(
            parse_key("1,0.5", &opts("")).err(),
            Some(KeySpecError::CannotIndentEndOfLine)
        );
        // `illegal field descriptor`（`:186-191`）：开始列停在非逗号的字符上。
        for spec in ["1 2", "1n2", "1.2.3", "1M"] {
            assert_eq!(
                parse_key(spec, &opts("")).err(),
                Some(KeySpecError::IllegalFieldDescriptor),
                "键形 {spec:?}"
            );
        }
        // `fields out of order`（`:213-217`）：三个条件全成立才报。
        assert_eq!(
            parse_key("2.5,1.2", &opts("")).err(),
            Some(KeySpecError::FieldsOutOfOrder)
        );
        // 缺一个就不报：结束列没给 `.C`（缩进为 0，源码注释写着「i.e. okay」），
        // 以及结束列号比开始列大的那种正常接法。
        assert!(parse_key("2,1", &opts("")).is_ok());
        assert!(parse_key("1,2.1", &opts("")).is_ok());
        // 也接受 `strtol` 的宽容形：前导空白、正号、多余的前导零。
        for spec in ["+1", " 1", "01"] {
            assert_eq!(
                parse_key(spec, &opts("")).unwrap().start_field,
                1,
                "{spec:?}"
            );
        }
    }

    /// `-l` 排的是「跨度还有几个字节」（C `fields.c:length()`：先 `SKIP_BLANKS(line)`，
    /// 再把 `lineend - line` 写成十进制串交给 `number()`）。下面四个读数直接拿参照件
    /// `sort_ref` 量过：空行（0）、`b 2`（1）、`cc 3`（2）、`aaa 1`（3）。
    /// 量的是长度而不是内容，所以 `b 2` 反而排在 `aaa 1` 之前（字典序倒过来）。
    #[test]
    fn sort_by_length_orders_by_span_length_not_content() {
        let by_length = opts("-l");
        assert_eq!(compare("b 2", "aaa 1", &by_length), Ordering::Less);
        assert_eq!(compare("cc 3", "aaa 1", &by_length), Ordering::Less);
        assert_eq!(compare("", "b 2", &by_length), Ordering::Less);
        // 等长时平在本键上，再回退到整行比较（那是本树的既有形状；C 的默认是并列键保持
        // 输入序，那一处差异已登记在批次二百八十七条的「登记不执行」第②条）。
        assert_eq!(compare("ab 1", "cd 2", &by_length), Ordering::Less);
        // 全局 `-l` 与局部 `-k 1l` 同序（`-l` 走的是 `fldtab[0].flags`，`sort.c:168-172`）。
        assert_eq!(compare("b 2", "aaa 1", &opts("-k 1l")), Ordering::Less);
        // 跳前导空白只发生在本键那一支里（`SKIP_BLANKS`），所以 `-t:` 下「两个补白加 aa」
        // 的长度是 2 而不是 4——参照件给 `b:2`、`  aa:1`、`ccc:3` 这个序。
        let delimited = opts("-t: -k 1l");
        assert_eq!(compare("  aa:1", "b:2", &delimited), Ordering::Greater);
        assert_eq!(compare("  aa:1", "ccc:3", &delimited), Ordering::Less);
    }

    /// `l` 与 `n` 同时存在时按长度走：C 在 `enterfield` 里先查 `L` 再查 `N`
    /// （`fields.c:208-211`），参照件量出 `-k 1ln` 与 `-k 1nl` 的读数与 `-k 1l` 完全相同
    ///（而不是数值序：`aaa` 与 `cc` 都没有数字前缀，走数值两者都是 0 而平局）。
    #[test]
    fn the_length_flag_wins_over_numeric() {
        for spec in ["1ln", "1nl"] {
            let key = parse_key(spec, &opts("")).unwrap();
            assert!(key.by_length && key.numeric, "{spec} 两位都该点亮");
            assert_eq!(
                compare("cc 3", "aaa 1", &opts(&format!("-k {spec}"))),
                Ordering::Less,
                "{spec}：长度序优先，不是数值序也不是字典序"
            );
        }
    }

    /// `init.c:193-194` 的继承判据是「局部一个字母也没有」才整体拿全局标志，不是逐位或。
    /// 这一形是参照件量出来的：`-n -k 2r` 与 `-k 2r` 同序（按字典序反排），而不是
    /// `-n -k 2` 的反序（那才是把 `n` 当全局位或进来的写法）。
    #[test]
    fn a_local_modifier_switches_the_global_flags_off() {
        // 全局 `-n` 且局部无字母：继承，2 排在 10 前。
        assert_eq!(compare("x 2", "y 10", &opts("-n -k 2")), Ordering::Less);
        // 全局 `-n` 但局部有 `r`：这一支的 **键**不继承 `n`，只按字典序的键反排——
        // 所以两条命令排出来是同一个序。比的是那一条键规格，不是整个 [`Options`]
        //（全局位本身当然还记在 Options 上，那里没有差异可查）。
        let with_global = parse_options(&["-n", "-k", "2r"]).unwrap().0;
        let without_global = parse_options(&["-k", "2r"]).unwrap().0;
        assert_eq!(
            with_global.keys, without_global.keys,
            "局部有字母时全局标志不得逐位渗进来"
        );
        assert!(!with_global.keys[0].numeric);
        assert!(with_global.numeric, "全局位本身仍照 C 存在 gflag 里");
        // 而「局部空」那一支确实拿到了全局：同一条 `-k 2` 带不带 `-n` 是两个序。
        assert!(parse_options(&["-n", "-k", "2"]).unwrap().0.keys[0].numeric);
    }

    #[test]
    fn test_local_reverse_drops_under_global() {
        // init.c:195-197: a local `r` does not invert a global `-r`; the
        // key parses without reversal.
        let global = opts("-r");
        let key = parse_key("1r", &global).unwrap();
        assert!(!key.reverse);
    }

    #[test]
    fn test_fields_split_on_blank_runs() {
        let global = opts("");
        let spec = parse_key("2,2", &global).unwrap();
        let mut options = opts("");
        options.keys.clear();
        options.keys.push(spec);
        assert_eq!(compare("aa b cc", "xx y zz", &options), Ordering::Less);
        assert_eq!(compare("a y z", "b x w", &options), Ordering::Greater);
    }

    #[test]
    fn test_tie_falls_back_to_whole_line() {
        let global = opts("");
        let spec = parse_key("1,1.1", &global).unwrap();
        let mut options = opts("");
        options.keys.clear();
        options.keys.push(spec);
        // The one-byte key ties; the whole line breaks the tie.
        assert_eq!(compare("a123", "a456", &options), Ordering::Less);
    }

    #[test]
    fn test_parse_options_takes_the_output_path() {
        // `-o` takes its value inline or as the next word, and a later one replaces
        // the earlier, because C's `case 'o'` is a plain `outpath = optarg`
        // assignment (`sort.c:188-190`). Without the option the field stays `None`,
        // which is what distinguishes standard output from `-o ""`.
        let (options, operands) = parse_options(&["-o", "out.txt", "in.txt"]).unwrap();
        assert_eq!(options.output.as_deref(), Some("out.txt".as_bytes()));
        assert_eq!(operands, vec!["in.txt"]);
        let (inline, _) = parse_options(&["-oout", "in"]).unwrap();
        assert_eq!(inline.output.as_deref(), Some("out".as_bytes()));
        let (replaced, _) = parse_options(&["-oa", "-ob"]).unwrap();
        assert_eq!(replaced.output.as_deref(), Some("b".as_bytes()));
        let (none, _) = parse_options(&["-n"]).unwrap();
        assert_eq!(none.output, None);
        assert_eq!(
            parse_options(&["-o"]),
            Err(ParseError::Getopt(crate::GetoptReport::MissingValue('o')))
        );
    }

    /// 三条出口各自一个型，而且只有两条前面带 libc 那一行（壳根据型选行数）。
    /// 集外字母那一支另有一枚测（`test_parse_options_rejects_unknown`），不在此重复。
    #[test]
    fn test_parse_options_splits_the_three_error_paths() {
        use crate::GetoptReport;
        // 缺值：与集外字母同属 `Getopt`，但句子不同。
        assert_eq!(
            parse_options(&["-t"]),
            Err(ParseError::Getopt(GetoptReport::MissingValue('t')))
        );
        // 键形不对：C 是 `errx(2, …)` 五句之一，不接 usage。
        assert_eq!(
            parse_options(&["-k", "x"]),
            Err(ParseError::KeySpec(KeySpecError::FieldNumbersNotPositive))
        );
        assert_eq!(
            parse_options(&["-k", "1 2"]),
            Err(ParseError::KeySpec(KeySpecError::IllegalFieldDescriptor))
        );
        // `-t` 给第二次才是 `usage("multiple field delimiters")`；
        // 而多字节本身**不是**错（C 只取首字节）。
        let (one, _) = parse_options(&["-t,", "f"]).unwrap();
        assert_eq!(one.delimiter, Some(b','));
        let (two, _) = parse_options(&["-t::", "f"]).unwrap();
        assert_eq!(two.delimiter, Some(b':'), "多字节取首字节，不报错");
        assert_eq!(
            parse_options(&["-t,", "-t;"]).err(),
            Some(ParseError::MultipleFieldDelimiters)
        );
        // 空值取到那个 `\0`（C 的 `*optarg`）。
        let (nul, _) = parse_options(&["-t", "", "f"]).unwrap();
        assert_eq!(nul.delimiter, Some(0));
    }

    #[test]
    fn test_output_target_follows_what_lstat_said() {
        // The three answers of `sort.c:320-355`, and the fourth row that keeps
        // `-o ""` distinct from no option at all: an empty path is still a path, so
        // it goes to the direct branch and the open fails there.
        assert_eq!(decide_output(None, None), Output::StandardOutput);
        assert_eq!(
            decide_output(Some(b"new"), None),
            Output::Direct {
                path: b"new".to_vec()
            }
        );
        assert_eq!(
            decide_output(Some(b""), None),
            Output::Direct { path: Vec::new() }
        );
        assert_eq!(
            decide_output(
                Some(b"dev"),
                Some(OutputProbe {
                    mode: 0o666,
                    is_device: true
                }),
            ),
            Output::Direct {
                path: b"dev".to_vec()
            }
        );
        assert_eq!(
            decide_output(
                Some(b"old"),
                Some(OutputProbe {
                    mode: 0o640,
                    is_device: false
                }),
            ),
            Output::ViaTemp {
                path: b"old".to_vec(),
                mode: 0o640
            }
        );
    }

    #[test]
    fn test_temporary_names_keep_the_prefix_and_walk_forward() {
        // C writes `%sXXXXXX` and lets `mkstemp` fill the six slots from a
        // sixty-two-character alphabet (`gettemp.c:49-50`), carrying forward while a
        // create collides. What the caller relies on is the prefix, the length, and
        // that consecutive attempts differ.
        let first = temp_candidate(b"out.txt", 0x1234, 0);
        let second = temp_candidate(b"out.txt", 0x1234, 1);
        assert_eq!(first.len(), b"out.txt".len() + 6);
        assert!(first.starts_with(b"out.txt"));
        assert_ne!(first, second, "consecutive attempts must differ");
        for byte in &first[b"out.txt".len()..] {
            assert!(
                b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".contains(byte),
                "suffix byte {byte:#04x} is outside C's padchar set"
            );
        }
    }

    #[test]
    fn test_parse_options_rejects_unknown() {
        assert_eq!(
            parse_options(&["-Z"]),
            Err(ParseError::Getopt(crate::GetoptReport::UnknownOption('Z')))
        );
    }

    #[test]
    fn test_parse_options_accepts_merge() {
        let (options, operands) = parse_options(&["-m", "a.txt"]).unwrap();
        assert!(options.merge);
        assert_eq!(operands, ["a.txt"]);
    }

    #[test]
    fn test_parse_options_accepts_h_and_s_as_noops() {
        // C accepts `-H` ("now the default") and `-s` (stable = default) with no
        // observable effect; they must parse cleanly (not BadOption) and change
        // no flag — the previous behavior errored them, diverging from C.
        let (options, operands) = parse_options(&["-H", "-s", "data.txt"]).unwrap();
        assert!(!options.merge);
        assert!(!options.unique);
        assert!(!options.check);
        assert!(!options.numeric);
        assert_eq!(operands, ["data.txt"]);
        // Clustered in one argument too (getopt treats each letter separately).
        let (clustered, _) = parse_options(&["-sH"]).unwrap();
        assert!(!clustered.numeric);
    }

    #[test]
    fn test_merge_sorted_two_runs_interleave() {
        let options = Options::default();
        let a = ["a1".to_string(), "a3".to_string(), "a5".to_string()];
        let b = ["a2".to_string(), "a4".to_string()];
        let merged = merge_sorted(&[&a, &b], &options);
        assert_eq!(
            merged,
            ["a1", "a2", "a3", "a4", "a5"].map(String::from).to_vec()
        );
    }

    #[test]
    fn test_merge_sorted_equal_keys_keep_earlier_run_first() {
        let options = Options::default();
        let a = ["x".to_string(), "y".to_string()];
        let b = ["x".to_string()];
        let merged = merge_sorted(&[&a, &b], &options);
        // run a's "x" precedes run b's "x": a stable, left-biased merge.
        assert_eq!(merged, ["x", "x", "y"].map(String::from).to_vec());
    }
}

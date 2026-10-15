//! `join` deciding half: merge two field-structured inputs on a key field.
//!
//! Ground truth: `minix3/usr.bin/join/join.c` — inputs are read grouped by
//! their (equal) join field (`slurp`, lines 300-403), the two files are
//! merged key by key (`cmp`/`joinlines`, lines 405-450), and each matched pair
//! prints the join field then the remaining fields of file 1 then of file 2
//! (`outtwoline` default branch, lines 476-489); unmatched lines print all
//! their own fields when `-a` asks (`outoneline`, lines 447-466). Fields split
//! on runs of `tabchar` with empties skipped (`spans`, default on).
//!
//! Wired here: the default join (field 1 both sides), `-t` (delimiter +
//! output separator = that byte), `-1`/`-2` (per-file join field), `-j` (both
//! join fields at once — the CLI alias for `-1 N -2 N`), `-a` (unmatched
//! lines), `-v` (report only unpairables from one file, suppressing matched
//! lines via the C `joinout` global), `-o` (output-field list) and `-e`
//! (empty replacement). Declared residual: the obsolete one-argument form (a
//! bare `-1`/`-2`/`-o`/`-t` without a value) — this module models each option
//! independently, so the caller supplies the values it does not set as defaults.
//!
//! Pure: takes the two inputs as byte-line slices and returns the output
//! lines, so the merge is unit-tested without files or the IPC layer.

use crate::GetoptReport;
use alloc::vec::Vec;

/// Which side an output-field entry draws from (the `F` of `join -o F.N`),
/// or the join field itself (`0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// The join field (always taken from file 1's key column).
    Join,
    /// A field of file 1.
    One,
    /// A field of file 2.
    Two,
}

/// One `join -o` output field: a `Which` and a 0-based field index (unused
/// for `Join`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OField {
    pub which: Which,
    pub idx: usize,
}

/// Join options, mirroring the C globals `join` carries per run.
#[derive(Debug, Clone)]
pub struct JoinOptions {
    /// Field index (0-based) that keys file 1 (`-1`, minus one).
    pub field1: usize,
    /// Field index that keys file 2 (`-2`, minus one).
    pub field2: usize,
    /// Delimiter byte: both the split character and the output separator
    /// (`-t`; the C default is `" \t"`, whose first byte — a space — also
    /// separates output). Modelled as one byte; the default is space with
    /// tabs treated as separators too via `collapse`.
    pub delim: u8,
    /// Emit unpaired lines from file 1 (`-a 1`).
    pub unmatched1: bool,
    /// Emit unpaired lines from file 2 (`-a 2`).
    pub unmatched2: bool,
    /// Explicit output field list (`-o`); empty selects the default layout
    /// (join field, then file 1's rest, then file 2's rest).
    pub output_fields: Vec<OField>,
    /// Replacement for a missing/unpaired field (`-e`); `None` leaves it blank.
    pub empty: Option<Vec<u8>>,
    /// Emit the matched-line output. `-v` clears it so only the unpaired lines
    /// from the specified file print (C `joinout` global, `join.c:104,207-208`).
    pub joinout: bool,
}

impl Default for JoinOptions {
    fn default() -> Self {
        JoinOptions {
            field1: 0,
            field2: 0,
            delim: b' ',
            unmatched1: false,
            unmatched2: false,
            output_fields: Vec::new(),
            empty: None,
            joinout: true,
        }
    }
}

/// Parse a `join -o` field list (`"1.2,2.3,0"`) into entries; tokens are
/// comma/space/tab separated (the C `strsep(", \t")`), `0` is the join field,
/// `F.N` (1-based) a field of file `F`; the two refusal shapes are kept apart
/// because C's `fieldarg` prints a different sentence for each (see [`OListError`]).
pub fn parse_olist(spec: &str) -> Result<Vec<OField>, OListError> {
    let mut fields = Vec::new();
    for token in spec.split([',', ' ', '\t']) {
        if token.is_empty() {
            continue;
        }
        if token == "0" {
            fields.push(OField {
                which: Which::Join,
                idx: 0,
            });
            continue;
        }
        // C `fieldarg` 的两支 `errx` 是不同的句子，所以下面两道守卫不能合：
        // 前缀不是 `1.`/`2.` 或有尾字符走「malformed -o option field」，
        // 形状对但字段号是 0 走「field numbers are 1 based」（`join.c:526-532`）。
        if !matches!(token.as_bytes().first(), Some(b'1' | b'2'))
            || token.as_bytes().get(1) != Some(&b'.')
        {
            return Err(OListError::Malformed);
        }
        let num = &token[2..];
        let (value, consumed) = strtol_decimal(num);
        if consumed != num.len() {
            // C 的 `if (*end)` 在前：尾字符一律算 malformed。
            return Err(OListError::Malformed);
        }
        if value == 0 {
            return Err(OListError::OneBased);
        }
        // 负数与超宽值在本树拒掉：C 把 `strtol` 的结果赋给 `u_long fieldno`，
        // 所以 `-o 1.-1` 在 C 里反而能过（回绕成巨值）。这一形要照搬得先拿输出级参照，
        // 而探针件在走到输出前就崩（已在 18 篇台账登记），故先记为声明差而不静默同 C。
        if value < 0 || value > i64::from(u32::MAX) {
            return Err(OListError::Malformed);
        }
        let which = if token.as_bytes()[0] == b'1' {
            Which::One
        } else {
            Which::Two
        };
        fields.push(OField {
            which,
            idx: (value as u32 - 1) as usize,
        });
    }
    Ok(fields)
}

/// `-o` 字段的两种失手，逐字对位 C `fieldarg` 的两支 `errx`（两句不同、都不接 usage）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OListError {
    /// 前缀不是 `1.`/`2.`，或字段号带尾字符（`join.c:527-530`）。
    Malformed,
    /// 形状对但字段号是 0（`join.c:531-532`）。
    OneBased,
}

/// C 的 `strtol(optarg, &end, 10)` 在本命令实际需要的那一段：先跳前导空白（空格、
/// `\t`、`\n`、垂直跳格 `\x0b`、换页 `\x0c`、回车——与 `isspace` 同集），再可带一个符号，
/// 再吃十进制位。**一个数字也没吃到时值为 0、
/// `end` 回到开头**——这个形就是 `-1 x` 落「less than 1」而不是「illegal field number」的原因
/// （参照件同读数，`join.c:140-147`）。返回（值，吃掉的字节数），调用方用
/// 「吃掉数 < 长度」等价于 C 的 `*end != '\0'`。饱和行为不在本支射程（字段号远小于
/// `LONG_MAX`，C 饱和后仍然 `>= 1`，这里钳住不致溢出）。
#[must_use]
fn strtol_decimal(text: &str) -> (i64, usize) {
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while matches!(
        bytes.get(at),
        Some(b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
    ) {
        at += 1;
    }
    let negative = match bytes.get(at) {
        Some(b'+') => {
            at += 1;
            false
        }
        Some(b'-') => {
            at += 1;
            true
        }
        _ => false,
    };
    let digits_start = at;
    let mut value: i64 = 0;
    while matches!(bytes.get(at), Some(d) if d.is_ascii_digit()) {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(bytes[at] - b'0'));
        at += 1;
    }
    if at == digits_start {
        // 无数字：值 0、`end` 停在符号/空白后的位置——与 C 同形（符号算已消费）。
        return (0, at);
    }
    (if negative { -value } else { value }, at)
}

/// `join` 的 argv 尾部解析结果：选项 + 两个文件操数。
#[derive(Debug, Clone)]
pub struct JoinArgs<'a> {
    pub opts: JoinOptions,
    pub files: Vec<&'a str>,
}

/// `join` 参数解析失败。每个变体对应 C `join.c` 的一支，**带该支自己的选项字母与被拒的值**：
/// C 的句子把字母写在前面（`"-1 option field number less than 1"`）或把 `optarg` 拓在
/// `-- ` 后面（`"illegal field number -- %s"`），所以这两样东西必须在决定半就交回来，
/// 否则壳只能拼出一句通用话（旧状：`join: option field number less than 1`，丢了字母与值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError<'a> {
    /// 只有非两个操数走这一支（`join.c:236-237` 的 `if (argc != 2) usage();`）：
    /// 选项环已经跑完，所以前面**没有** libc 那一行。
    Usage,
    /// 表外字母与带值选项缺值：libc 先印一行，再是同样的两行 usage。
    Getopt(crate::GetoptReport),
    /// `-1`/`-2`/`-j`：`strtol` 值 < 1（C `join.c:141`/`:152`/`:186`）。字段是 C 句子里的那个字母串。
    FieldLessThan1(&'static str),
    /// 同上三支但 `*end` 非零（`:145`/`:156`/`:190`）：C 把 `optarg` 整个跟在 `-- ` 后，
    /// 而**这一支的句子不写选项字母**（所以下面只带值，不带用不上的字母）。
    IllegalFieldNumber(&'a str),
    /// `-a`/`-v` 的文件号不是 1/2（`:171`/`:217`）。
    FileNumberNot12(&'static str),
    /// `-a`/`-v` 的 `*end` 非零（`:176`/`:222`）；与上一支同形，C 也不写选项字母。
    IllegalFileNumber(&'a str),
    /// `-t` 的串不是单字节（`:202`）——这一支 C 不写选项字母。
    BadTabChar,
    /// `-o` 的字段形状不对（`fieldarg` 第一支 `errx`）：不接 usage。
    MalformedOutputField,
    /// `-o` 的字段号是 0（`fieldarg` 第二支 `errx`，另一句话）：不接 usage。
    FieldNumbersAre1Based,
    /// `-a` 与 `-v` 同时给（`:235` 的 `errx`）：印完就走，没有 usage。
    AAndVMutuallyExclusive,
    /// legacy `-j` 的畸形写法：`obsolete()` 在 `getopt` **之前**就把整条 argv 扫过一遍，
    /// `-j` 后面不是 `1`/`2`、或跟着数字却不止一位，都当场
    /// `errx(1, "illegal option -- %s", ap)`（`join.c:592` 的 `jbad`），句子带的是**整个
    /// token**（含前导短线）。参照件读数：`join -j3 f1 f2` → `illegal option -- -j3`。
    /// 因为它在选项环之前，所以 `-j3` 后面还藏着别的错也轮不到先报。
    IllegalOptionToken(&'a str),
}

/// legacy `-o arg arg` 里那些「要被重写成 `-o<该项>`」的字段规格，判据逐字对位
/// `join.c:604-609`：首字符是 `1` 或 `2`、次字符是 `.`、其后至少一位且全是十进制位
/// （C 用 `len - 2 != strspn(p + 2, "0123456789")` 表达「其余至少一位且全是数字」——
/// 门槛是 `>= 3` 而不是 `>= 4`：`1.1` 这种一项数字的写法在 C 确实会被改写）。
fn is_field_spec(word: &str) -> bool {
    let b = word.as_bytes();
    b.len() >= 3 && matches!(b[0], b'1' | b'2') && b[1] == b'.' && b[2..].iter().all(u8::is_ascii_digit)
}

/// 把 `join` 的 argv 尾部按 BSD `getopt("\01a:e:j:1:2:o:t:v:")` 拆成 [`JoinArgs`]。
/// 除选项字母外的剩余部分（`-t,` 的 `,`）或是附值或是紧跟的下一项，都是选项值——
/// `join.c:133` 的 `getopt` 对带 `:` 的选项接受两种形。旧前门 `match flag{…}`
/// 只比整个 `a[1..]`，故 `join -t, f1 f2`、`-a1`、`-o0,1` 这些 C 合法的附值形
/// 会误落「unknown option」。`--` 结束选项扫描（BSD getopt），裸 `-` 与非连字符
/// 项都是文件操数。C 的 `obsolete(argv)`（`:132`）对裸 `-a`、`-j[12]` 等 legacy 写法
/// 重写 argv 为 `\01`/`-1`/`-2`，本层不实现该 legacy 重写（登记为余面）。
pub fn parse_join_args<'a>(args: &'a [&'a str]) -> Result<JoinArgs<'a>, JoinError<'a>> {
    let mut opts = JoinOptions::default();
    let mut files: Vec<&'a str> = Vec::new();
    let (mut aflag, mut vflag) = (false, false);
    // `obsolete()` 的畸形 `-j` 形在 `getopt` 之前就 `errx`（`join.c:570-595`）：`-j` 后面
    // 既不是 `1`/`-2` 也不是空，或者虽是其二却还拖着别的字符（`-j12`、`-j1x`、`-j3`），
    // 都当场报「illegal option -- <整个 token>」。这一遍只判错、不改写（改写在各支里做）。
    {
        let mut k = 0;
        while k < args.len() {
            let tok = args[k];
            if tok == "--" {
                break;
            }
            let tb = tok.as_bytes();
            if tb.len() >= 2 && tb[0] == b'-' && tb[1] == b'j' {
                let bad = match tb.len() {
                    2 => false,
                    3 => !matches!(tb[2], b'1' | b'2'),
                    _ => true,
                };
                if bad {
                    return Err(JoinError::IllegalOptionToken(tok));
                }
            }
            k += 1;
        }
    }
    let mut i = 0;
    // 取一个选项的值：附在字母后（`&arg[2..]`）或紧跟的下一项。缺值时要把**那个字母**
    // 交出去：C 那一支先由 libc 印 `"<prog>: option requires an argument -- <字母>"`
    // （`getopt.c:132-136`），然后才轮到 `usage()`。
    macro_rules! value {
        ($letter:expr) => {{
            let cur = args[i];
            if cur.len() > 2 {
                &cur[2..]
            } else {
                i += 1;
                if i >= args.len() {
                    return Err(JoinError::Getopt(GetoptReport::MissingValue($letter)));
                }
                args[i]
            }
        }};
    }
    // 一个必为十进制且 >=1 的字段号。判序逐字对位 C：先 `strtol` 取值、**先查 `< 1`**、
    // 再查尾字符（`join.c:140-147`/`:151-158`/`:184-191`）。这个顺序有可观察后果：
    // `x` 一个数字也转不出→值为 0→落「less than 1」，而不是「illegal field number」
    // （旧实现按「是不是全数字」分流，把 `x` 归错了支；参照件同读数钉住这一形）。
    fn field_no<'b>(v: &'b str, opt: &'static str) -> Result<usize, JoinError<'b>> {
        let (value, consumed) = strtol_decimal(v);
        if value < 1 {
            return Err(JoinError::FieldLessThan1(opt));
        }
        if consumed != v.len() {
            return Err(JoinError::IllegalFieldNumber(v));
        }
        Ok(value as usize - 1)
    }
    // 一个只能是 1/2 的文件号。C 也是两道守卫且**先 switch 后查尾字符**
    // （`:165-179`/`:209-224`），所以 `1x` 落「illegal file number -- 1x」（switch 先认了 1），
    // 而 `x`、`0`、`3` 落「not 1 or 2」（旧实现把 `1x` 归进了后一支）。
    fn file_no<'b>(
        v: &'b str,
        opt: &'static str,
        unpair1: &mut bool,
        unpair2: &mut bool,
    ) -> Result<(), JoinError<'b>> {
        let (value, consumed) = strtol_decimal(v);
        match value {
            1 => *unpair1 = true,
            2 => *unpair2 = true,
            _ => return Err(JoinError::FileNumberNot12(opt)),
        }
        if consumed != v.len() {
            return Err(JoinError::IllegalFileNumber(v));
        }
        Ok(())
    }
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        }
        let b = a.as_bytes();
        if b.len() < 2 || b[0] != b'-' {
            files.push(a);
            i += 1;
            continue;
        }
        // `join.c:obsolete()`（`:547`）的三段 legacy 重写在这里落地。它跑在 `getopt`
        // **之前**、遇 `--` 即停，所以畸形形要先报错、改写形要按改写后的样子派发。
        // legacy `-j1`/`-j2`：C 把 token 就地改成 `-1`/`-2` 再交给 getopt，于是值取**下一词**
        // （inline 那位是字母的一部分，不是值）。参照件坐实：`join -j1 f1 f2` 报的是
        // `-1 option field number less than 1`——下一词是文件名，一个数字也转不出来。
        if b.len() == 3 && b[1] == b'j' && matches!(b[2], b'1' | b'2') {
            i += 1;
            if i >= args.len() {
                return Err(JoinError::Getopt(GetoptReport::MissingValue(b[2] as char)));
            }
            let v = args[i];
            if b[2] == b'1' {
                opts.field1 = field_no(v, "-1")?;
            } else {
                opts.field2 = field_no(v, "-2")?;
            }
            i += 1;
            continue;
        }
        // legacy 裸 `-a`（不带文件号）：C 把 `ap[1]` 写成 `\01`，落到 `case '\01'`
        // （`join.c:135-138`）——`aflag` 置起且**两个文件都**出不配对。本壳此前把 `-a`
        // 当成「带值选项」，于是 `join -a f1 f2` 会吞掉第一个文件名再按 `f1` 转数字，
        // 落「field number less than 1」——那是把 legacy 形当成新形读的实证错。
        if b.len() == 2 && b[1] == b'a' {
            aflag = true;
            opts.unmatched1 = true;
            opts.unmatched2 = true;
            i += 1;
            continue;
        }
        match b[1] {
            b'1' => opts.field1 = field_no(value!('1'), "-1")?,
            b'2' => opts.field2 = field_no(value!('2'), "-2")?,
            b't' => {
                let v = value!('t');
                let bytes = v.as_bytes();
                if bytes.len() != 1 {
                    return Err(JoinError::BadTabChar);
                }
                opts.delim = bytes[0];
            }
            b'a' => {
                aflag = true;
                file_no(
                    value!('a'),
                    "-a",
                    &mut opts.unmatched1,
                    &mut opts.unmatched2,
                )?;
            }
            b'v' => {
                vflag = true;
                opts.joinout = false;
                file_no(
                    value!('v'),
                    "-v",
                    &mut opts.unmatched1,
                    &mut opts.unmatched2,
                )?;
            }
            b'e' => opts.empty = Some(value!('e').as_bytes().to_vec()),
            b'j' => {
                let n = field_no(value!('j'), "-j")?;
                opts.field1 = n;
                opts.field2 = n;
            }
            b'o' => {
                // C 的 `-o arg arg` 改写只在 token **恰好是裸 `-o`** 时才发生
                //（`join.c:601-602` 的 `if (ap[2] != '\0') break;`），所以附值形 `-o1.1`
                // 后面的 `2.1` 仍是操作数。这个门槛要在取 value 之前记下来。
                let bare_o = a.len() == 2;
                // `fieldarg` 的两支都是 `errx`，但句子不同，所以不能拢一个错型上去。
                let push_list = |list: &'a str, acc: &mut Vec<OField>| -> Result<(), JoinError<'a>> {
                    match parse_olist(list) {
                        Ok(fields) => {
                            // C 的 `fieldarg` 是**往 olist 尾部追加**（`join.c:540-542` 的
                            // `olist[olistcnt] = ...; ++olistcnt`），所以重复 `-o` 累加而不是覆盖。
                            acc.extend(fields);
                            Ok(())
                        }
                        Err(OListError::Malformed) => Err(JoinError::MalformedOutputField),
                        Err(OListError::OneBased) => Err(JoinError::FieldNumbersAre1Based),
                    }
                };
                push_list(value!('o'), &mut opts.output_fields)?;
                // legacy `-o arg arg`：C 在 `obsolete()` 里把紧跟其后的每个 `[12].<数字>` 项
                // 逐个重写成 `-o<该项>`（`join.c:596-617`），所以它们**不再是操作数**而是各自
                // 一条 `-o` 列表；第一个不像字段规格的项就停止改写（后面的仍算操作数）。
                while bare_o && i + 1 < args.len() && is_field_spec(args[i + 1]) {
                    i += 1;
                    push_list(args[i], &mut opts.output_fields)?;
                }
            }
            // 表外字母：C 也先有一行 libc 报文（`getopt.c:101-104`），再落
            // `join.c:226-228` 的 `case '?'`/`default: usage()`。
            _ => {
                return Err(JoinError::Getopt(GetoptReport::UnknownOption(b[1] as char)));
            }
        }
        i += 1;
    }
    if aflag && vflag {
        return Err(JoinError::AAndVMutuallyExclusive);
    }
    if files.len() != 2 {
        return Err(JoinError::Usage);
    }
    Ok(JoinArgs { opts, files })
}

/// Split a line into fields on runs of space/tab (the C default `tabchar`
/// `" \t"` with `spans`), or on the single `-t` byte; empty runs are skipped.
fn split_fields(line: &[u8], delim: u8) -> Vec<&[u8]> {
    let mut fields = Vec::new();
    let mut start = 0usize;
    for i in 0..=line.len() {
        let is_delim = i < line.len() && (line[i] == delim || (delim == b' ' && line[i] == b'\t'));
        if is_delim {
            if i > start {
                fields.push(&line[start..i]);
            }
            start = i + 1;
        } else if i == line.len() && i > start {
            fields.push(&line[start..i]);
        }
    }
    fields
}

/// The key of a field vector at index `f` (empty when the line is too short).
fn key<'l>(fields: &[&'l [u8]], f: usize) -> &'l [u8] {
    fields.get(f).copied().unwrap_or(b"")
}

/// Join `f1` and `f2` (byte-line slices, each already newline-stripped) into
/// output lines. Inputs are assumed sorted by their join field (as `join(1)`
/// requires); equal keys cross-multiply, the lesser key advances alone.
pub fn join_lines(f1: &[&[u8]], f2: &[&[u8]], opts: &JoinOptions) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < f1.len() && j < f2.len() {
        let g1 = group_end(f1, i, opts.field1, opts.delim);
        let g2 = group_end(f2, j, opts.field2, opts.delim);
        let k1 = key(&split_fields(f1[i], opts.delim), opts.field1);
        let k2 = key(&split_fields(f2[j], opts.delim), opts.field2);
        match k1.cmp(k2) {
            core::cmp::Ordering::Equal => {
                // C's `if (joinout) joinlines(F1, F2)` (`join.c:259`): with `-v`
                // clearing `joinout`, matched groups still advance but emit
                // nothing.
                if opts.joinout {
                    for la in f1[i..g1].iter().copied() {
                        let fa = split_fields(la, opts.delim);
                        for lb in f2[j..g2].iter().copied() {
                            let fb = split_fields(lb, opts.delim);
                            out.push(twoline(&fa, &fb, opts));
                        }
                    }
                }
                i = g1;
                j = g2;
            }
            core::cmp::Ordering::Less => {
                if opts.unmatched1 {
                    for la in f1[i..g1].iter().copied() {
                        out.push(onlyline(&split_fields(la, opts.delim), true, opts));
                    }
                }
                i = g1;
            }
            core::cmp::Ordering::Greater => {
                if opts.unmatched2 {
                    for lb in f2[j..g2].iter().copied() {
                        out.push(onlyline(&split_fields(lb, opts.delim), false, opts));
                    }
                }
                j = g2;
            }
        }
    }
    // A file may have trailing groups past the other's end.
    if opts.unmatched1 {
        while i < f1.len() {
            let g = group_end(f1, i, opts.field1, opts.delim);
            for la in f1[i..g].iter().copied() {
                out.push(onlyline(&split_fields(la, opts.delim), true, opts));
            }
            i = g;
        }
    }
    if opts.unmatched2 {
        while j < f2.len() {
            let g = group_end(f2, j, opts.field2, opts.delim);
            for lb in f2[j..g].iter().copied() {
                out.push(onlyline(&split_fields(lb, opts.delim), false, opts));
            }
            j = g;
        }
    }
    out
}

/// Index one past the run of lines from `start` sharing the key of `start`.
fn group_end(lines: &[&[u8]], start: usize, field: usize, delim: u8) -> usize {
    let base = key(&split_fields(lines[start], delim), field).to_vec();
    let mut end = start + 1;
    while end < lines.len() && key(&split_fields(lines[end], delim), field).to_vec() == base {
        end += 1;
    }
    end
}

/// Append a non-empty field to an in-progress output line, inserting the
/// separator before all but the first field.
fn push_tok(out: &mut Vec<u8>, sep: u8, first: &mut bool, tok: &[u8]) {
    if tok.is_empty() {
        return;
    }
    if !*first {
        out.push(sep);
    }
    out.extend_from_slice(tok);
    *first = false;
}

/// Pick the token for one `join -o` entry given the (possibly absent) field
/// vectors of each file; a missing field or the absent side yields the `-e`
/// replacement (empty string when none was given).
fn olist_token(
    entry: &OField,
    a: Option<&[&[u8]]>,
    b: Option<&[&[u8]]>,
    opts: &JoinOptions,
) -> Vec<u8> {
    let fallback = opts.empty.clone().unwrap_or_default();
    let pick = |side: Option<&[&[u8]]>, idx: usize| -> Vec<u8> {
        match side {
            Some(fields) => fields
                .get(idx)
                .map(|f| f.to_vec())
                .unwrap_or_else(|| fallback.clone()),
            None => fallback.clone(),
        }
    };
    match entry.which {
        // The join field comes from file 1's key column when present, else
        // file 2's (the unmatched file-1-only case still has its own key).
        Which::Join => a
            .and_then(|f| f.get(opts.field1))
            .or_else(|| b.and_then(|f| f.get(opts.field2)))
            .map(|f| f.to_vec())
            .unwrap_or_default(),
        Which::One => pick(a, entry.idx),
        Which::Two => pick(b, entry.idx),
    }
}

/// The pair output line: the `-o` field list when set (each entry resolved
/// against both files), else the default join field + file 1 rest + file 2 rest.
fn twoline(a: &[&[u8]], b: &[&[u8]], opts: &JoinOptions) -> Vec<u8> {
    let mut line = Vec::new();
    let mut first = true;
    if !opts.output_fields.is_empty() {
        for entry in &opts.output_fields {
            push_tok(
                &mut line,
                opts.delim,
                &mut first,
                &olist_token(entry, Some(a), Some(b), opts),
            );
        }
        return line;
    }
    if a.len() > opts.field1 {
        push_tok(&mut line, opts.delim, &mut first, a[opts.field1]);
    }
    for (idx, f) in a.iter().enumerate() {
        if idx != opts.field1 {
            push_tok(&mut line, opts.delim, &mut first, f);
        }
    }
    for (idx, f) in b.iter().enumerate() {
        if idx != opts.field2 {
            push_tok(&mut line, opts.delim, &mut first, f);
        }
    }
    line
}

/// An unpaired line: with `-o` the other file's fields yield the `-e`
/// replacement; otherwise all of the line's own fields, joined by `delim`.
fn onlyline(fields: &[&[u8]], is_file1: bool, opts: &JoinOptions) -> Vec<u8> {
    let mut line = Vec::new();
    let mut first = true;
    if !opts.output_fields.is_empty() {
        let a: Option<&[&[u8]]> = if is_file1 { Some(fields) } else { None };
        let b: Option<&[&[u8]]> = if is_file1 { None } else { Some(fields) };
        for entry in &opts.output_fields {
            push_tok(
                &mut line,
                opts.delim,
                &mut first,
                &olist_token(entry, a, b, opts),
            );
        }
        return line;
    }
    for f in fields {
        if f.is_empty() {
            continue;
        }
        if !first {
            line.push(opts.delim);
        }
        line.extend_from_slice(f);
        first = false;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<Vec<u8>> {
        s.lines().map(|l| l.as_bytes().to_vec()).collect()
    }

    fn refs(v: &[Vec<u8>]) -> Vec<&[u8]> {
        v.iter().map(|l| l.as_slice()).collect()
    }

    fn join(f1: &str, f2: &str, opts: &JoinOptions) -> String {
        let (a, b) = (lines(f1), lines(f2));
        let out = join_lines(&refs(&a), &refs(&b), opts);
        out.iter()
            .map(|l| core::str::from_utf8(l).unwrap().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_default_inner_join() {
        let f1 = "1 a\n2 b\n3 c";
        let f2 = "1 x\n2 y\n4 z";
        assert_eq!(join(f1, f2, &JoinOptions::default()), "1 a x\n2 b y");
    }

    #[test]
    fn test_unmatched_a_flags() {
        let f1 = "1 a\n3 c";
        let f2 = "1 x\n4 z";
        let o = JoinOptions {
            unmatched1: true,
            ..Default::default()
        };
        // -a 1: file 1's unpaired key 3 survives; file 2's key 4 dropped.
        assert_eq!(join(f1, f2, &o), "1 a x\n3 c");
        let o2 = JoinOptions {
            unmatched1: true,
            unmatched2: true,
            ..Default::default()
        };
        assert_eq!(join(f1, f2, &o2), "1 a x\n3 c\n4 z");
    }

    #[test]
    fn test_equal_keys_cross_multiply() {
        // Multiple lines share a key on both sides -> full cross product.
        let f1 = "1 a\n1 b";
        let f2 = "1 x\n1 y";
        assert_eq!(
            join(f1, f2, &JoinOptions::default()),
            "1 a x\n1 a y\n1 b x\n1 b y"
        );
    }

    #[test]
    fn test_custom_join_fields() {
        // -1 2 -2 1: join file1's field 2 against file2's field 1.
        let f1 = "x A\ny B";
        let f2 = "A 1\nB 2";
        let o = JoinOptions {
            field1: 1,
            field2: 0,
            ..Default::default()
        };
        // key is file1's field2; output = key then file1-rest then file2-rest.
        assert_eq!(join(f1, f2, &o), "A x 1\nB y 2");
    }

    #[test]
    fn test_dash_t_delimiter() {
        let f1 = "1:a\n2:b";
        let f2 = "1:x\n3:z";
        let o = JoinOptions {
            delim: b':',
            ..Default::default()
        };
        // split and join both use ':'; unmatched file lines dropped (no -a).
        assert_eq!(join(f1, f2, &o), "1:a:x");
    }

    #[test]
    fn test_multifield_lines() {
        let f1 = "1 a b\n2 c d";
        let f2 = "1 x\n2 y";
        assert_eq!(join(f1, f2, &JoinOptions::default()), "1 a b x\n2 c d y");
    }

    #[test]
    fn test_parse_olist_forms() {
        assert_eq!(
            parse_olist("1.2,2.1,0").unwrap(),
            vec![
                OField {
                    which: Which::One,
                    idx: 1
                },
                OField {
                    which: Which::Two,
                    idx: 0
                },
                OField {
                    which: Which::Join,
                    idx: 0
                },
            ]
        );
        assert!(parse_olist("3.1").is_err()); // no file 3
        assert!(parse_olist("1.0").is_err()); // fields are 1-based
        assert!(parse_olist("5").is_err()); // no dot, not `0`
    }

    #[test]
    fn test_olist_selects_fields_and_join() {
        // -o 0,2.2,1.2 : join field, file2's field2, file1's field2.
        let o = JoinOptions {
            output_fields: parse_olist("0,2.2,1.2").unwrap(),
            ..Default::default()
        };
        assert_eq!(join("1 a b\n2 c d", "1 x\n2 y", &o), "1 x a\n2 y c");
    }

    #[test]
    fn test_olist_empty_replacement_on_unmatched() {
        // -a 1 -o 0,1.2,2.2 -e '-' : file-1-only line has no file-2 field.
        let mut o = JoinOptions {
            output_fields: parse_olist("0,1.2,2.2").unwrap(),
            empty: Some(b"-".to_vec()),
            unmatched1: true,
            ..Default::default()
        };
        o.unmatched2 = false;
        // key 3 pairs with nothing -> file-2 field replaced by '-'.
        assert_eq!(join("1 a\n3 c", "1 x", &o), "1 a x\n3 c -");
    }

    #[test]
    fn test_v1_emits_only_file1_unpaired() {
        // `-v 1` (joinout off, unpair file 1): the key 1 match prints nothing,
        // key 3 alone from file 1 prints, file 2's key 4 stays silent.
        let o = JoinOptions {
            joinout: false,
            unmatched1: true,
            ..Default::default()
        };
        assert_eq!(join("1 a\n3 c", "1 x\n4 z", &o), "3 c");
    }

    #[test]
    fn test_v2_emits_only_file2_unpaired() {
        let o = JoinOptions {
            joinout: false,
            unmatched2: true,
            ..Default::default()
        };
        assert_eq!(join("1 a\n3 c", "1 x\n4 z", &o), "4 z");
    }

    #[test]
    fn test_joinout_off_without_unpair_emits_nothing() {
        // Defensive: `-v` with no matching unpair file yields empty output —
        // matched groups are silent and no side emits singletons.
        let o = JoinOptions {
            joinout: false,
            ..Default::default()
        };
        assert_eq!(join("1 a\n2 b", "1 x\n3 z", &o), "");
    }

    fn parse<'a>(v: &'a [&'a str]) -> JoinArgs<'a> {
        parse_join_args(v).expect("parse ok")
    }

    #[test]
    fn parse_attached_option_values_accepted() {
        // 关键回归：BSD getopt 的附值形不被旧前门接受。
        let a = parse(&["-t,", "f1", "f2"]);
        assert_eq!(a.opts.delim, b',');
        assert_eq!(a.files, vec!["f1", "f2"]);
        let b = parse(&["-a1", "-o0,1.2", "f1", "f2"]);
        assert!(b.opts.unmatched1 && !b.opts.unmatched2);
        assert_eq!(b.opts.output_fields.len(), 2);
        // 分离值仍正确（向后兼容）。
        let c = parse(&["-t", "|", "-1", "2", "f1", "f2"]);
        assert_eq!(c.opts.delim, b'|');
        assert_eq!(c.opts.field1, 1);
    }

    #[test]
    fn parse_terminator_and_operands() {
        // `--` 后全为操数；裸 `-` 是操数（stdin）。
        let a = parse(&["-t,", "--", "-a", "f2"]);
        assert_eq!(a.files, vec!["-a", "f2"]); // `-a` 在 `--` 后是文件名的字串
        let b = parse(&["-", "f2"]);
        assert_eq!(b.files, vec!["-", "f2"]);
    }

    /// `join.c:obsolete()` 的三段 legacy 重写（跑在 `getopt` 之前，遇 `--` 即停）。
    /// 这一段有反直觉的读法，所以每形都单独钉住：
    /// ①裸 `-a` 被改写成 `case '\01'`（`join.c:135-138`）——`aflag` 加**两个**unpair；
    /// ②正因为改写无条件，`join -a 3 f1 f2` 里那个 `3` **不再是 `-a` 的值**而是操作数，
    ///   于是三个操作数撞 `if (argc != 2) usage()`（`join.c:236-237`）——参照件那形零输出
    ///   崩在 `usage()` 里，与此同形；③`-j1`/`-j2` 改成 `-1`/`-2`，值取下一词（参照件
    ///   `join -j1 f1 f2` 报的就是 `-1 option field number less than 1`）；④畸形 `-j` 形
    ///   在选项环之前就让 `illegal option -- <整个 token>` 先出场。
    #[test]
    fn obsolete_legacy_forms_rewrite_before_getopt() {
        // ①裸 `-a`：两文件都出不配对，且不吞值。
        let bare = parse_join_args(&["-a", "f1", "f2"]).unwrap();
        assert!(bare.opts.unmatched1 && bare.opts.unmatched2);
        assert_eq!(bare.files, vec!["f1", "f2"]);
        // ②分开写的值成了操作数 → 三操数 → 裸 usage 那一支。
        assert_eq!(
            parse_join_args(&["-a", "3", "f1", "f2"]).err(),
            Some(JoinError::Usage)
        );
        // ③`-j1` 改成 `-1`：值取下一词，字段一是那一词转出来的（这里转不出→<1 那句）。
        assert_eq!(
            parse_join_args(&["-j1", "f1", "f2"]).err(),
            Some(JoinError::FieldLessThan1("-1"))
        );
        let conv = parse_join_args(&["-j2", "2", "f1", "f2"]).unwrap();
        assert_eq!(conv.opts.field2, 1, "`-j2 2` 等价于 `-2 2`");
        assert_eq!(conv.opts.field1, 0, "开始列没被 `-j2` 动过");
        // ④畸形 `-j` 形：整个 token 进句子，且它排在别的错之前。
        assert_eq!(
            parse_join_args(&["-j3", "f1", "f2"]).err(),
            Some(JoinError::IllegalOptionToken("-j3"))
        );
        assert_eq!(
            parse_join_args(&["-j12", "f1", "f2"]).err(),
            Some(JoinError::IllegalOptionToken("-j12"))
        );
        assert_eq!(
            parse_join_args(&["-j1", "-j3", "f1", "f2"]).err(),
            Some(JoinError::IllegalOptionToken("-j3")),
            "pre-pass 先扫完整条 argv，所以后面的畸形形赢过前面的改写"
        );
        // `--` 之后不再扫：那串畸形形落进操作数，报的是裸 usage。
        assert_eq!(
            parse_join_args(&["--", "-j3", "f1", "f2"]).err(),
            Some(JoinError::Usage)
        );
    }

    /// legacy `-o arg arg`：C 在 `obsolete()` 里把紧跟 `-o` 值的每个 `[12].<数字>` 项
    /// 逐个重写成 `-o<该项>`（`join.c:596-617`），而 `fieldarg` 每次都是**追加**
    /// （`join.c:540-542`），所以重复 `-o` 累加而不是覆盖。不像字段规格的项停止改写，
    /// 它自己与后面的项仍是操作数。
    #[test]
    fn legacy_o_lists_accumulate_instead_of_replacing() {
        let one = parse_join_args(&["-o", "1.1", "f1", "f2"]).unwrap();
        assert_eq!(one.opts.output_fields.len(), 1);
        let legacy = parse_join_args(&["-o", "1.1", "2.1", "f1", "f2"]).unwrap();
        assert_eq!(
            legacy.opts.output_fields.len(),
            2,
            "`-o 1.1 2.1` 是 legacy 的两条列表，`2.1` 不是操作数"
        );
        assert_eq!(legacy.files, vec!["f1", "f2"]);
        // 附值形 `-o1.1` 不触发改写（`join.c:601-602` 的 `ap[2] != '\0'` 直接 break），
        // 所以 `2.1` 仍是操作数——两个操作数刚好配得上，三操数就会撞裸 usage。
        let inline = parse_join_args(&["-o1.1", "2.1", "f1"]).unwrap();
        assert_eq!(inline.opts.output_fields.len(), 1);
        assert_eq!(inline.files, vec!["2.1", "f1"]);
        // 两个附值 `-o` 也累加：这是 C 的 `olistcnt++`，不是覆盖。
        let twice = parse_join_args(&["-o1.1", "-o2.1", "f1", "f2"]).unwrap();
        assert_eq!(twice.opts.output_fields.len(), 2);
    }

    #[test]
    fn parse_validations_and_mutual_exclusion() {
        // 非两个操数、表外字母、-a&-v 互斥、非法号、非单字节 tab。
        // 非两个操数走裸 usage；表外字母走「libc 那一行 + usage」，两者不同型。
        assert_eq!(parse_join_args(&["only1"]).err(), Some(JoinError::Usage));
        assert_eq!(
            parse_join_args(&["-z", "f1", "f2"]).err(),
            Some(JoinError::Getopt(GetoptReport::UnknownOption('z')))
        );
        // 带值选项缺值同样带字母（`-t` 到量尽头没有值）。
        assert_eq!(
            parse_join_args(&["f1", "f2", "-t"]).err(),
            Some(JoinError::Getopt(GetoptReport::MissingValue('t')))
        );
        assert_eq!(
            parse_join_args(&["-a1", "-v2", "f1", "f2"]).err(),
            Some(JoinError::AAndVMutuallyExclusive)
        );
        assert_eq!(
            parse_join_args(&["-1", "0", "f1", "f2"]).err(),
            Some(JoinError::FieldLessThan1("-1"))
        );
        assert_eq!(
            parse_join_args(&["-1", "2x", "f1", "f2"]).err(),
            Some(JoinError::IllegalFieldNumber("2x"))
        );
        assert_eq!(
            parse_join_args(&["-t", "ab", "f1", "f2"]).err(),
            Some(JoinError::BadTabChar)
        );
        assert_eq!(
            parse_join_args(&["-a3", "f1", "f2"]).err(),
            Some(JoinError::FileNumberNot12("-a"))
        );
        assert_eq!(
            parse_join_args(&["-j", "f1", "f2"]).err(),
            // C 的 `strtol("f1")` 转不出数字→值 0→**先过 `< 1` 那道守卫**，
            // 所以落「less than 1」而不是「illegal field number」（`join.c:184-189`，
            // 参照件同读数）——旧实现在这里归错了支，它先按「是否全数字」分流。
            Some(JoinError::FieldLessThan1("-j"))
        );
        // 一个数字也转不出来的值一律先落 `< 1` 支（同上形，三支都适用）。
        assert_eq!(
            parse_join_args(&["-1", "x", "f1", "f2"]).err(),
            Some(JoinError::FieldLessThan1("-1"))
        );
        // 而文件号那两支的判序是「先 switch 后查尾字符」：`1x` 的 1 已被 switch 认下，
        // 所以它走 illegal 那句，而不是 not-1-or-2（`:165-179`）。
        assert_eq!(
            parse_join_args(&["-a1x", "f1", "f2"]).err(),
            Some(JoinError::IllegalFileNumber("1x")),
            "附值形才走 file_no：分开的 `-a 1x` 会被 obsolete() 改写成裸 `-a`（见下行测）"
        );
        assert_eq!(
            parse_join_args(&["-v", "x", "f1", "f2"]).err(),
            Some(JoinError::FileNumberNot12("-v"))
        );
        // `-o` 的两支 `errx` 是两个错型（两句不同），且都不接 usage。
        assert_eq!(
            parse_join_args(&["-o", "1.0", "f1", "f2"]).err(),
            Some(JoinError::FieldNumbersAre1Based)
        );
        assert_eq!(
            parse_join_args(&["-o", "3.1", "f1", "f2"]).err(),
            Some(JoinError::MalformedOutputField)
        );
    }

    #[test]
    fn strtol_decimal_mirrors_the_no_digit_case() {
        // `strtol` 的四形：前导空白与符号算已消费；无数字时值为 0 且不报错。
        assert_eq!(strtol_decimal("12"), (12, 2));
        assert_eq!(strtol_decimal(" 12"), (12, 3));
        assert_eq!(strtol_decimal("+12"), (12, 3));
        assert_eq!(strtol_decimal("-3"), (-3, 2));
        assert_eq!(strtol_decimal("x"), (0, 0));
        assert_eq!(strtol_decimal("1x"), (1, 1));
        assert_eq!(strtol_decimal(""), (0, 0));
    }
}
